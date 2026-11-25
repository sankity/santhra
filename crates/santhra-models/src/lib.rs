use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    English,
    Chinese,
    Indonesia,
    Multilingual,
}

impl Language {
    pub const ALL: [Language; 4] = [
        Language::English,
        Language::Chinese,
        Language::Indonesia,
        Language::Multilingual,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Language::English => "english",
            Language::Chinese => "chinese",
            Language::Indonesia => "indonesia",
            Language::Multilingual => "multilingual",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Language::English => "English",
            Language::Chinese => "Chinese",
            Language::Indonesia => "Indonesia",
            Language::Multilingual => "Multilingual",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|lang| lang.as_str() == value)
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Recognition languages currently exposed by Santhra.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpeechLanguage {
    pub id: &'static str,
    pub label: &'static str,
    pub whisper_code: Option<&'static str>,
}

const SPEECH_LANGUAGES: &[SpeechLanguage] = &[
    SpeechLanguage {
        id: "english",
        label: "English",
        whisper_code: Some("en"),
    },
    SpeechLanguage {
        id: "chinese",
        label: "Mandarin Chinese",
        whisper_code: Some("zh"),
    },
    SpeechLanguage {
        id: "spanish",
        label: "Spanish",
        whisper_code: Some("es"),
    },
    SpeechLanguage {
        id: "hindi",
        label: "Hindi",
        whisper_code: Some("hi"),
    },
    SpeechLanguage {
        id: "indonesia",
        label: "Indonesian",
        whisper_code: Some("id"),
    },
    SpeechLanguage {
        id: "french",
        label: "French",
        whisper_code: Some("fr"),
    },
    SpeechLanguage {
        id: "german",
        label: "German",
        whisper_code: Some("de"),
    },
    SpeechLanguage {
        id: "japanese",
        label: "Japanese",
        whisper_code: Some("ja"),
    },
    SpeechLanguage {
        id: "portuguese",
        label: "Portuguese",
        whisper_code: Some("pt"),
    },
    SpeechLanguage {
        id: "russian",
        label: "Russian",
        whisper_code: Some("ru"),
    },
];

pub fn speech_languages() -> &'static [SpeechLanguage] {
    SPEECH_LANGUAGES
}

pub fn speech_language(id: &str) -> Option<&'static SpeechLanguage> {
    SPEECH_LANGUAGES.iter().find(|language| language.id == id)
}

pub fn whisper_language_code(id: &str) -> Option<&'static str> {
    speech_language(id).and_then(|language| language.whisper_code)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelKind {
    ZipformerTransducer,
    ParaformerOnline,
    ParaformerOffline,
    WhisperOffline,
    NemoStreamingTransducer,
}

impl ModelKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ModelKind::ZipformerTransducer => "zipformer",
            ModelKind::ParaformerOnline => "paraformer-online",
            ModelKind::ParaformerOffline => "paraformer-offline",
            ModelKind::WhisperOffline => "whisper-offline",
            ModelKind::NemoStreamingTransducer => "nemo-streaming",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        [
            ModelKind::ZipformerTransducer,
            ModelKind::ParaformerOnline,
            ModelKind::ParaformerOffline,
            ModelKind::WhisperOffline,
            ModelKind::NemoStreamingTransducer,
        ]
        .into_iter()
        .find(|kind| kind.as_str() == value)
    }
}

/// Per-checkpoint language claims. Whisper multilingual support is backed by its tokenizer's
/// language table; other architectures list only languages confirmed for that checkpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LanguageSupport {
    Languages(&'static [&'static str]),
    WhisperMultilingual,
}

#[derive(Clone, Copy, Debug)]
pub struct RemoteFile {
    pub name: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub bytes: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct ModelEntry {
    pub id: &'static str,
    pub label: &'static str,
    pub language: Language,
    pub language_support: LanguageSupport,
    pub kind: ModelKind,
    pub license: &'static str,
    pub source: &'static str,
    pub files: &'static [RemoteFile],
}

impl ModelEntry {
    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|file| file.bytes).sum()
    }
}

