use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use app_core::{Message, Session, SessionOptions};
use audio_linux::Target;
use caption_core::{CaptionConfig, CaptionSnapshot, Captions};
use clap::{Args, Parser, Subcommand};
use santhra_download::{Progress, download_all};
use santhra_models::{
    ModelKind as CatalogKind, default_for_id, entry, model_dir, models_root, speech_language,
    whisper_language_code,
};
use serde_json::{Value, json};
use sherpa_onnx::Wave;
use speech_sherpa::{AsrOptions, ModelFiles, ModelKind, Transcriber};

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
    /// Transcribe a manifest of WAV files with one model load (JSONL output).
    TranscribeBatch {
        #[command(flatten)]
        common: Common,
        #[arg(long, help = "Dataset manifest with samples[].audio_path")]
        manifest: PathBuf,
        #[arg(long, help = "Write JSONL to this file instead of stdout")]
        output: Option<PathBuf>,
        #[arg(
            long,
            default_value_t = 600,
            help = "Batch wall-clock timeout in seconds; 0 disables it"
        )]
        timeout: u64,
    },
    Live {
        #[arg(
            long,
            help = "Sink node name or app:<object.serial>; default is the default sink's audio"
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
        #[arg(long, help = "Machine-readable output for the evaluation harness")]
        json: bool,
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
                CatalogKind::NemoStreamingTransducer => ModelKind::NemoStreamingTransducer,
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
            target: Target::from_key(target.as_deref()),
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

fn millis(start: Instant) -> f64 {
    (start.elapsed().as_secs_f64() * 1_000_000.0).round() / 1000.0
}

fn peak_rss_kb() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|line| {
        let rest = line.strip_prefix("VmHWM:")?;
        rest.split_whitespace().next()?.parse().ok()
    })
}

fn emit(sink: &mut dyn Write, value: &Value) -> Result<()> {
    writeln!(sink, "{}", serde_json::to_string(value)?)?;
    sink.flush()?;
    Ok(())
}

struct SampleOutcome {
    status: &'static str,
    finals: Vec<String>,
    elapsed_ms: Option<f64>,
    partial_count: u64,
    first_partial_ms: Option<f64>,
    final_caption_ms: Option<f64>,
    error: Option<String>,
}

impl SampleOutcome {
    fn failed(status: &'static str, error: String) -> Self {
        Self {
            status,
            finals: Vec::new(),
            elapsed_ms: None,
            partial_count: 0,
            first_partial_ms: None,
            final_caption_ms: None,
            error: Some(error),
        }
    }
}

fn run_sample(transcriber: &mut Transcriber, opts: &AsrOptions, path: &Path) -> SampleOutcome {
    let wave = match Wave::read(&path.display().to_string()) {
        Some(wave) => wave,
        None => {
            return SampleOutcome::failed(
                "audio_error",
                format!("cannot read WAV: {}", path.display()),
            );
        }
    };
    if wave.sample_rate() != speech_sherpa::SAMPLE_RATE {
        return SampleOutcome::failed(
            "audio_error",
            format!(
                "expected {} Hz audio, got {} Hz",
                speech_sherpa::SAMPLE_RATE,
                wave.sample_rate()
            ),
        );
    }

    let start = Instant::now();
    let mut outcome = SampleOutcome {
        status: "success",
        finals: Vec::new(),
        elapsed_ms: None,
        partial_count: 0,
        first_partial_ms: None,
        final_caption_ms: None,
        error: None,
    };
    let result = speech_sherpa::transcribe_with(transcriber, &wave, opts, |events| {
        if events
            .partial
            .as_deref()
            .is_some_and(|text| !text.is_empty())
        {
            outcome.partial_count += 1;
            if outcome.first_partial_ms.is_none() {
                outcome.first_partial_ms = Some(millis(start));
            }
        }
        for text in events.finals {
            outcome.final_caption_ms = Some(millis(start));
            outcome.finals.push(text);
        }
    });
    outcome.elapsed_ms = Some(millis(start));
    if let Err(error) = result {
        outcome.status = "model_error";
        outcome.error = Some(format!("{error:#}"));
    } else if outcome.finals.iter().all(|text| text.trim().is_empty()) {
        outcome.status = "empty_transcript";
    }
    outcome
}

