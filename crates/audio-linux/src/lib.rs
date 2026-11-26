use std::cell::{Cell, RefCell};
use std::process::Command;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use anyhow::{Result, anyhow};
use pipewire as pw;
use pw::properties::properties;
use pw::stream::StreamState;
use pw::spa;
use serde_json::Value;
use spa::param::audio::{AudioFormat, AudioInfoRaw};
use spa::param::format::{MediaSubtype, MediaType};
use spa::param::format_utils;

pub const SAMPLE_RATE: u32 = 16_000;
const QUEUE_CHUNKS: usize = 64;

pub struct Capture {
    pub rx: mpsc::Receiver<Vec<f32>>,
    pub dropped: Arc<AtomicU64>,
}

/// What a capture stream listens to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// Default sink monitor (whole system mix).
    Default,
    /// A specific sink's monitor, by `node.name`.
    Sink(String),
    /// A single application stream, by `object.serial` (one browser tab, one app).
    App(u32),
}

impl Target {
    /// Decode the `settings.target` / CLI string form. Absent = default sink.
    pub fn from_key(key: Option<&str>) -> Target {
        match key {
            None => Target::Default,
            Some(key) => match key.strip_prefix("app:") {
                Some(serial) => match serial.parse() {
                    Ok(serial) => Target::App(serial),
                    Err(_) => Target::Sink(key.to_owned()),
                },
                None => Target::Sink(key.to_owned()),
            },
        }
    }

    /// Encode back to the string form (`None` for the default sink).
    pub fn key(&self) -> Option<String> {
        match self {
            Target::Default => None,
            Target::Sink(name) => Some(name.clone()),
            Target::App(serial) => Some(format!("app:{serial}")),
        }
    }

    /// The sink this target captures; `None` for the default sink and app streams.
    pub fn sink_name(&self) -> Option<&str> {
        match self {
            Target::Sink(name) => Some(name),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppStream {
    pub serial: u32,
    pub app: String,
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sink {
    pub name: String,
    pub description: String,
}

#[derive(Clone, Debug, Default)]
pub struct Targets {
    pub sinks: Vec<Sink>,
    pub apps: Vec<AppStream>,
}

/// Application streams as seen by pipewire-pulse. `pactl` exposes `media.name`
/// (the tab title) next to `object.serial`; registry globals do not.
fn list_apps() -> Vec<AppStream> {
    let Ok(output) = Command::new("pactl")
        .args(["-f", "json", "list", "sink-inputs"])
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    parse_apps(&String::from_utf8_lossy(&output.stdout))
}

fn parse_apps(json: &str) -> Vec<AppStream> {
    let Ok(items) = serde_json::from_str::<Vec<Value>>(json) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let props = item.get("properties");
            let text = |key: &str| -> String {
                props
                    .and_then(|p| p.get(key))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned()
            };
            let serial = text("object.serial").parse::<u32>().ok()?;
            let app = text("application.name");
            let app = if app.is_empty() { text("node.name") } else { app };
            if app.is_empty() {
                return None;
            }
            Some(AppStream {
                serial,
                app,
                title: text("media.name"),
            })
        })
        .collect()
}

/// An app target that no longer exists falls back to the default sink.
pub fn resolve_target(target: Target, apps: &[AppStream]) -> Target {
    match target {
        Target::App(serial) if apps.iter().any(|app| app.serial == serial) => Target::App(serial),
        Target::App(_) => Target::Default,
        other => other,
    }
}

pub fn spawn(target: Target, stop: Arc<AtomicBool>) -> Result<Capture> {
    let target = resolve_target(target, &list_targets().apps);
    let (tx, rx) = mpsc::sync_channel(QUEUE_CHUNKS);
    let dropped = Arc::new(AtomicU64::new(0));
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();

    let thread_dropped = dropped.clone();
    std::thread::Builder::new()
        .name("pipewire-capture".into())
        .spawn(move || {
            if let Err(err) = run(tx, thread_dropped, target, stop, ready_tx.clone()) {
                let _ = ready_tx.send(Err(err.to_string()));
            }
        })?;

    ready_rx
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| anyhow!("pipewire capture did not start within 5 s"))?
        .map_err(|err| anyhow!("pipewire: {err}"))?;
    Ok(Capture { rx, dropped })
}

fn run(
    tx: mpsc::SyncSender<Vec<f32>>,
    dropped: Arc<AtomicU64>,
    target: Target,
    stop: Arc<AtomicBool>,
    ready: mpsc::Sender<Result<(), String>>,
) -> Result<(), pw::Error> {
    pw::init();

    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;

    let mut props = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Music",
    };
    // App targets need the stream state to report a vanished node: WirePlumber would
    // otherwise silently fall back to the default source (the microphone).
    let state_gated = matches!(target, Target::App(_));
    match &target {
        Target::Default => {
            props.insert(*pw::keys::STREAM_CAPTURE_SINK, "true");
        }
        Target::Sink(name) => {
            props.insert(*pw::keys::STREAM_CAPTURE_SINK, "true");
            props.insert(*pw::keys::TARGET_OBJECT, name.as_str());
        }
        Target::App(serial) => {
            props.insert(*pw::keys::TARGET_OBJECT, serial.to_string());
            // No pipewire-rs constant for this key; set it literally.
            props.insert("node.dont-fallback", "true");
        }
    }

