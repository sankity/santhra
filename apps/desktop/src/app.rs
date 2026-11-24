use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use app_core::{Message, Session};
use caption_core::CaptionSnapshot;
use rune::tok::{self, type_};
use rune::{
    Color, Cx, El, Family, ScrollSpec, TextEdit, TextStyle, Waker, WindowHandle, col, el, row, text,
};

use crate::fonts;
use crate::settings::Settings;
use crate::widgets::button;

pub struct FontChoice {
    pub family: Option<&'static str>,
    pub size: f32,
}

pub struct Shared {
    pub settings: Mutex<Settings>,
    pub snapshot: Mutex<CaptionSnapshot>,
    pub revision: AtomicUsize,
    pub wakers: Mutex<Vec<(&'static str, Waker)>>,
    pub overlay: Mutex<Option<WindowHandle>>,
    pub settings_window: Mutex<Option<WindowHandle>>,
    pub sinks: Mutex<Option<Vec<audio_linux::Sink>>>,
    pub model_ok: AtomicBool,
    pub running: AtomicBool,
    pub font: Mutex<FontChoice>,
    pub repaint_until: Mutex<Option<Instant>>,
}

impl Shared {
    pub fn get() -> &'static Self {
        static SHARED: OnceLock<Shared> = OnceLock::new();
        SHARED.get_or_init(|| {
            let settings = Settings::load();
            let font = Mutex::new(FontChoice {
                family: settings.font_family.as_deref().map(leaked_family),
                size: settings.font_size,
            });
            let model_ok = speech_sherpa::ModelFiles::load(Path::new(&settings.model_dir)).is_ok();
            Shared {
                settings: Mutex::new(settings),
                snapshot: Mutex::new(CaptionSnapshot::default()),
                revision: AtomicUsize::new(0),
                wakers: Mutex::new(Vec::new()),
                overlay: Mutex::new(None),
                settings_window: Mutex::new(None),
                sinks: Mutex::new(None),
                model_ok: AtomicBool::new(model_ok),
                running: AtomicBool::new(false),
                repaint_until: Mutex::new(None),
                font,
            }
        })
    }

    pub fn apply_settings(&self) {
        let settings = self.settings();
        let ok = speech_sherpa::ModelFiles::load(Path::new(&settings.model_dir)).is_ok();
        self.model_ok.store(ok, Ordering::Relaxed);
        let _ = settings.save();
        *self.font.lock().unwrap() = FontChoice {
            family: settings.font_family.as_deref().map(leaked_family),
            size: settings.font_size,
        };
        crate::apply_overlay_options();
        crate::set_overlay_enabled(settings.overlay_enabled);
        self.publish();
    }

    pub fn wake(&self) {
        for (_, waker) in self.wakers.lock().unwrap().iter() {
            waker();
        }
    }

