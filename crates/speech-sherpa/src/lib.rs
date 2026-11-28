use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
pub use caption_core::{Segment, SegmentStatus};
pub use santhra_models::ModelKind;
use sherpa_onnx::{
    OfflineRecognizer, OfflineRecognizerConfig, OnlineRecognizer, OnlineRecognizerConfig,
    OnlineStream, Wave,
};

pub const SAMPLE_RATE: i32 = 16_000;

#[derive(Debug)]
pub enum ModelFiles {
    Zipformer {
        encoder: PathBuf,
        decoder: PathBuf,
        joiner: PathBuf,
        tokens: PathBuf,
    },
    ParaformerOnline {
        encoder: PathBuf,
        decoder: PathBuf,
        tokens: PathBuf,
    },
    ParaformerOffline {
        model: PathBuf,
        tokens: PathBuf,
    },
    Whisper {
        encoder: PathBuf,
        decoder: PathBuf,
        tokens: PathBuf,
    },
    /// NeMo cache-aware FastConformer-RNNT (Nemotron streaming). sherpa-onnx selects its NeMo
    /// implementation from the decoder ONNX; the language is a per-stream option.
    NemoTransducer {
        encoder: PathBuf,
        decoder: PathBuf,
        joiner: PathBuf,
        tokens: PathBuf,
    },
}

fn pick(dir: &Path, names: &[&str]) -> Result<PathBuf> {
    names
        .iter()
        .map(|name| dir.join(name))
        .find(|path| path.is_file())
        .with_context(|| format!("missing {} in {}", names.join(" / "), dir.display()))
}

fn pick_prefixed(dir: &Path, names: &[&str], prefix: &str) -> Result<PathBuf> {
    if let Ok(path) = pick(dir, names) {
        return Ok(path);
    }
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("read model directory {}", dir.display()))?
        .filter_map(|item| item.ok())
        .map(|item| item.path())
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(prefix) && name.ends_with(".onnx"))
        })
        .collect();
    candidates.sort_by_key(|path| {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        (!name.ends_with(".int8.onnx"), name.to_owned())
    });
    candidates
        .into_iter()
        .next()
        .with_context(|| format!("missing {}*.onnx in {}", prefix, dir.display()))
}

fn pick_suffix(dir: &Path, suffix: &str) -> Result<PathBuf> {
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("read model directory {}", dir.display()))?
        .filter_map(|item| item.ok())
        .map(|item| item.path())
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with(suffix))
        })
        .collect();
    candidates.sort();
    candidates
        .into_iter()
        .next()
        .with_context(|| format!("missing *{suffix} in {}", dir.display()))
}

fn pick_whisper_component(dir: &Path, component: &str) -> Result<PathBuf> {
    pick_suffix(dir, &format!("-{component}.int8.onnx"))
        .or_else(|_| pick_suffix(dir, &format!("-{component}.onnx")))
}

impl ModelFiles {
    /// Backwards-compatible loader for the existing Zipformer layout.
    pub fn load(dir: &Path) -> Result<Self> {
        Self::load_kind(dir, ModelKind::ZipformerTransducer)
    }

    pub fn load_kind(dir: &Path, kind: ModelKind) -> Result<Self> {
        let model = match kind {
            ModelKind::ZipformerTransducer => Self::Zipformer {
                encoder: pick_prefixed(
                    dir,
                    &[
                        "encoder-epoch-99-avg-1.int8.onnx",
                        "encoder-epoch-99-avg-1.onnx",
                    ],
                    "encoder-",
                )?,
                decoder: pick_prefixed(
                    dir,
                    &[
                        "decoder-epoch-99-avg-1.int8.onnx",
                        "decoder-epoch-99-avg-1.onnx",
                    ],
                    "decoder-",
                )?,
                joiner: pick_prefixed(
                    dir,
                    &[
                        "joiner-epoch-99-avg-1.int8.onnx",
                        "joiner-epoch-99-avg-1.onnx",
                    ],
                    "joiner-",
                )?,
                tokens: pick(dir, &["tokens.txt"])?,
            },
            ModelKind::ParaformerOnline => Self::ParaformerOnline {
                encoder: pick(dir, &["encoder.int8.onnx", "encoder.onnx"])?,
                decoder: pick(dir, &["decoder.int8.onnx", "decoder.onnx"])?,
                tokens: pick(dir, &["tokens.txt"])?,
            },
            ModelKind::ParaformerOffline => Self::ParaformerOffline {
                model: pick(dir, &["model.int8.onnx", "model.onnx"])?,
                tokens: pick(dir, &["tokens.txt"])?,
            },
            ModelKind::WhisperOffline => Self::Whisper {
                encoder: pick_whisper_component(dir, "encoder")?,
                decoder: pick_whisper_component(dir, "decoder")?,
                tokens: pick_suffix(dir, "-tokens.txt").or_else(|_| pick(dir, &["tokens.txt"]))?,
            },
            ModelKind::NemoStreamingTransducer => Self::NemoTransducer {
                encoder: pick(dir, &["encoder.int8.onnx", "encoder.onnx"])?,
                decoder: pick(dir, &["decoder.int8.onnx", "decoder.onnx"])?,
                joiner: pick(dir, &["joiner.int8.onnx", "joiner.onnx"])?,
                tokens: pick(dir, &["tokens.txt"])?,
            },
        };
        Ok(model)
    }