fn transcribe_batch(
    common: Common,
    manifest: PathBuf,
    output: Option<PathBuf>,
    timeout: u64,
) -> Result<()> {
    let batch_start = Instant::now();
    let deadline = (timeout > 0).then(|| batch_start + Duration::from_secs(timeout));
    let selected = common.selected()?;
    let model_start = Instant::now();
    let model = ModelFiles::load_kind(&common.model_dir()?, common.model_kind())?;
    let opts = common.options();
    let mut transcriber = Transcriber::new(&model, &opts)?;
    let load_ms = millis(model_start);

    let text = std::fs::read_to_string(&manifest)
        .with_context(|| format!("cannot read manifest {}", manifest.display()))?;
    let document: Value = serde_json::from_str(&text)
        .with_context(|| format!("cannot parse manifest {}", manifest.display()))?;
    let samples = document
        .get("samples")
        .and_then(Value::as_array)
        .context("manifest has no samples array")?;
    let manifest_dir = manifest.parent().unwrap_or_else(|| Path::new("."));

    let model_files: Vec<Value> = selected
        .map(|item| {
            item.files
                .iter()
                .map(|file| json!({"name": file.name, "sha256": file.sha256, "bytes": file.bytes}))
                .collect()
        })
        .unwrap_or_default();

    let mut sink: Box<dyn Write> = match &output {
        Some(path) => {
            Box::new(BufWriter::new(File::create(path).with_context(|| {
                format!("cannot create output file {}", path.display())
            })?))
        }
        None => Box::new(BufWriter::new(io::stdout())),
    };

    emit(
        &mut sink,
        &json!({
            "type": "meta",
            "model_id": selected.map(|item| item.id).unwrap_or("custom"),
            "language": common.language,
            "kind": common.model_kind().as_str(),
            "load_ms": load_ms,
            "sample_count": samples.len(),
            "model_files": model_files,
            "asr": {
                "chunk_ms": opts.chunk_ms,
                "threads": opts.threads,
                "provider": opts.provider.clone(),
                "buffer_ms": opts.buffer_ms,
                "whisper_language": opts.language.clone(),
            },
        }),
    )?;

    let mut success = 0u64;
    let mut empty = 0u64;
    let mut failed = 0u64;
    let mut timed_out = 0u64;
    for (index, sample) in samples.iter().enumerate() {
        let id = sample
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            timed_out += 1;
            emit(
                &mut sink,
                &json!({
                    "type": "sample",
                    "index": index,
                    "id": id,
                    "status": "timeout",
                    "finals": [],
                    "elapsed_ms": Value::Null,
                    "partial_count": 0,
                    "first_partial_ms": Value::Null,
                    "final_caption_ms": Value::Null,
                    "error": format!("batch timeout after {timeout}s"),
                }),
            )?;
            continue;
        }

        let audio = sample
            .get("audio_path")
            .or_else(|| sample.get("wav"))
            .and_then(Value::as_str);
        let outcome = match audio {
            Some(audio) => {
                let path = PathBuf::from(audio);
                let path = if path.is_absolute() {
                    path
                } else {
                    manifest_dir.join(path)
                };
                run_sample(&mut transcriber, &opts, &path)
            }
            None => SampleOutcome::failed("audio_error", "sample has no audio_path".into()),
        };
        match outcome.status {
            "success" => success += 1,
            "empty_transcript" => empty += 1,
            _ => failed += 1,
        }
        emit(
            &mut sink,
            &json!({
                "type": "sample",
                "index": index,
                "id": id,
                "status": outcome.status,
                "finals": outcome.finals,
                "elapsed_ms": outcome.elapsed_ms,
                "partial_count": outcome.partial_count,
                "first_partial_ms": outcome.first_partial_ms,
                "final_caption_ms": outcome.final_caption_ms,
                "error": outcome.error,
            }),
        )?;
    }

    emit(
        &mut sink,
        &json!({
            "type": "summary",
            "attempted": samples.len(),
            "success": success,
            "empty_transcript": empty,
            "failed": failed,
            "timeout": timed_out,
            "peak_rss_kb": peak_rss_kb(),
            "total_elapsed_ms": millis(batch_start),
        }),
    )?;
    Ok(())
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
        ModelsCommand::List { language, json } => {
            let filter = language
                .as_deref()
                .map(|id| {
                    speech_language(id)
                        .map(|_| id)
                        .with_context(|| format!("unknown language id: {id}"))
                })
                .transpose()?;
            if json {
                let default_id = filter.and_then(default_for_id).map(|item| item.id);
                let mut items = Vec::new();
                for item in santhra_models::catalog() {
                    if filter.is_some_and(|id| !santhra_models::matches_language_id(item, id)) {
                        continue;
                    }
                    let installed = model_dir(item.id)
                        .map(|dir| santhra_models::resolve(item, &dir).is_ok())
                        .unwrap_or(false);
                    items.push(json!({
                        "id": item.id,
                        "label": item.label,
                        "language": item.language.as_str(),
                        "kind": item.kind.as_str(),
                        "license": item.license,
                        "source": item.source,
                        "installed": installed,
                        "default": default_id == Some(item.id),
                        "bytes": item.total_bytes(),
                        "files": item.files.iter().map(|file| json!({
                            "name": file.name,
                            "sha256": file.sha256,
                            "bytes": file.bytes,
                        })).collect::<Vec<_>>(),
                    }));
                }
                println!("{}", serde_json::to_string_pretty(&items)?);
                return Ok(());
            }
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
        Command::TranscribeBatch {
            common,
            manifest,
            output,
            timeout,
        } => transcribe_batch(common, manifest, output, timeout),
        Command::Live { target, common } => live(target, common),
    }
}

fn main() {
    if let Err(err) = run(Cli::parse()) {
        eprintln!("error: {err:#}");
        std::process::exit(2);
    }
}
