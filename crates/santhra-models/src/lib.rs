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

/// Language codes accepted by Whisper's multilingual tokenizer, plus automatic detection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpeechLanguage {
    pub id: &'static str,
    pub label: &'static str,
    pub whisper_code: Option<&'static str>,
}

const SPEECH_LANGUAGES: &[SpeechLanguage] = &[
    SpeechLanguage {
        id: "multilingual",
        label: "Auto-detect (multilingual)",
        whisper_code: None,
    },
    SpeechLanguage {
        id: "afrikaans",
        label: "Afrikaans",
        whisper_code: Some("af"),
    },
    SpeechLanguage {
        id: "albanian",
        label: "Albanian",
        whisper_code: Some("sq"),
    },
    SpeechLanguage {
        id: "amharic",
        label: "Amharic",
        whisper_code: Some("am"),
    },
    SpeechLanguage {
        id: "arabic",
        label: "Arabic",
        whisper_code: Some("ar"),
    },
    SpeechLanguage {
        id: "armenian",
        label: "Armenian",
        whisper_code: Some("hy"),
    },
    SpeechLanguage {
        id: "assamese",
        label: "Assamese",
        whisper_code: Some("as"),
    },
    SpeechLanguage {
        id: "azerbaijani",
        label: "Azerbaijani",
        whisper_code: Some("az"),
    },
    SpeechLanguage {
        id: "bashkir",
        label: "Bashkir",
        whisper_code: Some("ba"),
    },
    SpeechLanguage {
        id: "basque",
        label: "Basque",
        whisper_code: Some("eu"),
    },
    SpeechLanguage {
        id: "belarusian",
        label: "Belarusian",
        whisper_code: Some("be"),
    },
    SpeechLanguage {
        id: "bengali",
        label: "Bengali",
        whisper_code: Some("bn"),
    },
    SpeechLanguage {
        id: "bosnian",
        label: "Bosnian",
        whisper_code: Some("bs"),
    },
    SpeechLanguage {
        id: "breton",
        label: "Breton",
        whisper_code: Some("br"),
    },
    SpeechLanguage {
        id: "bulgarian",
        label: "Bulgarian",
        whisper_code: Some("bg"),
    },
    SpeechLanguage {
        id: "catalan",
        label: "Catalan",
        whisper_code: Some("ca"),
    },
    SpeechLanguage {
        id: "chinese",
        label: "Chinese",
        whisper_code: Some("zh"),
    },
    SpeechLanguage {
        id: "croatian",
        label: "Croatian",
        whisper_code: Some("hr"),
    },
    SpeechLanguage {
        id: "czech",
        label: "Czech",
        whisper_code: Some("cs"),
    },
    SpeechLanguage {
        id: "danish",
        label: "Danish",
        whisper_code: Some("da"),
    },
    SpeechLanguage {
        id: "dutch",
        label: "Dutch",
        whisper_code: Some("nl"),
    },
    SpeechLanguage {
        id: "english",
        label: "English",
        whisper_code: Some("en"),
    },
    SpeechLanguage {
        id: "estonian",
        label: "Estonian",
        whisper_code: Some("et"),
    },
    SpeechLanguage {
        id: "faroese",
        label: "Faroese",
        whisper_code: Some("fo"),
    },
    SpeechLanguage {
        id: "finnish",
        label: "Finnish",
        whisper_code: Some("fi"),
    },
    SpeechLanguage {
        id: "french",
        label: "French",
        whisper_code: Some("fr"),
    },
    SpeechLanguage {
        id: "galician",
        label: "Galician",
        whisper_code: Some("gl"),
    },
    SpeechLanguage {
        id: "georgian",
        label: "Georgian",
        whisper_code: Some("ka"),
    },
    SpeechLanguage {
        id: "german",
        label: "German",
        whisper_code: Some("de"),
    },
    SpeechLanguage {
        id: "greek",
        label: "Greek",
        whisper_code: Some("el"),
    },
    SpeechLanguage {
        id: "gujarati",
        label: "Gujarati",
        whisper_code: Some("gu"),
    },
    SpeechLanguage {
        id: "haitian-creole",
        label: "Haitian Creole",
        whisper_code: Some("ht"),
    },
    SpeechLanguage {
        id: "hausa",
        label: "Hausa",
        whisper_code: Some("ha"),
    },
    SpeechLanguage {
        id: "hawaiian",
        label: "Hawaiian",
        whisper_code: Some("haw"),
    },
    SpeechLanguage {
        id: "hebrew",
        label: "Hebrew",
        whisper_code: Some("he"),
    },
    SpeechLanguage {
        id: "hindi",
        label: "Hindi",
        whisper_code: Some("hi"),
    },
    SpeechLanguage {
        id: "hungarian",
        label: "Hungarian",
        whisper_code: Some("hu"),
    },
    SpeechLanguage {
        id: "icelandic",
        label: "Icelandic",
        whisper_code: Some("is"),
    },
    SpeechLanguage {
        id: "indonesia",
        label: "Indonesian",
        whisper_code: Some("id"),
    },
    SpeechLanguage {
        id: "italian",
        label: "Italian",
        whisper_code: Some("it"),
    },
    SpeechLanguage {
        id: "japanese",
        label: "Japanese",
        whisper_code: Some("ja"),
    },
    SpeechLanguage {
        id: "javanese",
        label: "Javanese",
        whisper_code: Some("jw"),
    },
    SpeechLanguage {
        id: "kannada",
        label: "Kannada",
        whisper_code: Some("kn"),
    },
    SpeechLanguage {
        id: "kazakh",
        label: "Kazakh",
        whisper_code: Some("kk"),
    },
    SpeechLanguage {
        id: "khmer",
        label: "Khmer",
        whisper_code: Some("km"),
    },
    SpeechLanguage {
        id: "korean",
        label: "Korean",
        whisper_code: Some("ko"),
    },
    SpeechLanguage {
        id: "lao",
        label: "Lao",
        whisper_code: Some("lo"),
    },
    SpeechLanguage {
        id: "latin",
        label: "Latin",
        whisper_code: Some("la"),
    },
    SpeechLanguage {
        id: "latvian",
        label: "Latvian",
        whisper_code: Some("lv"),
    },
    SpeechLanguage {
        id: "lingala",
        label: "Lingala",
        whisper_code: Some("ln"),
    },
    SpeechLanguage {
        id: "lithuanian",
        label: "Lithuanian",
        whisper_code: Some("lt"),
    },
    SpeechLanguage {
        id: "luxembourgish",
        label: "Luxembourgish",
        whisper_code: Some("lb"),
    },
    SpeechLanguage {
        id: "macedonian",
        label: "Macedonian",
        whisper_code: Some("mk"),
    },
    SpeechLanguage {
        id: "malagasy",
        label: "Malagasy",
        whisper_code: Some("mg"),
    },
    SpeechLanguage {
        id: "malay",
        label: "Malay",
        whisper_code: Some("ms"),
    },
    SpeechLanguage {
        id: "malayalam",
        label: "Malayalam",
        whisper_code: Some("ml"),
    },
    SpeechLanguage {
        id: "maltese",
        label: "Maltese",
        whisper_code: Some("mt"),
    },
    SpeechLanguage {
        id: "maori",
        label: "Maori",
        whisper_code: Some("mi"),
    },
    SpeechLanguage {
        id: "marathi",
        label: "Marathi",
        whisper_code: Some("mr"),
    },
    SpeechLanguage {
        id: "mongolian",
        label: "Mongolian",
        whisper_code: Some("mn"),
    },
    SpeechLanguage {
        id: "myanmar",
        label: "Myanmar",
        whisper_code: Some("my"),
    },
    SpeechLanguage {
        id: "nepali",
        label: "Nepali",
        whisper_code: Some("ne"),
    },
    SpeechLanguage {
        id: "norwegian",
        label: "Norwegian",
        whisper_code: Some("no"),
    },
    SpeechLanguage {
        id: "nynorsk",
        label: "Nynorsk",
        whisper_code: Some("nn"),
    },
    SpeechLanguage {
        id: "occitan",
        label: "Occitan",
        whisper_code: Some("oc"),
    },
    SpeechLanguage {
        id: "pashto",
        label: "Pashto",
        whisper_code: Some("ps"),
    },
    SpeechLanguage {
        id: "persian",
        label: "Persian",
        whisper_code: Some("fa"),
    },
    SpeechLanguage {
        id: "polish",
        label: "Polish",
        whisper_code: Some("pl"),
    },
    SpeechLanguage {
        id: "portuguese",
        label: "Portuguese",
        whisper_code: Some("pt"),
    },
    SpeechLanguage {
        id: "punjabi",
        label: "Punjabi",
        whisper_code: Some("pa"),
    },
    SpeechLanguage {
        id: "romanian",
        label: "Romanian",
        whisper_code: Some("ro"),
    },
    SpeechLanguage {
        id: "russian",
        label: "Russian",
        whisper_code: Some("ru"),
    },
    SpeechLanguage {
        id: "sanskrit",
        label: "Sanskrit",
        whisper_code: Some("sa"),
    },
    SpeechLanguage {
        id: "serbian",
        label: "Serbian",
        whisper_code: Some("sr"),
    },
    SpeechLanguage {
        id: "shona",
        label: "Shona",
        whisper_code: Some("sn"),
    },
    SpeechLanguage {
        id: "sindhi",
        label: "Sindhi",
        whisper_code: Some("sd"),
    },
    SpeechLanguage {
        id: "sinhala",
        label: "Sinhala",
        whisper_code: Some("si"),
    },
    SpeechLanguage {
        id: "slovak",
        label: "Slovak",
        whisper_code: Some("sk"),
    },
    SpeechLanguage {
        id: "slovenian",
        label: "Slovenian",
        whisper_code: Some("sl"),
    },
    SpeechLanguage {
        id: "somali",
        label: "Somali",
        whisper_code: Some("so"),
    },
    SpeechLanguage {
        id: "spanish",
        label: "Spanish",
        whisper_code: Some("es"),
    },
    SpeechLanguage {
        id: "sundanese",
        label: "Sundanese",
        whisper_code: Some("su"),
    },
    SpeechLanguage {
        id: "swahili",
        label: "Swahili",
        whisper_code: Some("sw"),
    },
    SpeechLanguage {
        id: "swedish",
        label: "Swedish",
        whisper_code: Some("sv"),
    },
    SpeechLanguage {
        id: "tagalog",
        label: "Tagalog",
        whisper_code: Some("tl"),
    },
    SpeechLanguage {
        id: "tajik",
        label: "Tajik",
        whisper_code: Some("tg"),
    },
    SpeechLanguage {
        id: "tamil",
        label: "Tamil",
        whisper_code: Some("ta"),
    },
    SpeechLanguage {
        id: "tatar",
        label: "Tatar",
        whisper_code: Some("tt"),
    },
    SpeechLanguage {
        id: "telugu",
        label: "Telugu",
        whisper_code: Some("te"),
    },
    SpeechLanguage {
        id: "thai",
        label: "Thai",
        whisper_code: Some("th"),
    },
    SpeechLanguage {
        id: "tibetan",
        label: "Tibetan",
        whisper_code: Some("bo"),
    },
    SpeechLanguage {
        id: "turkish",
        label: "Turkish",
        whisper_code: Some("tr"),
    },
    SpeechLanguage {
        id: "turkmen",
        label: "Turkmen",
        whisper_code: Some("tk"),
    },
    SpeechLanguage {
        id: "ukrainian",
        label: "Ukrainian",
        whisper_code: Some("uk"),
    },
    SpeechLanguage {
        id: "urdu",
        label: "Urdu",
        whisper_code: Some("ur"),
    },
    SpeechLanguage {
        id: "uzbek",
        label: "Uzbek",
        whisper_code: Some("uz"),
    },
    SpeechLanguage {
        id: "vietnamese",
        label: "Vietnamese",
        whisper_code: Some("vi"),
    },
    SpeechLanguage {
        id: "welsh",
        label: "Welsh",
        whisper_code: Some("cy"),
    },
    SpeechLanguage {
        id: "yiddish",
        label: "Yiddish",
        whisper_code: Some("yi"),
    },
    SpeechLanguage {
        id: "yoruba",
        label: "Yoruba",
        whisper_code: Some("yo"),
    },
    SpeechLanguage {
        id: "cantonese",
        label: "Cantonese",
        whisper_code: Some("yue"),
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
}

impl ModelKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ModelKind::ZipformerTransducer => "zipformer",
            ModelKind::ParaformerOnline => "paraformer-online",
            ModelKind::ParaformerOffline => "paraformer-offline",
            ModelKind::WhisperOffline => "whisper-offline",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        [
            ModelKind::ZipformerTransducer,
            ModelKind::ParaformerOnline,
            ModelKind::ParaformerOffline,
            ModelKind::WhisperOffline,
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
            selected == Language::Multilingual.as_str()
                || speech_language(selected).is_some_and(|language| language.whisper_code.is_some())
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

        for model in [whisper_tiny, whisper_base, whisper_small] {
            assert!(matches_language_id(model, "french"));
            assert!(matches_language_id(model, "japanese"));
            assert!(matches_language_id(model, "indonesia"));
            assert!(matches_language_id(model, "multilingual"));
            assert!(!matches_language_id(model, "not-a-language"));
        }
        assert!(matches_language_id(zipformer_english, "english"));
        assert!(!matches_language_id(zipformer_english, "french"));
        assert!(matches_language_id(paraformer_chinese, "chinese"));
        assert!(!matches_language_id(paraformer_chinese, "english"));
        assert_eq!(
            default_for_id("french").unwrap().id,
            "whisper-tiny-multilingual"
        );
        assert_eq!(default_for_id("english").unwrap().id, "zipformer-en-20m");
    }

    #[test]
    fn language_filter_lists_every_compatible_model() {
        let french_models = for_language_id("french");
        assert_eq!(french_models.len(), 3);
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
        assert_eq!(for_language_id("chinese").len(), 5);
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