    pub fn kind(&self) -> ModelKind {
        match self {
            ModelFiles::Zipformer { .. } => ModelKind::ZipformerTransducer,
            ModelFiles::ParaformerOnline { .. } => ModelKind::ParaformerOnline,
            ModelFiles::ParaformerOffline { .. } => ModelKind::ParaformerOffline,
            ModelFiles::Whisper { .. } => ModelKind::WhisperOffline,
            ModelFiles::NemoTransducer { .. } => ModelKind::NemoStreamingTransducer,
        }
    }

    fn tokens(&self) -> &Path {
        match self {
            ModelFiles::Zipformer { tokens, .. }
            | ModelFiles::ParaformerOnline { tokens, .. }
            | ModelFiles::ParaformerOffline { tokens, .. }
            | ModelFiles::Whisper { tokens, .. }
            | ModelFiles::NemoTransducer { tokens, .. } => tokens,
        }
    }

    fn online_config(&self, opts: &AsrOptions) -> Result<OnlineRecognizerConfig> {
        let mut config = OnlineRecognizerConfig::default();
        let model = &mut config.model_config;
        match self {
            ModelFiles::Zipformer {
                encoder,
                decoder,
                joiner,
                ..
            }
            | ModelFiles::NemoTransducer {
                encoder,
                decoder,
                joiner,
                ..
            } => {
                model.transducer.encoder = Some(encoder.display().to_string());
                model.transducer.decoder = Some(decoder.display().to_string());
                model.transducer.joiner = Some(joiner.display().to_string());
            }
            ModelFiles::ParaformerOnline {
                encoder, decoder, ..
            } => {
                model.paraformer.encoder = Some(encoder.display().to_string());
                model.paraformer.decoder = Some(decoder.display().to_string());
            }
            _ => bail!("model is not a streaming recognizer"),
        }
        model.tokens = Some(self.tokens().display().to_string());
        model.provider = Some(opts.provider.clone());
        model.num_threads = opts.threads;
        model.debug = false;
        config.decoding_method = Some("greedy_search".into());
        config.enable_endpoint = true;
        Ok(config)
    }

    fn offline_config(&self, opts: &AsrOptions) -> Result<OfflineRecognizerConfig> {
        let mut config = OfflineRecognizerConfig::default();
        let model = &mut config.model_config;
        match self {
            ModelFiles::ParaformerOffline { model: path, .. } => {
                model.paraformer.model = Some(path.display().to_string());
            }
            ModelFiles::Whisper {
                encoder, decoder, ..
            } => {
                model.whisper.encoder = Some(encoder.display().to_string());
                model.whisper.decoder = Some(decoder.display().to_string());
                model.whisper.language = opts.language.clone();
                model.whisper.task = Some("transcribe".into());
            }
            _ => bail!("model is not an offline recognizer"),
        }
        model.tokens = Some(self.tokens().display().to_string());
        model.provider = Some(opts.provider.clone());
        model.num_threads = opts.threads;
        model.debug = false;
        config.decoding_method = Some("greedy_search".into());
        Ok(config)
    }
}

#[derive(Debug)]
pub struct AsrOptions {
    pub chunk_ms: u32,
    pub threads: i32,
    pub provider: String,
    /// Buffer size for offline models, in milliseconds.
    pub buffer_ms: u32,
    /// Whisper language code (e.g. "en", "zh", "id"), when the model supports it.
    pub language: Option<String>,
}

impl Default for AsrOptions {
    fn default() -> Self {
        Self {
            chunk_ms: 100,
            threads: 2,
            provider: "cpu".into(),
            buffer_ms: 2500,
            language: None,
        }
    }
}