    let stream = pw::stream::StreamBox::new(&core, "santhra-capture", props)?;

    let ready_state = ready.clone();
    let _listener = stream
        .add_local_listener_with_user_data(AudioInfoRaw::default())
        .state_changed(move |_, _, _, new| {
            if !state_gated {
                return;
            }
            match new {
                StreamState::Paused | StreamState::Streaming => {
                    let _ = ready_state.send(Ok(()));
                }
                StreamState::Error(msg) => {
                    let _ = ready_state.send(Err(msg.clone()));
                }
                _ => {}
            }
        })
        .param_changed(|_, info, id, param| {
            let Some(param) = param else { return };
            if id != spa::param::ParamType::Format.as_raw() {
                return;
            }
            let Ok((media_type, media_subtype)) = format_utils::parse_format(param) else {
                return;
            };
            if media_type == MediaType::Audio && media_subtype == MediaSubtype::Raw {
                let _ = info.parse(param);
            }
        })
        .process(move |stream, info| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let Some(data) = buffer.datas_mut().first_mut() else {
                return;
            };
            let size = data.chunk().size() as usize;
            let Some(bytes) = data.data() else { return };
            let samples = to_mono(
                &bytes[..size.min(bytes.len())],
                info.channels().max(1) as usize,
            );
            if tx.try_send(samples).is_err() {
                dropped.fetch_add(1, Ordering::Relaxed);
            }
        })
        .register()?;

    let mut audio_info = AudioInfoRaw::new();
    audio_info.set_format(AudioFormat::F32LE);
    audio_info.set_rate(SAMPLE_RATE);
    audio_info.set_channels(1);
    let object = pw::spa::pod::Object {
        type_: pw::spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
        id: pw::spa::param::ParamType::EnumFormat.as_raw(),
        properties: audio_info.into(),
    };
    let values: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(object),
    )
    .expect("serialize audio format")
    .0
    .into_inner();
    let mut params = [spa::pod::Pod::from_bytes(&values).expect("valid audio format pod")];

    stream.connect(
        spa::utils::Direction::Input,
        None,
        pw::stream::StreamFlags::AUTOCONNECT
            | pw::stream::StreamFlags::MAP_BUFFERS
            | pw::stream::StreamFlags::RT_PROCESS,
        &mut params,
    )?;

    if !state_gated {
        ready.send(Ok(())).ok();
    }
    while !stop.load(Ordering::Relaxed) {
        mainloop
            .loop_()
            .iterate(pw::loop_::Timeout::Finite(Duration::from_millis(200)));
    }
    Ok(())
}

/// Enumerate capture targets: sinks and application streams.
pub fn list_targets() -> Targets {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(enumerate_targets());
    });
    rx.recv_timeout(Duration::from_secs(2)).unwrap_or_default()
}

/// Sinks only; kept for callers/tests that predate application streams.
pub fn list_sinks() -> Vec<Sink> {
    list_targets().sinks
}

fn enumerate_targets() -> Targets {
    let Ok(mainloop) = pw::main_loop::MainLoopRc::new(None) else {
        return Targets::default();
    };
    let Ok(context) = pw::context::ContextRc::new(&mainloop, None) else {
        return Targets::default();
    };
    let Ok(core) = context.connect_rc(None) else {
        return Targets::default();
    };
    let Ok(registry) = core.get_registry() else {
        return Targets::default();
    };
    let Ok(pending) = core.sync(0) else {
        return Targets::default();
    };

    let targets = Rc::new(RefCell::new(Targets::default()));
    let done = Rc::new(Cell::new(false));

    let _core_listener = core
        .add_listener_local()
        .done({
            let done = done.clone();
            let mainloop = mainloop.clone();
            move |id, seq| {
                if id == pw::core::PW_ID_CORE && seq == pending {
                    done.set(true);
                    mainloop.quit();
                }
            }
        })
        .register();

    let _registry_listener = registry
        .add_listener_local()
        .global({
            let targets = targets.clone();
            move |global| {
                let Some(props) = global.props else { return };
                if let Some("Audio/Sink") = props.get(*pw::keys::MEDIA_CLASS) {
                    let Some(name) = props.get(*pw::keys::NODE_NAME) else {
                        return;
                    };
                    let description = props
                        .get(*pw::keys::NODE_DESCRIPTION)
                        .or_else(|| props.get(*pw::keys::NODE_NICK))
                        .unwrap_or(name);
                    targets.borrow_mut().sinks.push(Sink {
                        name: name.to_string(),
                        description: description.to_string(),
                    });
                }
            }
        })
        .register();

    mainloop.run();
    let mut targets = targets.borrow().clone();
    let _ = done.get();
    targets.apps = list_apps();
    targets
}