static CATALOG: &[ModelEntry] = &[
    ModelEntry {
        id: "zipformer-en-20m",
        label: "Zipformer EN 20M (streaming)",
        language: Language::English,
        language_support: LanguageSupport::Languages(&["english"]),
        kind: ModelKind::ZipformerTransducer,
        license: "Apache-2.0",
        source: "csukuangfj/sherpa-onnx-streaming-zipformer-en-20M-2023-02-17",
        files: &[
            RemoteFile {
                name: "encoder-epoch-99-avg-1.int8.onnx",
                url: "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-en-20M-2023-02-17/resolve/main/encoder-epoch-99-avg-1.int8.onnx",
                sha256: "3810755ce7c3ab26b42a8bcf39d191308fa27fb0f53358823ba46141d03b7eb3",
                bytes: 42_845_182,
            },
            RemoteFile {
                name: "decoder-epoch-99-avg-1.int8.onnx",
                url: "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-en-20M-2023-02-17/resolve/main/decoder-epoch-99-avg-1.int8.onnx",
                sha256: "21e2a2acd961b3ac72f55be2f10f1a285e1b0b0ba010d7c0b6eab141411b163c",
                bytes: 539_499,
            },
            RemoteFile {
                name: "joiner-epoch-99-avg-1.int8.onnx",
                url: "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-en-20M-2023-02-17/resolve/main/joiner-epoch-99-avg-1.int8.onnx",
                sha256: "e085d73b593cf9b0707f370dbd656d58327d3fe36d80d849202ef81df02cb01e",
                bytes: 259_572,
            },
            RemoteFile {
                name: "tokens.txt",
                url: "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-en-20M-2023-02-17/resolve/main/tokens.txt",
                sha256: "49e3c2646595fd907228b3c6787069658f67b17377c60aeb8619c4551b2316fb",
                bytes: 5_048,
            },
        ],
    },
    ModelEntry {
        id: "paraformer-zh-offline",
        label: "Paraformer ZH (offline, int8)",
        language: Language::Chinese,
        language_support: LanguageSupport::Languages(&["chinese"]),
        kind: ModelKind::ParaformerOffline,
        license: "MIT",
        source: "csukuangfj/sherpa-onnx-paraformer-zh-2023-03-28",
        files: &[
            RemoteFile {
                name: "model.int8.onnx",
                url: "https://huggingface.co/csukuangfj/sherpa-onnx-paraformer-zh-2023-03-28/resolve/main/model.int8.onnx",
                sha256: "9ada9127ca5b82320385ac12340eb8b05dee64fd45cf8cf593ec693826ec2fd7",
                bytes: 223_385_835,
            },
            RemoteFile {
                name: "tokens.txt",
                url: "https://huggingface.co/csukuangfj/sherpa-onnx-paraformer-zh-2023-03-28/resolve/main/tokens.txt",
                sha256: "59aba8873a2ed1e122c25fee421e25f283b63290efbde85c1f01a853d83cb6e6",
                bytes: 75_756,
            },
        ],
    },
    ModelEntry {
        id: "paraformer-zh-online",
        label: "Paraformer ZH (streaming, int8)",
        language: Language::Chinese,
        language_support: LanguageSupport::Languages(&["chinese"]),
        kind: ModelKind::ParaformerOnline,
        license: "Apache-2.0",
        source: "csukuangfj/sherpa-onnx-streaming-paraformer-zh",
        files: &[
            RemoteFile {
                name: "encoder.int8.onnx",
                url: "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-paraformer-zh/resolve/main/encoder.int8.onnx",
                sha256: "81a70226a8934e6ed92aa1d4fc486b428b5398e2f2619ed4897b7294cab90e9a",
                bytes: 165_462_184,
            },
            RemoteFile {
                name: "decoder.int8.onnx",
                url: "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-paraformer-zh/resolve/main/decoder.int8.onnx",
                sha256: "f3cca9f77bb9d93c8fcbfb63ae617b6b1ee96818df3aa3b151c40658fe38594f",
                bytes: 71_664_561,
            },
            RemoteFile {
                name: "tokens.txt",
                url: "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-paraformer-zh/resolve/main/tokens.txt",
                sha256: "59aba8873a2ed1e122c25fee421e25f283b63290efbde85c1f01a853d83cb6e6",
                bytes: 75_756,
            },
        ],
    },
    ModelEntry {
        id: "whisper-tiny-multilingual",
        label: "Whisper Tiny (multilingual, int8)",
        language: Language::Multilingual,
        language_support: LanguageSupport::WhisperMultilingual,
        kind: ModelKind::WhisperOffline,
        license: "MIT",
        source: "csukuangfj/sherpa-onnx-whisper-tiny",
        files: &[
            RemoteFile {
                name: "tiny-encoder.int8.onnx",
                url: "https://huggingface.co/csukuangfj/sherpa-onnx-whisper-tiny/resolve/main/tiny-encoder.int8.onnx",
                sha256: "d24fb083ae3b1041fc24e97971d60e280c9342201fbb67b0ab428a8b4a51a434",
                bytes: 12_937_772,
            },
            RemoteFile {
                name: "tiny-decoder.int8.onnx",
                url: "https://huggingface.co/csukuangfj/sherpa-onnx-whisper-tiny/resolve/main/tiny-decoder.int8.onnx",
                sha256: "d2fece8dd42771f1df975c6c0445770d0c292bf7547c2cae04a6c0cc57540925",
                bytes: 89_855_401,
            },
            RemoteFile {
                name: "tiny-tokens.txt",
                url: "https://huggingface.co/csukuangfj/sherpa-onnx-whisper-tiny/resolve/main/tiny-tokens.txt",
                sha256: "b34b360dbb493e781e479794586d661700670d65564001f23024971d1f2fa126",
                bytes: 816_730,
            },
        ],
    },
    ModelEntry {
        id: "whisper-base-multilingual",
        label: "Whisper Base (multilingual, int8)",
        language: Language::Multilingual,
        language_support: LanguageSupport::WhisperMultilingual,
        kind: ModelKind::WhisperOffline,
        license: "MIT",
        source: "csukuangfj/sherpa-onnx-whisper-base",
        files: &[
            RemoteFile {
                name: "base-encoder.int8.onnx",
                url: "https://huggingface.co/csukuangfj/sherpa-onnx-whisper-base/resolve/main/base-encoder.int8.onnx",
                sha256: "0b8fb1304b6109976038efff5ace81720e00386f3ff6b54ee8c75291ca0a1e11",
                bytes: 29_120_534,
            },
            RemoteFile {
                name: "base-decoder.int8.onnx",
                url: "https://huggingface.co/csukuangfj/sherpa-onnx-whisper-base/resolve/main/base-decoder.int8.onnx",
                sha256: "9759d217388a01b3a4c7c15533201067b48ae819c4daafc8624e64b9409dc02d",
                bytes: 130_672_026,
            },
            RemoteFile {
                name: "base-tokens.txt",
                url: "https://huggingface.co/csukuangfj/sherpa-onnx-whisper-base/resolve/main/base-tokens.txt",
                sha256: "b34b360dbb493e781e479794586d661700670d65564001f23024971d1f2fa126",
                bytes: 816_730,
            },
        ],
    },
    ModelEntry {
        id: "whisper-small-multilingual",
        label: "Whisper Small (multilingual, int8)",
        language: Language::Multilingual,
        language_support: LanguageSupport::WhisperMultilingual,
        kind: ModelKind::WhisperOffline,
        license: "MIT",
        source: "csukuangfj/sherpa-onnx-whisper-small",
        files: &[
            RemoteFile {
                name: "small-encoder.int8.onnx",
                url: "https://huggingface.co/csukuangfj/sherpa-onnx-whisper-small/resolve/main/small-encoder.int8.onnx",
                sha256: "4cbe7b22fa9026b843b60a68640c747de05bafb1a11b57edc0e66c232d9f33a9",
                bytes: 112_442_483,
            },
            RemoteFile {
                name: "small-decoder.int8.onnx",
                url: "https://huggingface.co/csukuangfj/sherpa-onnx-whisper-small/resolve/main/small-decoder.int8.onnx",
                sha256: "acad50b5c782696e91b55914cc5ab4f756f1532f76e22aa6fc615f39fb69a8ee",
                bytes: 262_226_114,
            },
            RemoteFile {
                name: "small-tokens.txt",
                url: "https://huggingface.co/csukuangfj/sherpa-onnx-whisper-small/resolve/main/small-tokens.txt",
                sha256: "b34b360dbb493e781e479794586d661700670d65564001f23024971d1f2fa126",
                bytes: 816_730,
            },
        ],
    },
    // Whisper tiny fine-tune for Indonesian (maleo-ai/whisper-tiny-id), Apache-2.0.
    // Monolingual Indonesian. Offline Whisper -> final text only (no partials).
    // fp32 ONNX re-hosted from the sherpa-onnx export; int8 measured ~10 WER points worse here.
    ModelEntry {
        id: "whisper-tiny-id",
        label: "Whisper Tiny ID (Indonesian, offline, fp32)",
        language: Language::Indonesia,
        language_support: LanguageSupport::Languages(&["indonesia"]),
        kind: ModelKind::WhisperOffline,
        license: "Apache-2.0",
        source: "maleo-ai/whisper-tiny-id",
        files: &[
            RemoteFile {
                name: "tiny-id-encoder.onnx",
                url: "https://huggingface.co/sankity/sherpa-onnx-whisper-tiny-id/resolve/main/tiny-id-encoder.onnx",
                sha256: "6f26efab2fbe9d88bb00f8a72e8fd9b57baf4209c0b2662d35169a59e907bcdc",
                bytes: 37_634_686,
            },
            RemoteFile {
                name: "tiny-id-decoder.onnx",
                url: "https://huggingface.co/sankity/sherpa-onnx-whisper-tiny-id/resolve/main/tiny-id-decoder.onnx",
                sha256: "2a445dae36318dc462091acf540a117e950ea9c7cfb6ee5731dfd99ac42f06ef",
                bytes: 114_492_417,
            },
            RemoteFile {
                name: "tiny-id-tokens.txt",
                url: "https://huggingface.co/sankity/sherpa-onnx-whisper-tiny-id/resolve/main/tiny-id-tokens.txt",
                sha256: "b34b360dbb493e781e479794586d661700670d65564001f23024971d1f2fa126",
                bytes: 816_730,
            },
        ],
    },
    // NVIDIA Nemotron 3.5 ASR: cache-aware FastConformer-RNNT with a language-ID prompt.
    // sherpa-onnx picks its NeMo streaming implementation from the decoder ONNX itself; the
    // language is a per-stream option (see speech-sherpa). Indonesian is absent from the model's
    // 39 locale tags, so it is deliberately not listed in language_support.
    ModelEntry {
        id: "nemotron-3-5-asr-560ms",
        label: "Nemotron 3.5 ASR 0.6B (streaming, multilingual, int8)",
        language: Language::Multilingual,
        language_support: LanguageSupport::Languages(&[
            "english",
            "chinese",
            "spanish",
            "hindi",
            "french",
            "german",
            "japanese",
            "portuguese",
            "russian",
        ]),
        kind: ModelKind::NemoStreamingTransducer,
        license: "OpenMDW-1.1",
        source: "csukuangfj2/sherpa-onnx-nemotron-3.5-asr-streaming-0.6b-560ms-int8-2026-06-11",
        files: &[
            RemoteFile {
                name: "encoder.int8.onnx",
                url: "https://huggingface.co/csukuangfj2/sherpa-onnx-nemotron-3.5-asr-streaming-0.6b-560ms-int8-2026-06-11/resolve/main/encoder.int8.onnx",
                sha256: "012e9321373af99021415e0b0eb3ec827b4be3153be6f30d9b448fe65e896e68",
                bytes: 657_601_403,
            },
            RemoteFile {
                name: "decoder.int8.onnx",
                url: "https://huggingface.co/csukuangfj2/sherpa-onnx-nemotron-3.5-asr-streaming-0.6b-560ms-int8-2026-06-11/resolve/main/decoder.int8.onnx",
                sha256: "19f9c98fc6d0a2c33a65a43b36fdb2e914c26c0aa9764be3aebc502a1e982fb0",
                bytes: 14_978_075,
            },
            RemoteFile {
                name: "joiner.int8.onnx",
                url: "https://huggingface.co/csukuangfj2/sherpa-onnx-nemotron-3.5-asr-streaming-0.6b-560ms-int8-2026-06-11/resolve/main/joiner.int8.onnx",
                sha256: "4101c7c679a0bc30483794b27a059e34e79232aa2068d78d51231a22c8b0d7ce",
                bytes: 9_504_438,
            },
            RemoteFile {
                name: "tokens.txt",
                url: "https://huggingface.co/csukuangfj2/sherpa-onnx-nemotron-3.5-asr-streaming-0.6b-560ms-int8-2026-06-11/resolve/main/tokens.txt",
                sha256: "729cc103155bafa785f9cd45746cd41cabe97eab7182fc04d594129587958f8a",
                bytes: 131_440,
            },
        ],
    },
];

