use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use sherpa_onnx::{OnlineRecognizer, OnlineRecognizerConfig, Wave};

pub struct ModelFiles {
    encoder: String,
    decoder: String,
    joiner: String,
    tokens: String,
}

impl ModelFiles {
    pub fn load(dir: &Path) -> Result<Self> {
        let pick = |names: &[&str]| -> Option<PathBuf> {
            names.iter().map(|n| dir.join(n)).find(|p| p.is_file())
        };
        let want = |name: &str, found: Option<PathBuf>| -> Result<String> {
            Ok(found
                .with_context(|| format!("missing {name} in {}", dir.display()))?
                .display()
                .to_string())
        };
        Ok(Self {
            encoder: want(
                "encoder .onnx",
                pick(&[
                    "encoder-epoch-99-avg-1.int8.onnx",
                    "encoder-epoch-99-avg-1.onnx",
                ]),
            )?,
            decoder: want(
                "decoder .onnx",
                pick(&[
                    "decoder-epoch-99-avg-1.onnx",
                    "decoder-epoch-99-avg-1.int8.onnx",
                ]),
            )?,
            joiner: want(
                "joiner .onnx",
                pick(&[
                    "joiner-epoch-99-avg-1.int8.onnx",
                    "joiner-epoch-99-avg-1.onnx",
                ]),
            )?,
            tokens: want("tokens.txt", pick(&["tokens.txt"]))?,
        })
    }

    fn recognizer(&self, opts: &AsrOptions) -> Result<OnlineRecognizer> {
        let mut config = OnlineRecognizerConfig::default();
        config.model_config.transducer.encoder = Some(self.encoder.clone());
        config.model_config.transducer.decoder = Some(self.decoder.clone());
        config.model_config.transducer.joiner = Some(self.joiner.clone());
        config.model_config.tokens = Some(self.tokens.clone());
        config.model_config.provider = Some(opts.provider.clone());
        config.model_config.num_threads = opts.threads;
        config.model_config.debug = false;
        config.decoding_method = Some("greedy_search".into());
        config.enable_endpoint = true;
        OnlineRecognizer::create(&config).context("sherpa-onnx rejected the model config")
    }
}

pub struct AsrOptions {
    pub chunk_ms: u32,
    pub threads: i32,
    pub provider: String,
}

impl Default for AsrOptions {
    fn default() -> Self {
        Self {
            chunk_ms: 100,
            threads: 2,
            provider: "cpu".into(),
        }
    }
}

pub fn chunk_size(sample_rate: i32, chunk_ms: u32) -> usize {
    ((sample_rate as usize * chunk_ms as usize) / 1000).max(1)
}

/// Feed the waveform in chunks (simulating streaming) and report partial/final text.
pub fn transcribe(
    wave: &Wave,
    model: &ModelFiles,
    opts: &AsrOptions,
    mut on_partial: impl FnMut(&str),
    mut on_final: impl FnMut(&str),
) -> Result<()> {
    let sample_rate = wave.sample_rate();
    if sample_rate != 16000 {
        bail!("expected 16 kHz mono WAV, got {sample_rate} Hz (resampling lands in Phase 2)");
    }
    let recognizer = model.recognizer(opts)?;
    let stream = recognizer.create_stream();
    let chunk = chunk_size(sample_rate, opts.chunk_ms);

    for part in wave.samples().chunks(chunk) {
        stream.accept_waveform(sample_rate, part);
        while recognizer.is_ready(&stream) {
            recognizer.decode(&stream);
            if let Some(result) = recognizer.get_result(&stream) {
                on_partial(&result.text);
                if recognizer.is_endpoint(&stream) {
                    on_final(&result.text);
                    recognizer.reset(&stream);
                }
            }
        }
    }

    stream.input_finished();
    while recognizer.is_ready(&stream) {
        recognizer.decode(&stream);
    }
    if let Some(result) = recognizer.get_result(&stream) {
        on_final(&result.text);
    }
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
    #[ignore = "needs the model; set VOXERA_MODEL_DIR to the model directory"]
    fn transcribes_sample_wav() {
        // cargo runs tests with CWD = apps/cli, so resolve relative paths from the workspace root
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let dir = root
            .join(std::env::var("VOXERA_MODEL_DIR").expect("VOXERA_MODEL_DIR"))
            .display()
            .to_string();
        let model = ModelFiles::load(Path::new(&dir)).expect("model files");
        let wave = Wave::read(&format!("{dir}/test_wavs/0.wav")).expect("read wav");
        let mut text = String::new();
        transcribe(
            &wave,
            &model,
            &AsrOptions::default(),
            |_| {},
            |t| {
                text.push_str(t);
                text.push(' ');
            },
        )
        .expect("transcribe");
        assert!(
            text.split_whitespace().count() >= 3,
            "transcript too short: {text:?}"
        );
    }
}
