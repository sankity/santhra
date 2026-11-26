use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use audio_linux::Target;
use serde::{Deserialize, Serialize};

const DEFAULT_MODEL_DIR: &str = "models/sherpa-onnx-streaming-zipformer-en-20M-2023-02-17";

pub const BACKGROUND_PRESETS: [(&str, &str); 8] = [
    ("Dark", "#101014E6"),
    ("Black", "#000000E6"),
    ("Charcoal", "#1F1F24E6"),
    ("Slate", "#243244E6"),
    ("Navy", "#0B1B33E6"),
    ("Forest", "#0C2418E6"),
    ("Maroon", "#2A1010E6"),
    ("Plum", "#20102AE6"),
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    pub model_dir: String,
    pub threads: i32,
    pub max_lines: usize,
    pub show_partial: bool,
    pub language: Option<String>,
    pub model_id: Option<String>,
    pub model_kind: Option<String>,
    pub buffer_ms: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    pub font_size: f32,
    pub text_color: String,
    pub background_color: String,
    pub start_on_launch: bool,
    pub overlay_enabled: bool,
    pub overlay_always_on_top: bool,
    pub overlay_click_through: bool,
    pub overlay_opacity: f32,
    pub overlay_width: f32,
    pub overlay_height: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overlay_pos: Option<[f32; 2]>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            target: None,
            model_dir: DEFAULT_MODEL_DIR.into(),
            language: Some("english".into()),
            model_id: Some("zipformer-en-20m".into()),
            model_kind: Some("zipformer".into()),
            buffer_ms: 2500,
            threads: 2,
            max_lines: 2,
            show_partial: true,
            font_family: None,
            font_size: 32.0,
            text_color: "#FFFFFFFF".into(),
            background_color: "#101014E6".into(),
            start_on_launch: false,
            overlay_enabled: false,
            overlay_always_on_top: true,
            overlay_click_through: false,
            overlay_opacity: 0.85,
            overlay_width: 720.0,
            overlay_height: 180.0,
            overlay_pos: None,
        }
    }
}

impl Settings {
    pub fn path() -> PathBuf {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("santhra").join("config.toml")
    }

    pub fn load() -> Self {
        Self::load_from(&Self::path())
    }

    pub fn load_from(path: &Path) -> Self {
        let mut settings: Self = fs::read_to_string(path)
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default();

        if settings
            .language
            .as_deref()
            .is_some_and(|language| santhra_models::speech_language(language).is_none())
        {
            settings.language = Some("english".into());
            let model = settings.model_id.as_deref().and_then(santhra_models::entry);
            if model.is_none_or(|model| !santhra_models::matches_language_id(model, "english"))
                && let Some(default) = santhra_models::default_for_id("english")
            {
                settings.model_id = Some(default.id.into());
                settings.model_kind = Some(default.kind.as_str().into());
                settings.model_dir = santhra_models::model_dir(default.id)
                    .map(|dir| dir.display().to_string())
                    .unwrap_or(settings.model_dir);
            }
        }
        settings
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&Self::path())
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(self)?;
        fs::write(path, text).with_context(|| format!("cannot write {}", path.display()))
    }

    pub fn session_options(&self) -> app_core::SessionOptions {
        let selected = self.model_id.as_deref().and_then(santhra_models::entry);
        let model_dir = selected
            .map(|entry| {
                santhra_models::model_dir(entry.id)
                    .unwrap_or_else(|_| PathBuf::from(&self.model_dir))
            })
            .unwrap_or_else(|| PathBuf::from(&self.model_dir));
        let model_kind = selected
            .map(|entry| entry.kind.as_str().to_owned())
            .or_else(|| self.model_kind.clone())
            .unwrap_or_else(|| "zipformer".into());
        let whisper_language = self
            .language
            .as_deref()
            .and_then(santhra_models::whisper_language_code)
            .map(str::to_owned);
        app_core::SessionOptions {
            target: Target::from_key(self.target.as_deref()),
            model_dir,
            model_kind,
            buffer_ms: self.buffer_ms,
            language: whisper_language,
            threads: self.threads,
            provider: "cpu".into(),
            max_lines: self.max_lines,
            show_partial: self.show_partial,
        }
    }

    pub fn text_rgba(&self) -> [u8; 4] {
        parse_rgba(&self.text_color)
    }

    pub fn background_rgba(&self) -> [u8; 4] {
        parse_rgba(&self.background_color)
    }
}