    pub fn publish(&self) {
        self.revision.fetch_add(1, Ordering::Release);
        *self.repaint_until.lock().unwrap() = Some(Instant::now() + REPAINT_BURST);
        self.wake();
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    pub fn caption_style(&self, italic: bool) -> TextStyle {
        let font = self.font.lock().unwrap();
        let family = match font.family {
            Some(name) => Family::Named(name),
            None => Family::Sans,
        };
        let mut style = TextStyle::new(font.size, 500.0).family(family);
        if italic {
            style = style.italic();
        }
        style
    }
}

const REPAINT_BURST: Duration = Duration::from_millis(500);

/// `Family::Named` needs a static string. One small leak per font change is the ceiling.
// ponytail: one leak per font change; intern the names if a long session ever changes fonts thousands of times.
pub fn leaked_family(name: &str) -> &'static str {
    Box::leak(name.to_owned().into_boxed_str())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Picker {
    Font,
    Source,
}

pub struct State {
    pub id: &'static str,
    pub picker: Option<Picker>,
    pub seen: usize,
    pub session: Option<Session>,
    pub status: String,
    pub model_dir: TextEdit,
    pub text_hex: TextEdit,
    pub families: Vec<String>,
}

impl State {
    pub fn new(id: &'static str) -> Self {
        let settings = Shared::get().settings();
        State {
            id,
            picker: None,
            seen: 0,
            session: None,
            status: "Idle".into(),
            model_dir: TextEdit::new(settings.model_dir.clone()),
            text_hex: TextEdit::new(settings.text_color.clone()),
            families: fonts::installed(),
        }
    }
}

pub fn setup(s: &mut State, waker: Waker) {
    let shared = Shared::get();
    let mut wakers = shared.wakers.lock().unwrap();
    if !wakers.iter().any(|(id, _)| *id == s.id) {
        wakers.push((s.id, waker));
    }
}

fn seen_revision(s: &mut State) -> bool {
    let revision = Shared::get().revision.load(Ordering::Acquire);
    let changed = revision != s.seen;
    s.seen = revision;
    changed
}

pub fn poll_main(s: &mut State, _: f64, tx: &mut rune::Tx) -> bool {
    let mut message_changed = false;
    if let Some(session) = &s.session {
        let mut messages = Vec::new();
        while let Some(message) = session.try_recv() {
            messages.push(message);
        }
        for message in messages {
            match message {
                Message::Captions(snapshot) => {
                    let mut current = Shared::get().snapshot.lock().unwrap();
                    if *current != snapshot {
                        *current = snapshot;
                        message_changed = true;
                    }
                }
                Message::Stopped { dropped } => {
                    message_changed = true;
                    s.status = if dropped > 0 {
                        format!("Idle, {dropped} audio chunks dropped")
                    } else {
                        "Idle".into()
                    };
                    s.session = None;
                    Shared::get().running.store(false, Ordering::Relaxed);
                }
                Message::Failed(err) => {
                    message_changed = true;
                    s.status = format!("error: {err}");
                    s.session = None;
                    Shared::get().running.store(false, Ordering::Relaxed);
                }
            }
        }
        if s.session.is_some() {
            tx.wake_after(Duration::from_millis(100));
        }
    }
    if message_changed {
        Shared::get().publish();
    }
    let external = seen_revision(s);
    message_changed || external
}

pub fn poll_readonly(s: &mut State, _: f64, tx: &mut rune::Tx) -> bool {
    let shared = Shared::get();
    let repainting = {
        let mut until = shared.repaint_until.lock().unwrap();
        match *until {
            Some(deadline) if Instant::now() < deadline => true,
            Some(_) => {
                *until = None;
                false
            }
            None => false,
        }
    };
    if repainting || shared.running.load(Ordering::Relaxed) {
        tx.wake_after(Duration::from_millis(100));
    }
    seen_revision(s)
}

pub fn start_session(s: &mut State) {
    if s.session.is_some() {
        return;
    }
    let settings = Shared::get().settings();
    if let Err(err) = settings.save() {
        s.status = format!("cannot save settings: {err:#}");
    }
    match Session::start(settings.session_options()) {
        Ok(session) => {
            s.session = Some(session);
            s.status = "Listening".into();
            Shared::get().running.store(true, Ordering::Relaxed);
        }
        Err(err) => {
            s.status = format!("cannot start: {err:#}");
            Shared::get().running.store(false, Ordering::Relaxed);
        }
    }
    Shared::get().wake();
}

pub fn stop_session(s: &mut State) {
    if let Some(session) = &s.session {
        session.request_stop();
        s.status = "Stopping".into();
        Shared::get().wake();
    }
}

pub fn toggle_overlay(s: &mut State) {
    let enabled = !Shared::get().settings().overlay_enabled;
    crate::set_overlay_enabled(enabled);
    let _ = s;
}

pub fn open_settings(s: &mut State) {
    crate::open_settings_window();
    let _ = s;
}

pub fn refresh_model(s: &mut State) {
    let shared = Shared::get();
    let settings = shared.settings();
    let ok = speech_sherpa::ModelFiles::load(Path::new(&settings.model_dir)).is_ok();
    shared.model_ok.store(ok, Ordering::Relaxed);
    *shared.sinks.lock().unwrap() = Some(audio_linux::list_sinks());
    shared.publish();
    let _ = s;
}

pub fn view_main(s: &State, _: &mut Cx<State>) -> El {
    let t = tok::theme(true);
    let shared = Shared::get();
    let settings = shared.settings();
    let snapshot = shared.snapshot.lock().unwrap().clone();
    let running = s.session.is_some();
    let model_ok = shared.model_ok.load(Ordering::Relaxed);
    let failed = !model_ok || s.status.starts_with("error") || s.status.starts_with("cannot");
    let dot = if running {
        Color {
            r: 0.30,
            g: 0.69,
            b: 0.32,
            a: 1.0,
        }
    } else if failed {
        Color {
            r: 0.85,
            g: 0.20,
            b: 0.20,
            a: 1.0,
        }
    } else {
        Color {
            r: 0.50,
            g: 0.50,
            b: 0.50,
            a: 1.0,
        }
    };
    let bg = t.bg;

    let mut captions = col().w_full().gap(6.0);
    if snapshot.finals.is_empty() && snapshot.partial.is_empty() {
        captions = captions.child(
            text("No captions yet. Press Start, then play something.")
                .font(type_::BODY)
                .color(rgba(settings.text_rgba())),
        );
    } else {
        for line in &snapshot.finals {
            captions = captions.child(caption_line(&line.text, shared, false, &settings));
        }
        if settings.show_partial && !snapshot.partial.is_empty() {
            captions = captions.child(caption_line(&snapshot.partial, shared, true, &settings));
        }
    }

    col()
        .key("main")
        .w_full()
        .h_full()
        .fill(bg)
        .child(
            row()
                .key("toolbar")
                .h(44.0)
                .w_full()
                .gap(8.0)
                .center_y()
                .px(12.0)
                .child(button("Start", start_session))
                .child(button("Stop", stop_session))
                .child(button(
                    if settings.overlay_enabled {
                        "Overlay: on"
                    } else {
                        "Overlay: off"
                    },
                    toggle_overlay,
                ))
                .child(el().size(8.0, 8.0).radius(4.0).fill(dot))
                .child(text(&s.status).font(type_::LABEL).color(t.text))
                .child(if model_ok {
                    el()
                } else {
                    text("model files not found").font(type_::LABEL).color(dot)
                })
                .child(button("Settings", open_settings)),
        )
        .child(
            col()
                .key("captions")
                .grow(1.0)
                .w_full()
                .px(16.0)
                .pb(16.0)
                .scroll(ScrollSpec { stick_bottom: true })
                .clip_rounded(0.0)
                .child(captions),
        )
}

fn caption_line(content: &str, shared: &Shared, italic: bool, settings: &Settings) -> El {
    text(content.to_owned())
        .wrap()
        .font(shared.caption_style(italic))
        .color(rgba(settings.text_rgba()))
}

pub fn rgba(rgba: [u8; 4]) -> Color {
    Color {
        r: f32::from(rgba[0]) / 255.0,
        g: f32::from(rgba[1]) / 255.0,
        b: f32::from(rgba[2]) / 255.0,
        a: f32::from(rgba[3]) / 255.0,
    }
}

pub fn background_rgba(settings: &Settings) -> Color {
    let [r, g, b, a] = settings.background_rgba();
    let a = (f32::from(a) * settings.overlay_opacity.clamp(0.1, 1.0)).round() as u8;
    rgba([r, g, b, a])
}

pub fn on_close() {
    let shared = Shared::get();
    if let Some(handle) = shared.overlay.lock().unwrap().clone() {
        handle.close();
    }
    if let Some(handle) = shared.settings_window.lock().unwrap().clone() {
        handle.close();
    }
    let _ = shared.settings.lock().unwrap().save();
}

/// Headless tooling scene: caption text without a running audio/ASR session.
pub fn scene_captions(_: &mut State) {
    use caption_core::CaptionLine;

    let shared = Shared::get();
    *shared.snapshot.lock().unwrap() = CaptionSnapshot {
        partial: String::new(),
        finals: vec![
            CaptionLine {
                id: 1,
                text: "the first caption line".into(),
            },
            CaptionLine {
                id: 2,
                text: "AFTER EARLY NIGHTFALL THE YELLOW LAMPS WOULD LIGHT UP HERE AND THERE".into(),
            },
        ],
    };
    *shared.font.lock().unwrap() = FontChoice {
        family: Some(leaked_family("DejaVu Serif")),
        size: 32.0,
    };
    shared.revision.fetch_add(1, Ordering::Relaxed);
}
