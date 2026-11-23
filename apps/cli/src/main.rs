use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use app_core::{Message, Session, SessionOptions};
use caption_core::{CaptionConfig, CaptionSnapshot, Captions};
use clap::{Args, Parser, Subcommand};
use sherpa_onnx::Wave;
use speech_sherpa::{AsrOptions, ModelFiles};

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
    fn options(&self) -> AsrOptions {
        AsrOptions {
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

    fn session_options(&self, target: Option<String>) -> SessionOptions {
        SessionOptions {
            target,
            model_dir: self.model_dir.clone(),
            threads: self.threads,
            provider: self.provider.clone(),
            max_lines: self.max_lines,
            show_partial: !self.no_partial,
        }
    }
}

#[derive(Default)]
struct Printer {
    last_final: u64,
    partial: String,
}

impl Printer {
    fn show(&mut self, snapshot: &CaptionSnapshot) {
        for line in &snapshot.finals {
            if line.id > self.last_final {
                print!("\r\x1b[K");
                println!("[final] {}", line.text);
                self.last_final = line.id;
            }
        }
        if snapshot.partial != self.partial {
            if snapshot.partial.is_empty() {
                print!("\r\x1b[K");
            } else {
                print!("\r\x1b[K{}", snapshot.partial);
            }
            let _ = io::stdout().flush();
            self.partial = snapshot.partial.clone();
        }
    }
}

fn transcribe_wav(wav: PathBuf, common: Common) -> Result<()> {
    let model = ModelFiles::load(&common.model_dir)?;
    let wave = Wave::read(&wav.display().to_string())
        .with_context(|| format!("cannot read WAV: {}", wav.display()))?;
    let mut captions = common.captions();
    let mut printer = Printer::default();
    speech_sherpa::transcribe(&wave, &model, &common.options(), |events| {
        captions.set_partial(&events.partial);
        for text in events.finals {
            captions.add_final(&text);
        }
        printer.show(&captions.snapshot());
    })
}

fn live(target: Option<String>, common: Common) -> Result<()> {
    let session = Session::start(common.session_options(target))?;
    let mut printer = Printer::default();
    for message in session.messages() {
        match message {
            Message::Captions(snapshot) => printer.show(&snapshot),
            Message::Stopped { dropped } => {
                print!("\r\x1b[K");
                if dropped > 0 {
                    eprintln!("warning: dropped {dropped} audio chunks (ASR could not keep up)");
                }
            }
            Message::Failed(err) => bail!(err),
        }
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
