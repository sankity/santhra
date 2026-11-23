use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;

use anyhow::Result;
use caption_core::{CaptionConfig, CaptionSnapshot, Captions};
use speech_sherpa::{AsrOptions, ModelFiles, Transcriber};

#[derive(Clone, Debug)]
pub struct SessionOptions {
    pub target: Option<String>,
    pub model_dir: PathBuf,
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
}

impl Session {
    pub fn start(opts: SessionOptions) -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let capture = audio_linux::spawn(opts.target.clone(), stop.clone())?;
        let dropped = capture.dropped.clone();
        let (tx, rx) = mpsc::channel();

        let asr = std::thread::Builder::new()
            .name("santhra-asr".into())
            .spawn(move || {
                let asr_opts = AsrOptions {
                    chunk_ms: 100,
                    threads: opts.threads,
                    provider: opts.provider.clone(),
                };
                let model = match ModelFiles::load(&opts.model_dir) {
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

impl Drop for Session {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.asr.take() {
            let _ = handle.join();
        }
    }
}
