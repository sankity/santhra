use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use app_core::{Message, Session, SessionOptions};
use caption_core::{CaptionConfig, CaptionSnapshot, Captions};
use clap::{Args, Parser, Subcommand};
use santhra_download::{Progress, download_all};
use santhra_models::{
    ModelKind as CatalogKind, default_for_id, entry, model_dir, models_root, speech_language,
    whisper_language_code,
};
use sherpa_onnx::Wave;
use speech_sherpa::{AsrOptions, ModelFiles, ModelKind};

const DEFAULT_MODEL_DIR: &str = "models/sherpa-onnx-streaming-zipformer-en-20M-2023-02-17";

#[derive(Parser)]
#[command(
    name = "santhra-cli",
    version,
    about = "Santhra live caption PoC: offline system audio transcription"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Models {
        #[command(subcommand)]
        action: ModelsCommand,
    },
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

#[derive(Subcommand)]
enum ModelsCommand {
    List {
        #[arg(long)]
        language: Option<String>,
    },
    Download {
        id: String,
    },
    Verify {
        id: String,
    },
    Root,
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
        help = "Speech language id from Santhra's supported recognition language list"
    )]
    language: Option<String>,

    #[arg(
        long,
        help = "Model id from `models list`; defaults to the legacy --model-dir"
    )]
    model: Option<String>,

    #[arg(
        long,
        default_value_t = 2500,
        help = "Buffer size for offline models, in ms"
    )]
    buffer_ms: u32,

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
    fn selected(&self) -> Result<Option<&'static santhra_models::ModelEntry>> {
        if let Some(id) = self.language.as_deref()
            && speech_language(id).is_none()
        {
            bail!("unknown language id: {id}");
        }
        if let Some(id) = &self.model {
            let selected = entry(id).with_context(|| format!("unknown model id: {id}"))?;
            if let Some(language) = self.language.as_deref()
                && !santhra_models::matches_language_id(selected, language)
            {
                bail!("model {id} does not support language {language}");
            }
            return Ok(Some(selected));
        }
        self.language
            .as_deref()
            .map(|id| default_for_id(id).with_context(|| format!("unknown language id: {id}")))
            .transpose()
    }

    fn model_dir(&self) -> Result<PathBuf> {
        match self.selected()? {
            Some(item) => model_dir(item.id),
            None => Ok(self.model_dir.clone()),
        }
    }

    fn model_kind(&self) -> ModelKind {
        match self.selected() {
            Ok(Some(item)) => match item.kind {
                CatalogKind::ZipformerTransducer => ModelKind::ZipformerTransducer,
                CatalogKind::ParaformerOnline => ModelKind::ParaformerOnline,
                CatalogKind::ParaformerOffline => ModelKind::ParaformerOffline,
                CatalogKind::WhisperOffline => ModelKind::WhisperOffline,
            },
            _ => ModelKind::ZipformerTransducer,
        }
    }

    fn whisper_language(&self) -> Option<String> {
        self.language
            .as_deref()
            .and_then(whisper_language_code)
            .map(str::to_owned)
    }

    fn options(&self) -> AsrOptions {
        AsrOptions {
            chunk_ms: self.chunk_ms,
            threads: self.threads,
            provider: self.provider.clone(),
            buffer_ms: self.buffer_ms,
            language: self.whisper_language(),
        }
    }

    fn captions(&self) -> Captions {
        Captions::new(CaptionConfig {
            max_lines: self.max_lines,
            show_partial: !self.no_partial,
        })
    }

    fn session_options(&self, target: Option<String>) -> Result<SessionOptions> {
        Ok(SessionOptions {
            target,
            model_dir: self.model_dir()?,
            model_kind: self.model_kind().as_str().to_owned(),
            buffer_ms: self.buffer_ms,
            language: self.whisper_language(),
            threads: self.threads,
            provider: self.provider.clone(),
            max_lines: self.max_lines,
            show_partial: !self.no_partial,
        })
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
    let model = ModelFiles::load_kind(&common.model_dir()?, common.model_kind())?;
    let wave = Wave::read(&wav.display().to_string())
        .with_context(|| format!("cannot read WAV: {}", wav.display()))?;
    let mut captions = common.captions();
    let mut printer = Printer::default();
    speech_sherpa::transcribe(&wave, &model, &common.options(), |events| {
        if let Some(partial) = &events.partial {
            captions.set_partial(partial);
        }
        for text in events.finals {
            captions.add_final(&text);
        }
        printer.show(&captions.snapshot());
    })
}

fn live(target: Option<String>, common: Common) -> Result<()> {
    let session = Session::start(common.session_options(target)?)?;
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

fn models_command(action: ModelsCommand) -> Result<()> {
    match action {
        ModelsCommand::Root => println!("{}", models_root().display()),
        ModelsCommand::List { language } => {
            let filter = language
                .as_deref()
                .map(|id| {
                    speech_language(id)
                        .map(|_| id)
                        .with_context(|| format!("unknown language id: {id}"))
                })
                .transpose()?;
            for item in santhra_models::catalog() {
                if filter.is_some_and(|id| !santhra_models::matches_language_id(item, id)) {
                    continue;
                }
                let installed = santhra_models::resolve(item, &model_dir(item.id)?).is_ok();
                println!(
                    "{:<26} {:<12} {:<18} {:>10} MB  {}",
                    item.id,
                    item.language.as_str(),
                    item.kind.as_str(),
                    item.total_bytes() / 1_000_000,
                    if installed {
                        "installed"
                    } else {
                        "not downloaded"
                    }
                );
            }
        }
        ModelsCommand::Download { id } => {
            let item = entry(&id).with_context(|| format!("unknown model id: {id}"))?;
            let cancel = std::sync::atomic::AtomicBool::new(false);
            download_all(item, &cancel, |progress| match progress {
                Progress::Start { file, bytes } => println!("downloading {file} ({bytes} bytes)"),
                Progress::Bytes { done, total, .. } => {
                    let pct = done.saturating_mul(100).checked_div(total).unwrap_or(0);
                    print!("\r\x1b[K  {pct}% ({done}/{total})");
                    let _ = std::io::stdout().flush();
                }
                Progress::Done { file } => println!("\r\x1b[K  {file}: ok"),
            })?;
            println!("model {id} is ready");
        }
        ModelsCommand::Verify { id } => {
            let item = entry(&id).with_context(|| format!("unknown model id: {id}"))?;
            santhra_models::resolve(item, &model_dir(item.id)?)?;
            println!("model {id} is valid");
        }
    }
    Ok(())
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Models { action } => models_command(action),
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