pub fn catalog() -> &'static [ModelEntry] {
    CATALOG
}

pub fn entry(id: &str) -> Option<&'static ModelEntry> {
    CATALOG.iter().find(|entry| entry.id == id)
}

pub fn matches_language(model: &ModelEntry, selected: Language) -> bool {
    matches_language_id(model, selected.as_str())
}

pub fn matches_language_id(model: &ModelEntry, selected: &str) -> bool {
    match model.language_support {
        LanguageSupport::Languages(languages) => languages.contains(&selected),
        LanguageSupport::WhisperMultilingual => {
            speech_language(selected).is_some_and(|language| language.whisper_code.is_some())
        }
    }
}

pub fn for_language(language: Language) -> Vec<&'static ModelEntry> {
    CATALOG
        .iter()
        .filter(|model| matches_language(model, language))
        .collect()
}

pub fn for_language_id(language: &str) -> Vec<&'static ModelEntry> {
    CATALOG
        .iter()
        .filter(|model| matches_language_id(model, language))
        .collect()
}

pub fn default_for(language: Language) -> &'static ModelEntry {
    match language {
        Language::English => entry("zipformer-en-20m").unwrap(),
        Language::Chinese => entry("paraformer-zh-offline").unwrap(),

        Language::Indonesia | Language::Multilingual => entry("whisper-tiny-multilingual").unwrap(),
    }
}