#[derive(Default)]
pub struct Events {
    pub partial: Option<String>,
    pub finals: Vec<Segment>,
}

fn samples_to_ms(samples: u64) -> u64 {
    samples / 16
}

fn seconds_to_ms(seconds: f32) -> u64 {
    if !seconds.is_finite() || seconds <= 0.0 {
        0
    } else {
        (seconds * 1000.0).round() as u64
    }
}

/// Sample-accurate clock used when sherpa does not expose token timestamps.
#[derive(Default)]
struct Clock {
    session_samples: u64,
    stream_start_samples: u64,
    segment_start_samples: u64,
}

impl Clock {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn advance(&mut self, samples: usize) {
        self.session_samples += samples as u64;
    }

    fn on_endpoint(&mut self) {
        self.stream_start_samples = self.session_samples;
        self.segment_start_samples = self.session_samples;
    }

    /// Prefer sherpa's token timestamps; they are relative to the current stream, so add the
    /// samples already consumed before that stream started. `start_time` is always 0 in the
    /// pinned sherpa build, so it is not used.
    fn timing(&self, timestamps: Option<&Vec<f32>>) -> (u64, Option<u64>) {
        if let Some(first) = timestamps.and_then(|ts| ts.first())
            && let Some(last) = timestamps.and_then(|ts| ts.last())
        {
            let base = samples_to_ms(self.stream_start_samples);
            let start = base.saturating_add(seconds_to_ms(*first));
            let end = base.saturating_add(seconds_to_ms(*last)).max(start);
            return (start, Some(end));
        }
        let start = samples_to_ms(self.segment_start_samples);
        let end = samples_to_ms(self.session_samples).max(start);
        (start, Some(end))
    }
}

enum Engine {
    Online {
        recognizer: OnlineRecognizer,
        stream: OnlineStream,
        last_partial: String,
        /// Per-stream model option. Only NeMo multilingual models consume it.
        language: Option<String>,
    },
    Offline {
        recognizer: OfflineRecognizer,
        buffer: Vec<f32>,
        samples_per_chunk: usize,
    },
}

pub struct Transcriber {
    engine: Engine,
    clock: Clock,
}

impl Transcriber {
    pub fn new(model: &ModelFiles, opts: &AsrOptions) -> Result<Self> {
        let engine = match model {
            ModelFiles::Zipformer { .. }
            | ModelFiles::ParaformerOnline { .. }
            | ModelFiles::NemoTransducer { .. } => {
                let recognizer = OnlineRecognizer::create(&model.online_config(opts)?)
                    .context("sherpa-onnx rejected the streaming model config")?;
                // Only the multilingual NeMo transducer reads this option; it selects the
                // language-ID prompt and defaults to auto-detection when unset.
                let language = match model {
                    ModelFiles::NemoTransducer { .. } => opts.language.clone(),
                    _ => None,
                };
                let stream = create_online_stream(&recognizer, language.as_deref());
                Engine::Online {
                    recognizer,
                    stream,
                    last_partial: String::new(),
                    language,
                }
            }
            ModelFiles::ParaformerOffline { .. } | ModelFiles::Whisper { .. } => {
                let recognizer = OfflineRecognizer::create(&model.offline_config(opts)?)
                    .context("sherpa-onnx rejected the offline model config")?;
                Engine::Offline {
                    recognizer,
                    buffer: Vec::new(),
                    samples_per_chunk: ((SAMPLE_RATE as usize * opts.buffer_ms as usize) / 1000)
                        .max(1),
                }
            }
        };
        Ok(Self {
            engine,
            clock: Clock::default(),
        })
    }

    /// Clear per-utterance state without reloading the model. The clock restarts too, so batch
    /// samples are timed relative to themselves; a live session never calls this.
    fn reset(&mut self) {
        self.clock.reset();
        match &mut self.engine {
            Engine::Online {
                recognizer,
                stream,
                last_partial,
                language,
            } => {
                *stream = create_online_stream(recognizer, language.as_deref());
                last_partial.clear();
            }
            Engine::Offline { buffer, .. } => buffer.clear(),
        }
    }

