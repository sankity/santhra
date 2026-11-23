mod asr;
mod capture;

use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};
use caption_core::{CaptionConfig, CaptionEvent, Captions};
use clap::{Args, Parser, Subcommand};
use sherpa_onnx::Wave;

const DEFAULT_MODEL_DIR: &str = "models/sherpa-onnx-streaming-zipformer-en-20M-2023-02-17";

#[derive(Parser)]
#[command(
    name = "voxera-cli",
    version,
    about = "Voxera live caption PoC: offline system audio transcription"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Transcribe {
        wav: PathBuf,
        #[command(flatten)]
        common: Common,
    },
    Live {
        #[arg(
            long,
            help = "Sink node name or serial; default is the default sink's audio"
        )]
        target: Option<String>,
        #[command(flatten)]
        common: Common,
    },
}

#[derive(Args)]
struct Common {
    #[arg(long, default_value = DEFAULT_MODEL_DIR)]
    model_dir: PathBuf,

    #[arg(long, default_value_t = 100)]
    chunk_ms: u32,

    #[arg(long, default_value_t = 2)]
    threads: i32,

    #[arg(long, default_value = "cpu")]
    provider: String,

    #[arg(
        long,
        default_value_t = 2,
        help = "Final caption lines kept in the caption state"
    )]
    max_lines: usize,

    #[arg(long, help = "Only print final segments")]
    no_partial: bool,
}

impl Common {
    fn options(&self) -> asr::AsrOptions {
        asr::AsrOptions {
            chunk_ms: self.chunk_ms,
            threads: self.threads,
            provider: self.provider.clone(),
        }
    }

    fn captions(&self) -> Captions {
        Captions::new(CaptionConfig {
            max_lines: self.max_lines,
            show_partial: !self.no_partial,
        })
    }
}

fn render(event: Option<CaptionEvent>) {
    match event {
        Some(CaptionEvent::Final { text, .. }) => {
            print!("\r\x1b[K");
            println!("[final] {text}");
        }
        Some(CaptionEvent::Partial(text)) if !text.is_empty() => {
            print!("\r\x1b[K{text}");
            let _ = io::stdout().flush();
        }
        _ => {}
    }
}

fn feed(captions: &mut Captions, events: asr::Events) {
    render(captions.set_partial(&events.partial));
    for text in events.finals {
        render(captions.add_final(&text));
    }
}

fn transcribe_wav(wav: PathBuf, common: Common) -> Result<()> {
    let model = asr::ModelFiles::load(&common.model_dir)?;
    let wave = Wave::read(&wav.display().to_string())
        .with_context(|| format!("cannot read WAV: {}", wav.display()))?;
    let mut captions = common.captions();
    asr::transcribe(&wave, &model, &common.options(), |events| {
        feed(&mut captions, events)
    })
}

fn live(target: Option<String>, common: Common) -> Result<()> {
    let model = asr::ModelFiles::load(&common.model_dir)?;
    let mut transcriber = asr::Transcriber::new(&model, &common.options())?;
    let capture = capture::spawn(target)?;
    let mut captions = common.captions();

    for chunk in capture.rx.iter() {
        feed(&mut captions, transcriber.push(&chunk));
    }
    feed(&mut captions, transcriber.finish());
    print!("\r\x1b[K");

    let dropped = capture.dropped.load(std::sync::atomic::Ordering::Relaxed);
    if dropped > 0 {
        eprintln!("warning: dropped {dropped} audio chunks (ASR could not keep up)");
    }
    Ok(())
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Transcribe { wav, common } => transcribe_wav(wav, common),
        Command::Live { target, common } => live(target, common),
    }
}

fn main() {
    if let Err(err) = run(Cli::parse()) {
        eprintln!("error: {err:#}");
        std::process::exit(2);
    }
}