pub fn default_for_id(language: &str) -> Option<&'static ModelEntry> {
    let option = speech_language(language)?;
    Some(match Language::parse(option.id) {
        Some(Language::English) => default_for(Language::English),
        Some(Language::Chinese) => default_for(Language::Chinese),

        Some(Language::Indonesia) => default_for(Language::Indonesia),
        Some(Language::Multilingual) => default_for(Language::Multilingual),
        None => default_for(Language::Multilingual),
    })
}

pub fn models_root() -> PathBuf {
    if let Some(dir) = std::env::var_os("SANTHRA_MODELS_DIR") {
        return PathBuf::from(dir);
    }
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("share"))
        })
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("santhra").join("models")
}

pub fn model_dir(id: &str) -> Result<PathBuf> {
    if id.is_empty() || id.contains('/') || id.contains('\\') || id == "." || id == ".." {
        bail!("invalid model id: {id}");
    }
    Ok(models_root().join(id))
}

pub fn verify_file(path: &Path, expected: &RemoteFile) -> Result<()> {
    let meta = std::fs::metadata(path).with_context(|| format!("missing {}", path.display()))?;
    if !meta.is_file() {
        bail!("{} is not a regular file", path.display());
    }
    if meta.len() != expected.bytes {
        bail!(
            "{} has {} bytes, expected {}",
            path.display(),
            meta.len(),
            expected.bytes
        );
    }
    let mut file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = format!("{:x}", hasher.finalize());
    if digest != expected.sha256 {
        bail!("checksum mismatch for {}", path.display());
    }
    Ok(())
}