    pub fn push(&mut self, samples: &[f32]) -> Events {
        self.clock.advance(samples.len());
        match &mut self.engine {
            Engine::Online {
                recognizer,
                stream,
                last_partial,
                ..
            } => {
                if !samples.is_empty() {
                    stream.accept_waveform(SAMPLE_RATE, samples);
                }
                decode_online(recognizer, stream, last_partial, &mut self.clock)
            }
            Engine::Offline {
                recognizer,
                buffer,
                samples_per_chunk,
            } => {
                buffer.extend_from_slice(samples);
                let mut cursor = self
                    .clock
                    .session_samples
                    .saturating_sub(buffer.len() as u64);
                let mut events = Events::default();
                while buffer.len() >= *samples_per_chunk {
                    let chunk: Vec<f32> = buffer.drain(..*samples_per_chunk).collect();
                    let start_ms = samples_to_ms(cursor);
                    cursor += chunk.len() as u64;
                    decode_offline(
                        recognizer,
                        &chunk,
                        &mut events,
                        start_ms,
                        samples_to_ms(cursor),
                    );
                }
                events
            }
        }
    }

    pub fn finish(&mut self) -> Events {
        match &mut self.engine {
            Engine::Online {
                recognizer,
                stream,
                last_partial,
                ..
            } => {
                stream.input_finished();
                let mut events = decode_online(recognizer, stream, last_partial, &mut self.clock);
                if !last_partial.trim().is_empty() {
                    let (start_ms, end_ms) = self.clock.timing(None);
                    events.finals.push(Segment {
                        text: std::mem::take(last_partial),
                        start_ms,
                        end_ms,
                        status: SegmentStatus::Final,
                        confidence: None,
                    });
                }
                events
            }
            Engine::Offline {
                recognizer, buffer, ..
            } => {
                let mut events = Events::default();
                if !buffer.is_empty() {
                    let chunk = std::mem::take(buffer);
                    let start_ms = self
                        .clock
                        .session_samples
                        .saturating_sub(chunk.len() as u64);
                    decode_offline(
                        recognizer,
                        &chunk,
                        &mut events,
                        samples_to_ms(start_ms),
                        samples_to_ms(self.clock.session_samples),
                    );
                }
                events
            }
        }
    }
}

/// Create a stream and apply the per-stream model options it needs.
///
/// `reset()` replaces the stream, so this is the only way streams are created; a fresh
/// `create_stream()` starts with empty options.
fn create_online_stream(recognizer: &OnlineRecognizer, language: Option<&str>) -> OnlineStream {
    let stream = recognizer.create_stream();
    if let Some(code) = language {
        stream.set_option("language", code);
    }
    stream
}

fn decode_online(
    recognizer: &OnlineRecognizer,
    stream: &OnlineStream,
    last_partial: &mut String,
    clock: &mut Clock,
) -> Events {
    let mut events = Events::default();
    while recognizer.is_ready(stream) {
        recognizer.decode(stream);
        let Some(result) = recognizer.get_result(stream) else {
            continue;
        };
        let endpoint = recognizer.is_endpoint(stream);
        if endpoint {
            if !result.text.trim().is_empty() {
                let (start_ms, end_ms) = clock.timing(result.timestamps.as_ref());
                events.finals.push(Segment {
                    text: result.text.clone(),
                    start_ms,
                    end_ms,
                    status: SegmentStatus::Final,
                    confidence: None,
                });
            }
            events.partial = Some(String::new());
            last_partial.clear();
            recognizer.reset(stream);
            clock.on_endpoint();
        } else if !result.text.is_empty() {
            events.partial = Some(result.text.clone());
            *last_partial = result.text.clone();
        }
    }
    events
}

fn decode_offline(
    recognizer: &OfflineRecognizer,
    samples: &[f32],
    events: &mut Events,
    start_ms: u64,
    end_ms: u64,
) {
    let stream = recognizer.create_stream();
    stream.accept_waveform(SAMPLE_RATE, samples);
    recognizer.decode(&stream);
    if let Some(result) = stream
        .get_result()
        .filter(|result| !result.text.trim().is_empty())
    {
        let (start, end) = match result.timestamps.as_ref() {
            Some(ts) if ts.len() >= 2 => {
                let first = start_ms.saturating_add(seconds_to_ms(ts[0]));
                let last = start_ms.saturating_add(seconds_to_ms(ts[ts.len() - 1]));
                (first, last.max(first).min(end_ms))
            }
            _ => (start_ms, end_ms),
        };
        events.finals.push(Segment {
            text: result.text,
            start_ms: start,
            end_ms: Some(end),
            status: SegmentStatus::Final,
            confidence: None,
        });
    }
    events.partial = None;
}

pub fn chunk_size(sample_rate: i32, chunk_ms: u32) -> usize {
    ((sample_rate as usize * chunk_ms as usize) / 1000).max(1)
}

