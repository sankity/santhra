use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
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
}

fn pick(dir: &Path, names: &[&str]) -> Result<PathBuf> {
    names
        .iter()
        .map(|name| dir.join(name))
        .find(|path| path.is_file())
        .with_context(|| format!("missing {} in {}", names.join(" / "), dir.display()))
}

impl ModelFiles {
    /// Backwards-compatible loader for the existing Zipformer layout.
    pub fn load(dir: &Path) -> Result<Self> {
        Self::load_kind(dir, ModelKind::ZipformerTransducer)
    }

    pub fn load_kind(dir: &Path, kind: ModelKind) -> Result<Self> {
        let model = match kind {
            ModelKind::ZipformerTransducer => Self::Zipformer {
                encoder: pick(
                    dir,
                    &[
                        "encoder-epoch-99-avg-1.int8.onnx",
                        "encoder-epoch-99-avg-1.onnx",
                    ],
                )?,
                decoder: pick(
                    dir,
                    &[
                        "decoder-epoch-99-avg-1.int8.onnx",
                        "decoder-epoch-99-avg-1.onnx",
                    ],
                )?,
                joiner: pick(
                    dir,
                    &[
                        "joiner-epoch-99-avg-1.int8.onnx",
                        "joiner-epoch-99-avg-1.onnx",
                    ],
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
                encoder: pick(dir, &["tiny-encoder.int8.onnx", "tiny-encoder.onnx"])?,
                decoder: pick(dir, &["tiny-decoder.int8.onnx", "tiny-decoder.onnx"])?,
                tokens: pick(dir, &["tiny-tokens.txt", "tokens.txt"])?,
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
        }
    }

    fn tokens(&self) -> &Path {
        match self {
            ModelFiles::Zipformer { tokens, .. }
            | ModelFiles::ParaformerOnline { tokens, .. }
            | ModelFiles::ParaformerOffline { tokens, .. }
            | ModelFiles::Whisper { tokens, .. } => tokens,
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
    pub finals: Vec<String>,
}

enum Engine {
    Online {
        recognizer: OnlineRecognizer,
        stream: OnlineStream,
        last_partial: String,
    },
    Offline {
        recognizer: OfflineRecognizer,
        buffer: Vec<f32>,
        samples_per_chunk: usize,
    },
}

pub struct Transcriber {
    engine: Engine,
}

impl Transcriber {
    pub fn new(model: &ModelFiles, opts: &AsrOptions) -> Result<Self> {
        let engine = match model {
            ModelFiles::Zipformer { .. } | ModelFiles::ParaformerOnline { .. } => {
                let recognizer = OnlineRecognizer::create(&model.online_config(opts)?)
                    .context("sherpa-onnx rejected the streaming model config")?;
                let stream = recognizer.create_stream();
                Engine::Online {
                    recognizer,
                    stream,
                    last_partial: String::new(),
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
        Ok(Self { engine })
    }

    pub fn push(&mut self, samples: &[f32]) -> Events {
        match &mut self.engine {
            Engine::Online {
                recognizer,
                stream,
                last_partial,
            } => {
                if !samples.is_empty() {
                    stream.accept_waveform(SAMPLE_RATE, samples);
                }
                decode_online(recognizer, stream, last_partial)
            }
            Engine::Offline {
                recognizer,
                buffer,
                samples_per_chunk,
            } => {
                buffer.extend_from_slice(samples);
                let mut events = Events::default();
                while buffer.len() >= *samples_per_chunk {
                    let chunk: Vec<f32> = buffer.drain(..*samples_per_chunk).collect();
                    decode_offline(recognizer, &chunk, &mut events);
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
            } => {
                stream.input_finished();
                let mut events = decode_online(recognizer, stream, last_partial);
                if !last_partial.trim().is_empty() {
                    events.finals.push(std::mem::take(last_partial));
                }
                events
            }
            Engine::Offline {
                recognizer, buffer, ..
            } => {
                let mut events = Events::default();
                if !buffer.is_empty() {
                    let chunk = std::mem::take(buffer);
                    decode_offline(recognizer, &chunk, &mut events);
                }
                events
            }
        }
    }
}

fn decode_online(
    recognizer: &OnlineRecognizer,
    stream: &OnlineStream,
    last_partial: &mut String,
) -> Events {
    let mut events = Events::default();
    while recognizer.is_ready(stream) {
        recognizer.decode(stream);
        let Some(result) = recognizer.get_result(stream) else {
            continue;
        };
        if !result.text.is_empty() {
            events.partial = Some(result.text.clone());
            *last_partial = result.text.clone();
        }
        if recognizer.is_endpoint(stream) {
            if !result.text.trim().is_empty() {
                events.finals.push(result.text.clone());
            }
            events.partial = Some(String::new());
            last_partial.clear();
            recognizer.reset(stream);
        }
    }
    events
}

fn decode_offline(recognizer: &OfflineRecognizer, samples: &[f32], events: &mut Events) {
    let stream = recognizer.create_stream();
    stream.accept_waveform(SAMPLE_RATE, samples);
    recognizer.decode(&stream);
    if let Some(result) = stream
        .get_result()
        .filter(|result| !result.text.trim().is_empty())
    {
        events.finals.push(result.text);
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
    mut on_events: impl FnMut(Events),
) -> Result<()> {
    let sample_rate = wave.sample_rate();
    if sample_rate != SAMPLE_RATE {
        bail!("expected 16 kHz mono WAV, got {sample_rate} Hz (resampling is not implemented yet)");
    }
    let mut transcriber = Transcriber::new(model, opts)?;
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
    fn chunk_size_math() {
        assert_eq!(chunk_size(16000, 100), 1600);
        assert_eq!(chunk_size(8000, 100), 800);
        assert_eq!(chunk_size(16000, 0), 1);
    }

    #[test]
    #[ignore = "needs the model; set SANTHRA_MODEL_DIR to the model directory"]
    fn transcribes_sample_wav() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let dir = root
            .join(std::env::var("SANTHRA_MODEL_DIR").expect("SANTHRA_MODEL_DIR"))
            .display()
            .to_string();
        let model = ModelFiles::load(Path::new(&dir)).expect("model files");
        let wave = Wave::read(
            root.join("models/sherpa-onnx-streaming-zipformer-en-20M-2023-02-17/test_wavs/0.wav")
                .to_str()
                .unwrap(),
        )
        .expect("wav");
        let mut finals = Vec::new();
        transcribe(&wave, &model, &AsrOptions::default(), |events| {
            finals.extend(events.finals);
        })
        .expect("transcribe");
        assert!(!finals.is_empty());
    }
}
