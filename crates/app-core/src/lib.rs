use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;

use anyhow::Result;
use audio_linux::{AppStream, Target};
use caption_core::{CaptionConfig, CaptionSnapshot, Captions};
use speech_sherpa::{AsrOptions, ModelFiles, ModelKind, Transcriber};

#[derive(Clone, Debug)]
pub struct SessionOptions {
    pub target: Target,
    pub model_dir: PathBuf,
    pub model_kind: String,
    pub buffer_ms: u32,
    pub language: Option<String>,
    pub threads: i32,
    pub provider: String,
    pub max_lines: usize,
    pub show_partial: bool,
}

pub enum Message {
    Captions(CaptionSnapshot),
    Stopped { dropped: u64 },
    Failed(String),
}

pub struct Session {
    rx: mpsc::Receiver<Message>,
    stop: Arc<AtomicBool>,
    asr: Option<JoinHandle<()>>,
    source_label: String,
    fell_back: bool,
}

impl Session {
    /// The source actually captured, for display in the UI.
    pub fn source_label(&self) -> &str {
        &self.source_label
    }

    /// True when the requested app stream was gone and the default sink was used instead.
    pub fn fell_back(&self) -> bool {
        self.fell_back
    }

    pub fn start(opts: SessionOptions) -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let apps = audio_linux::list_targets().apps;
        let mut target = audio_linux::resolve_target(opts.target.clone(), &apps);
        let mut fell_back = target != opts.target;

        let capture = match audio_linux::spawn(target.clone(), stop.clone()) {
            Ok(capture) => capture,
            // The app stream vanished after resolution: fall back to the default sink.
            Err(_) if matches!(target, Target::App(_)) => {
                fell_back = true;
                target = Target::Default;
                audio_linux::spawn(target.clone(), stop.clone())?
            }
            Err(err) => return Err(err),
        };
        let source_label = label_for(&target, &apps);
        let dropped = capture.dropped.clone();
        let (tx, rx) = mpsc::channel();

        let asr = std::thread::Builder::new()
            .name("santhra-asr".into())
            .spawn(move || {
                let asr_opts = AsrOptions {
                    chunk_ms: 100,
                    threads: opts.threads,
                    provider: opts.provider.clone(),
                    buffer_ms: opts.buffer_ms,
                    language: opts.language.clone(),
                };
                let kind =
                    ModelKind::parse(&opts.model_kind).unwrap_or(ModelKind::ZipformerTransducer);
                let model = match ModelFiles::load_kind(&opts.model_dir, kind) {
                    Ok(model) => model,
                    Err(err) => {
                        let _ = tx.send(Message::Failed(format!("{err:#}")));
                        return;
                    }
                };
                let mut transcriber = match Transcriber::new(&model, &asr_opts) {
                    Ok(transcriber) => transcriber,
                    Err(err) => {
                        let _ = tx.send(Message::Failed(format!("{err:#}")));
                        return;
                    }
                };
                let mut captions = Captions::new(CaptionConfig {
                    max_lines: opts.max_lines,
                    show_partial: opts.show_partial,
                });
                let mut feed = |events: speech_sherpa::Events| {
                    if let Some(partial) = &events.partial {
                        captions.set_partial(partial);
                    }
                    for text in events.finals {
                        captions.add_final(&text);
                    }
                    let _ = tx.send(Message::Captions(captions.snapshot()));
                };
                for chunk in capture.rx.iter() {
                    feed(transcriber.push(&chunk));
                }
                feed(transcriber.finish());
                let _ = tx.send(Message::Stopped {
                    dropped: dropped.load(Ordering::Relaxed),
                });
            })?;

        Ok(Self {
            rx,
            stop,
            asr: Some(asr),
            source_label,
            fell_back,
        })
    }

    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    pub fn try_recv(&self) -> Option<Message> {
        self.rx.try_recv().ok()
    }

    pub fn messages(&self) -> impl Iterator<Item = Message> + '_ {
        self.rx.iter()
    }
}

fn label_for(target: &Target, apps: &[AppStream]) -> String {
    match target {
        Target::Default => "Default sink".into(),
        Target::Sink(name) => name.clone(),
        Target::App(serial) => {
            let label = apps
                .iter()
                .find(|app| app.serial == *serial)
                .map(|app| {
                    if app.title.is_empty() {
                        app.app.clone()
                    } else {
                        format!("{} — {}", app.app, app.title)
                    }
                })
                .unwrap_or_else(|| format!("app:{serial}"));
            if label.chars().count() > 52 {
                format!("{}…", label.chars().take(51).collect::<String>())
            } else {
                label
            }
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.asr.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_label_uses_tab_title_or_sink_name() {
        let apps = vec![AppStream {
            serial: 7,
            app: "ExampleApp".into(),
            title: "A tab".into(),
        }];
        assert_eq!(label_for(&Target::Default, &apps), "Default sink");
        assert_eq!(label_for(&Target::Sink("sink".into()), &apps), "sink");
        assert_eq!(label_for(&Target::App(7), &apps), "ExampleApp — A tab");
        assert_eq!(label_for(&Target::App(9), &apps), "app:9");
    }
}