pub fn transcribe(
    wave: &Wave,
    model: &ModelFiles,
    opts: &AsrOptions,
    on_events: impl FnMut(Events),
) -> Result<()> {
    let mut transcriber = Transcriber::new(model, opts)?;
    transcribe_with(&mut transcriber, wave, opts, on_events)
}

/// Feed one wave through an already-loaded transcriber.
///
/// Evaluators call this once per sample to keep the recognizer (and therefore the model)
/// loaded across a batch. `transcribe` is the single-shot wrapper used by the CLI.
pub fn transcribe_with(
    transcriber: &mut Transcriber,
    wave: &Wave,
    opts: &AsrOptions,
    mut on_events: impl FnMut(Events),
) -> Result<()> {
    let sample_rate = wave.sample_rate();
    if sample_rate != SAMPLE_RATE {
        bail!("expected 16 kHz mono WAV, got {sample_rate} Hz (resampling is not implemented yet)");
    }
    transcriber.reset();
    let chunk = chunk_size(sample_rate, opts.chunk_ms);
    for part in wave.samples().chunks(chunk) {
        on_events(transcriber.push(part));
    }
    on_events(transcriber.finish());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zipformer_loader_accepts_versioned_file_names() {
        let dir = std::env::temp_dir().join(format!("santhra-model-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("model dir");
        for name in [
            "encoder-epoch-29-avg-9-with-averaged-model.int8.onnx",
            "decoder-epoch-29-avg-9-with-averaged-model.int8.onnx",
            "joiner-epoch-29-avg-9-with-averaged-model.int8.onnx",
            "tokens.txt",
        ] {
            std::fs::write(dir.join(name), []).expect("create model file");
        }
        let model =
            ModelFiles::load_kind(&dir, ModelKind::ZipformerTransducer).expect("load model");
        assert_eq!(model.kind(), ModelKind::ZipformerTransducer);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn whisper_loader_accepts_size_specific_file_names() {
        for size in ["base", "small"] {
            let dir =
                std::env::temp_dir().join(format!("santhra-whisper-{size}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("model dir");
            for name in [
                format!("{size}-encoder.int8.onnx"),
                format!("{size}-decoder.int8.onnx"),
                format!("{size}-tokens.txt"),
            ] {
                std::fs::write(dir.join(name), []).expect("create model file");
            }
            let model =
                ModelFiles::load_kind(&dir, ModelKind::WhisperOffline).expect("load Whisper model");
            assert_eq!(model.kind(), ModelKind::WhisperOffline);
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn nemo_loader_accepts_nemotron_file_names() {
        let dir = std::env::temp_dir().join(format!("santhra-nemo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("model dir");
        for name in [
            "encoder.int8.onnx",
            "decoder.int8.onnx",
            "joiner.int8.onnx",
            "tokens.txt",
        ] {
            std::fs::write(dir.join(name), []).expect("create model file");
        }
        let model = ModelFiles::load_kind(&dir, ModelKind::NemoStreamingTransducer)
            .expect("load Nemotron model");
        assert_eq!(model.kind(), ModelKind::NemoStreamingTransducer);
        assert!(model.online_config(&AsrOptions::default()).is_ok());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn chunk_size_math() {
        assert_eq!(chunk_size(16000, 100), 1600);
        assert_eq!(chunk_size(8000, 100), 800);
        assert_eq!(chunk_size(16000, 0), 1);
    }

    #[test]
    fn timing_helpers_are_accurate() {
        assert_eq!(samples_to_ms(16_000), 1000);
        assert_eq!(samples_to_ms(16), 1);
        assert_eq!(seconds_to_ms(1.25), 1250);
        assert_eq!(seconds_to_ms(-1.0), 0);
        assert_eq!(seconds_to_ms(f32::NAN), 0);
    }

    #[test]
    fn clock_fallback_counts_samples_and_restarts_on_reset() {
        let mut clock = Clock::default();
        clock.advance(16_000);
        assert_eq!(clock.timing(None), (0, Some(1000)));
        clock.reset();
        clock.advance(1600);
        assert_eq!(clock.timing(None), (0, Some(100)));
    }

    #[test]
    fn clock_keeps_session_offset_across_endpoint() {
        let mut clock = Clock::default();
        clock.advance(16_000);
        clock.on_endpoint();
        clock.advance(1600);
        assert_eq!(clock.timing(None), (1000, Some(1100)));
        assert_eq!(clock.timing(Some(&vec![0.1, 0.2])), (1100, Some(1200)));
    }
}
