mod asr;

use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use sherpa_onnx::Wave;

#[derive(Parser)]
#[command(
    name = "voxera-cli",
    version,
    about = "Voxera ASR PoC: transcribe a WAV file offline"
)]
struct Args {
    /// 16 kHz mono WAV file to transcribe
    wav: PathBuf,

    /// Directory containing the sherpa-onnx model files
    #[arg(
        long,
        default_value = "models/sherpa-onnx-streaming-zipformer-en-20M-2023-02-17"
    )]
    model_dir: PathBuf,

    /// Audio chunk size fed to the streaming recognizer
    #[arg(long, default_value_t = 100)]
    chunk_ms: u32,

    #[arg(long, default_value_t = 2)]
    threads: i32,

    #[arg(long, default_value = "cpu")]
    provider: String,

    /// Only print final segments
    #[arg(long)]
    no_partial: bool,
}

fn run(args: Args) -> Result<()> {
    let model = asr::ModelFiles::load(&args.model_dir)?;
    let wave = Wave::read(&args.wav.display().to_string())
        .with_context(|| format!("cannot read WAV: {}", args.wav.display()))?;

    asr::transcribe(
        &wave,
        &model,
        &asr::AsrOptions {
            chunk_ms: args.chunk_ms,
            threads: args.threads,
            provider: args.provider,
        },
        |text| {
            if !args.no_partial {
                print!("\r\x1b[K{}", text.trim());
                let _ = io::stdout().flush();
            }
        },
        |text| {
            if text.trim().is_empty() {
                return;
            }
            print!("\r\x1b[K");
            println!("[final] {}", text.trim());
        },
    )
}

fn main() {
    if let Err(err) = run(Args::parse()) {
        eprintln!("error: {err:#}");
        std::process::exit(2);
    }
}