pub fn to_mono(bytes: &[u8], channels: usize) -> Vec<f32> {
    let samples: Vec<f32> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect();
    let channels = channels.max(1);
    if channels == 1 {
        return samples;
    }
    samples
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f32_bytes(values: &[f32]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    #[test]
    fn to_mono_passthrough_and_average() {
        assert_eq!(to_mono(&f32_bytes(&[0.5, -0.5]), 1), vec![0.5, -0.5]);
        let stereo = f32_bytes(&[0.5, -0.5, 1.0, 0.0]);
        assert_eq!(to_mono(&stereo, 2), vec![0.0, 0.5]);
    }

    #[test]
    fn to_mono_ignores_trailing_partial_sample() {
        assert_eq!(
            to_mono(&[1, 2, 3, 4, 5], 1),
            vec![f32::from_le_bytes([1, 2, 3, 4])]
        );
    }

    #[test]
    fn target_key_roundtrip() {
        assert_eq!(Target::from_key(None), Target::Default);
        assert_eq!(Target::from_key(Some("default")), Target::Sink("default".into()));
        assert_eq!(Target::key(&Target::Default), None);
        for target in [Target::Sink("test-sink".into()), Target::App(645)] {
            assert_eq!(Target::from_key(target.key().as_deref()), target);
        }
        assert_eq!(Target::App(645).key().as_deref(), Some("app:645"));
        assert_eq!(Target::Sink("s".into()).sink_name(), Some("s"));
        assert_eq!(Target::Default.sink_name(), None);
    }

    #[test]
    fn parses_app_streams_from_pactl() {
        let apps = parse_apps(
            r#"[
              {"index":405,"properties":{"node.name":"app-a","media.name":"audio-a"}},
              {"index":1213,"properties":{"object.serial":"1213","application.name":"ExampleApp","media.name":"Example tab title"}},
              {"index":1288,"properties":{"object.serial":"1288","node.name":"player","media.name":"clip"}}
            ]"#,
        );
        assert_eq!(apps.len(), 2, "entries without object.serial are skipped");
        assert_eq!(
            apps[0],
            AppStream {
                serial: 1213,
                app: "ExampleApp".into(),
                title: "Example tab title".into(),
            }
        );
        assert_eq!(apps[1].app, "player");
    }

    #[test]
    fn resolve_missing_app_falls_back_to_default() {
        let apps = vec![AppStream {
            serial: 645,
            app: "ExampleApp".into(),
            title: "a".into(),
        }];
        assert_eq!(resolve_target(Target::App(645), &apps), Target::App(645));
        assert_eq!(resolve_target(Target::App(999), &apps), Target::Default);
        assert_eq!(
            resolve_target(Target::Sink("s".into()), &apps),
            Target::Sink("s".into())
        );
        assert_eq!(resolve_target(Target::Default, &apps), Target::Default);
    }

    #[test]
    #[ignore = "needs a running PipeWire session"]
    fn lists_sinks() {
        let sinks = list_sinks();
        assert!(!sinks.is_empty(), "no Audio/Sink found");
        assert!(
            sinks
                .iter()
                .all(|sink| !sink.name.is_empty() && !sink.description.is_empty())
        );
    }

    #[test]
    #[ignore = "needs a running PipeWire session"]
    fn missing_app_target_falls_back_to_default_sink() {
        let stop = Arc::new(AtomicBool::new(false));
        let capture = spawn(Target::App(u32::MAX), stop.clone()).expect("fallback to default sink");
        stop.store(true, Ordering::Relaxed);
        drop(capture);
    }

    #[test]
    #[ignore = "needs a running PipeWire session with an app playing audio"]
    fn captures_an_application_stream() {
        let apps = list_targets().apps;
        let Some(app) = apps.first() else {
            eprintln!("no application stream playing; skipping");
            return;
        };
        let stop = Arc::new(AtomicBool::new(false));
        let capture = spawn(Target::App(app.serial), stop.clone()).expect("capture app stream");
        stop.store(true, Ordering::Relaxed);
        drop(capture);
    }
}