/// Cheap UI check: files exist and have the expected size. No hashing.
pub fn files_present(entry: &ModelEntry, dir: &Path) -> bool {
    entry.files.iter().all(|file| {
        std::fs::metadata(dir.join(file.name))
            .map(|meta| meta.is_file() && meta.len() == file.bytes)
            .unwrap_or(false)
    })
}

pub struct ResolvedModel {
    pub entry: &'static ModelEntry,
    pub dir: PathBuf,
}

pub fn resolve(entry: &'static ModelEntry, dir: &Path) -> Result<ResolvedModel> {
    for file in entry.files {
        verify_file(&dir.join(file.name), file)?;
    }
    Ok(ResolvedModel {
        entry,
        dir: dir.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_consistent() {
        for item in catalog() {
            assert!(
                item.id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-')
            );
            assert!(item.source.split_once('/').is_some());
            assert!(!item.files.is_empty());
            for file in item.files {
                assert!(file.url.starts_with("https://huggingface.co/"));
                assert_eq!(file.sha256.len(), 64);
                assert!(file.bytes > 0);
            }
        }
    }

    #[test]
    fn language_and_kind_roundtrip() {
        for language in Language::ALL {
            assert_eq!(Language::parse(language.as_str()), Some(language));
        }
        for option in speech_languages() {
            assert_eq!(speech_language(option.id), Some(option));
        }
        for entry in catalog() {
            assert_eq!(ModelKind::parse(entry.kind.as_str()), Some(entry.kind));
        }
    }

    #[test]
    fn model_language_capabilities_are_explicit() {
        let whisper_tiny = entry("whisper-tiny-multilingual").unwrap();
        let whisper_base = entry("whisper-base-multilingual").unwrap();
        let whisper_small = entry("whisper-small-multilingual").unwrap();
        let zipformer_english = entry("zipformer-en-20m").unwrap();
        let paraformer_chinese = entry("paraformer-zh-offline").unwrap();
        let nemotron = entry("nemotron-3-5-asr-560ms").unwrap();

        for model in [whisper_tiny, whisper_base, whisper_small] {
            assert!(matches_language_id(model, "french"));
            assert!(matches_language_id(model, "japanese"));
            assert!(matches_language_id(model, "indonesia"));
            assert!(!matches_language_id(model, "multilingual"));
            assert!(!matches_language_id(model, "afrikaans"));
            assert!(!matches_language_id(model, "not-a-language"));
        }
        assert!(matches_language_id(zipformer_english, "english"));
        assert!(!matches_language_id(zipformer_english, "french"));
        assert!(matches_language_id(paraformer_chinese, "chinese"));
        assert!(!matches_language_id(paraformer_chinese, "english"));
        for language in [
            "english",
            "chinese",
            "spanish",
            "hindi",
            "french",
            "german",
            "japanese",
            "portuguese",
            "russian",
        ] {
            assert!(matches_language_id(nemotron, language));
        }
        assert!(!matches_language_id(nemotron, "indonesia"));
        assert!(!matches_language_id(nemotron, "multilingual"));
        let whisper_tiny_id = entry("whisper-tiny-id").unwrap();
        assert!(matches_language_id(whisper_tiny_id, "indonesia"));
        assert!(!matches_language_id(whisper_tiny_id, "english"));
        assert!(!matches_language_id(whisper_tiny_id, "multilingual"));
        assert_eq!(
            default_for_id("french").unwrap().id,
            "whisper-tiny-multilingual"
        );
        assert_eq!(default_for_id("english").unwrap().id, "zipformer-en-20m");
        assert!(default_for_id("afrikaans").is_none());
        assert!(default_for_id("multilingual").is_none());
    }

    #[test]
    fn selectable_languages_are_limited_to_the_supported_set() {
        let ids: Vec<_> = speech_languages()
            .iter()
            .map(|language| language.id)
            .collect();
        assert_eq!(
            ids,
            [
                "english",
                "chinese",
                "spanish",
                "hindi",
                "indonesia",
                "french",
                "german",
                "japanese",
                "portuguese",
                "russian",
            ]
        );
        assert_eq!(
            speech_language("chinese").unwrap().label,
            "Mandarin Chinese"
        );
        assert!(speech_language("multilingual").is_none());
    }

    #[test]
    fn language_filter_lists_every_compatible_model() {
        let french_models = for_language_id("french");
        assert_eq!(french_models.len(), 4);
        assert!(
            french_models
                .iter()
                .any(|model| model.id == "nemotron-3-5-asr-560ms")
        );
        assert!(
            french_models
                .iter()
                .any(|model| model.id == "whisper-tiny-multilingual")
        );
        assert!(
            french_models
                .iter()
                .any(|model| model.id == "whisper-base-multilingual")
        );
        assert!(
            french_models
                .iter()
                .any(|model| model.id == "whisper-small-multilingual")
        );
        assert_eq!(for_language_id("chinese").len(), 6);
        let indonesia_models = for_language_id("indonesia");
        assert_eq!(indonesia_models.len(), 4);
        assert!(
            indonesia_models
                .iter()
                .any(|model| model.id == "whisper-tiny-id")
        );
        assert!(for_language_id("unknown").is_empty());
    }

    #[test]
    fn whisper_language_codes_are_available_for_multilingual_choices() {
        assert_eq!(whisper_language_code("spanish"), Some("es"));
        assert_eq!(whisper_language_code("indonesia"), Some("id"));
        assert_eq!(whisper_language_code("multilingual"), None);
        assert_eq!(whisper_language_code("unknown"), None);
    }

    #[test]
    fn model_ids_cannot_escape_root() {
        assert!(model_dir("../evil").is_err());
        assert!(model_dir("a/b").is_err());
        assert!(model_dir("good-id").is_ok());
    }
}