pub fn parse_rgba(text: &str) -> [u8; 4] {
    let hex = text.trim().trim_start_matches('#');
    match (u32::from_str_radix(hex, 16), hex.len()) {
        (Ok(value), 6) => [(value >> 16) as u8, (value >> 8) as u8, value as u8, 255],
        (Ok(value), 8) => [
            (value >> 24) as u8,
            (value >> 16) as u8,
            (value >> 8) as u8,
            value as u8,
        ],
        _ => [255, 255, 255, 255],
    }
}

pub fn to_hex(rgba: [u8; 4]) -> String {
    format!(
        "#{:02X}{:02X}{:02X}{:02X}",
        rgba[0], rgba[1], rgba[2], rgba[3]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_through_file() {
        let dir = std::env::temp_dir().join(format!("santhra-settings-{}", std::process::id()));
        let path = dir.join("config.toml");
        let settings = Settings {
            font_size: 18.0,
            target: Some("sink.name".into()),
            overlay_enabled: true,
            overlay_opacity: 0.5,
            overlay_pos: Some([10.0, 20.0]),
            ..Default::default()
        };
        settings.save_to(&path).expect("save");
        assert_eq!(Settings::load_from(&path), settings);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn session_options_map_target() {
        let mut settings = Settings::default();
        assert_eq!(settings.session_options().target, Target::Default);

        settings.target = Some("app:645".into());
        assert_eq!(settings.session_options().target, Target::App(645));
    }

    #[test]
    fn defaults_save_and_load() {
        let dir = std::env::temp_dir().join(format!("santhra-defaults-{}", std::process::id()));
        let path = dir.join("config.toml");
        let settings = Settings::default();
        settings.save_to(&path).expect("save defaults");
        assert_eq!(Settings::load_from(&path), settings);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn session_options_use_selected_whisper_language_code() {
        let mut settings = Settings::default();
        settings.language = Some("french".into());
        settings.model_id = Some("whisper-tiny-multilingual".into());
        let options = settings.session_options();
        assert_eq!(options.language.as_deref(), Some("fr"));

        settings.language = Some("indonesia".into());
        assert_eq!(settings.session_options().language.as_deref(), Some("id"));

        settings.language = Some("multilingual".into());
        assert_eq!(settings.session_options().language, None);
    }

    #[test]
    fn unsupported_saved_language_falls_back_to_english_and_compatible_model() {
        let dir =
            std::env::temp_dir().join(format!("santhra-language-migration-{}", std::process::id()));
        let path = dir.join("config.toml");
        fs::create_dir_all(&dir).expect("dir");
        fs::write(
            &path,
            r#"language = "faroese"
model_id = "paraformer-zh-offline"
model_kind = "paraformer-offline"
"#,
        )
        .expect("write old settings");

        let settings = Settings::load_from(&path);
        assert_eq!(settings.language.as_deref(), Some("english"));
        assert_eq!(settings.model_id.as_deref(), Some("zipformer-en-20m"));
        assert_eq!(settings.model_kind.as_deref(), Some("zipformer"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn corrupt_file_falls_back_to_defaults() {
        let dir = std::env::temp_dir().join(format!("santhra-corrupt-{}", std::process::id()));
        let path = dir.join("config.toml");
        fs::create_dir_all(&dir).expect("dir");
        fs::write(&path, "this is [ not toml").expect("write");
        assert_eq!(Settings::load_from(&path), Settings::default());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let settings: Settings =
            toml::from_str("font_size = 20.0\nfuture_key = 1\n").expect("parse");
        assert_eq!(settings.font_size, 20.0);
        assert_eq!(settings.max_lines, Settings::default().max_lines);
    }

    #[test]
    fn background_presets_are_valid_and_include_default() {
        let default = Settings::default();
        assert!(
            BACKGROUND_PRESETS
                .iter()
                .any(|(_, value)| *value == default.background_color)
        );
        for (name, value) in BACKGROUND_PRESETS {
            assert!(
                value.len() == 9 && value.starts_with('#'),
                "{name}: {value}"
            );
            assert_eq!(parse_rgba(value)[3], 0xE6, "{name}");
        }
    }

    #[test]
    fn colours_parse_and_format() {
        assert_eq!(parse_rgba("#FF000080"), [255, 0, 0, 128]);
        assert_eq!(parse_rgba("00FF00"), [0, 255, 0, 255]);
        assert_eq!(parse_rgba("oops"), [255, 255, 255, 255]);
        assert_eq!(to_hex([255, 0, 0, 128]), "#FF000080");
    }
}
