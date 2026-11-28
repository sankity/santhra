use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use audio_linux::Target;
use serde::{Deserialize, Serialize};

/// Fallback only; the default model is resolved against the data dir so the app works when
/// launched from a `.desktop` file (CWD = `$HOME`).
const LEGACY_MODEL_DIR: &str = "models/sherpa-onnx-streaming-zipformer-en-20M-2023-02-17";

fn default_model_dir() -> String {
    santhra_models::model_dir("zipformer-en-20m")
        .map(|dir| dir.display().to_string())
        .unwrap_or_else(|_| LEGACY_MODEL_DIR.into())
}

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
    pub chunk_ms: u32,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overlay_pos: Option<[f32; 2]>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            target: None,
            model_dir: default_model_dir(),
            language: Some("english".into()),
            model_id: Some("zipformer-en-20m".into()),
            model_kind: Some("zipformer".into()),
            chunk_ms: 100,
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
        settings.normalize();
        settings
    }

    /// Repair configs written by older builds: unknown language, stale model id, and a
    /// CWD-relative `models/...` directory.
    fn normalize(&mut self) {
        if self
            .language
            .as_deref()
            .is_some_and(|language| santhra_models::speech_language(language).is_none())
        {
            self.language = Some("english".into());
        }

        let language = self.language.clone().unwrap_or_else(|| "english".into());
        let selected = self.model_id.as_deref().and_then(santhra_models::entry);

        if selected.is_some_and(|model| santhra_models::matches_language_id(model, &language)) {
            if let Some(id) = selected.map(|model| model.id)
                && let Ok(dir) = santhra_models::model_dir(id)
            {
                self.model_dir = dir.display().to_string();
            }
        } else if self.model_id.is_some() {
            // Stale or unknown model id: fall back to the default for the selected language.
            if let Some(default) = santhra_models::default_for_id(&language) {
                let dir = santhra_models::model_dir(default.id)
                    .map(|dir| dir.display().to_string())
                    .unwrap_or_else(|_| self.model_dir.clone());
                self.model_id = Some(default.id.into());
                self.model_kind = Some(default.kind.as_str().into());
                self.model_dir = dir;
            }
        } else if Path::new(&self.model_dir).is_relative() {
            // Legacy config with no model id: the old default was relative to the CWD.
            let name = Path::new(&self.model_dir)
                .file_name()
                .map(|name| name.to_owned())
                .unwrap_or_default();
            self.model_dir = santhra_models::models_root()
                .join(name)
                .display()
                .to_string();
        }
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
            chunk_ms: self.chunk_ms,
            buffer_ms: self.buffer_ms,
            language: whisper_language,
            threads: self.threads,
            provider: "cpu".into(),
            max_lines: self.max_lines,
            show_partial: self.show_partial,
        }
    }

    /// The text colour as a linear [`rune::Color`], decoded from the stored `#rrggbb[aa]` hex.
    pub fn text_color_value(&self) -> rune::Color {
        rune::color::hex(&self.text_color).unwrap_or(rune::Color::WHITE)
    }

    /// The background colour with `overlay_opacity` folded into the alpha channel.
    pub fn background_color_value(&self) -> rune::Color {
        let c = rune::color::hex(&self.background_color)
            .unwrap_or_else(|| rune::color::hex(BACKGROUND_PRESETS[0].1).unwrap());
        c.alpha(c.a * self.overlay_opacity.clamp(0.1, 1.0))
    }

    pub fn text_color_hex_no_alpha(&self) -> String {
        let raw = self.text_color.trim().trim_start_matches('#');
        format!("#{}", &raw[..raw.len().min(6)])
    }

    pub fn background_color_hex_no_alpha(&self) -> String {
        let raw = self.background_color.trim().trim_start_matches('#');
        format!("#{}", &raw[..raw.len().min(6)])
    }

    pub fn set_text_color(&mut self, hsv: rune_kit::Hsv) {
        self.text_color = hex_with_alpha(hsv, &self.text_color);
    }

    pub fn set_background_color(&mut self, hsv: rune_kit::Hsv) {
        self.background_color = hex_with_alpha(hsv, &self.background_color);
    }
}

fn hex_with_alpha(hsv: rune_kit::Hsv, current: &str) -> String {
    let [r, g, b] = hsv.to_srgb8();
    let a = parse_rgba(current)[3];
    format!("#{r:02X}{g:02X}{b:02X}{a:02X}")
}

/// Parse a `#rrggbb` or `#rrggbbaa` hex string into `[r, g, b, a]` bytes (sRGB, 0–255).
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

#[cfg(test)]
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
    fn session_options_carries_chunk_ms() {
        let settings = Settings {
            chunk_ms: 60,
            ..Default::default()
        };
        assert_eq!(settings.session_options().chunk_ms, 60);
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
        let mut settings = Settings {
            language: Some("french".into()),
            model_id: Some("whisper-tiny-multilingual".into()),
            ..Default::default()
        };
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
    fn colour_change_keeps_stored_alpha() {
        let mut settings = Settings::default();
        settings.set_text_color(rune_kit::Hsv::from_hex("#ff0000").unwrap());
        assert_eq!(settings.text_color, "#FF0000FF");

        settings.background_color = "#101014E6".into();
        settings.set_background_color(rune_kit::Hsv::from_hex("#00ff00").unwrap());
        assert_eq!(settings.background_color, "#00FF00E6");
    }

    #[test]
    fn colours_parse_and_format() {
        assert_eq!(parse_rgba("#FF000080"), [255, 0, 0, 128]);
        assert_eq!(parse_rgba("00FF00"), [0, 255, 0, 255]);
        assert_eq!(parse_rgba("oops"), [255, 255, 255, 255]);
        assert_eq!(to_hex([255, 0, 0, 128]), "#FF000080");
    }

    fn write_config(name: &str, body: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("santhra-{name}-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("config.toml");
        fs::write(&path, body).expect("write config");
        (dir, path)
    }

    #[test]
    fn valid_language_with_stale_model_id_is_repaired() {
        let (dir, path) = write_config(
            "stale-model",
            r#"language = "english"
model_id = "paraformer-zh-offline"
model_kind = "paraformer-offline"
model_dir = "models/paraformer-zh-offline"
"#,
        );

        let settings = Settings::load_from(&path);
        assert_eq!(settings.language.as_deref(), Some("english"));
        assert_eq!(settings.model_id.as_deref(), Some("zipformer-en-20m"));
        assert_eq!(settings.model_kind.as_deref(), Some("zipformer"));
        assert_eq!(
            settings.model_dir,
            santhra_models::model_dir("zipformer-en-20m")
                .unwrap()
                .display()
                .to_string()
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn default_config_resolves_model_dir_from_data_dir() {
        let dir = std::env::temp_dir().join(format!("santhra-data-dir-{}", std::process::id()));
        let path = dir.join("config.toml");
        let settings = Settings::load_from(&path);

        assert!(Path::new(&settings.model_dir).is_absolute());
        assert_eq!(
            settings.model_dir,
            santhra_models::model_dir("zipformer-en-20m")
                .unwrap()
                .display()
                .to_string()
        );
    }

    #[test]
    fn unknown_model_id_falls_back_to_language_default() {
        let (dir, path) = write_config(
            "unknown-model",
            r#"language = "indonesia"
model_id = "does-not-exist"
"#,
        );

        let settings = Settings::load_from(&path);
        assert_eq!(
            settings.model_id.as_deref(),
            Some("whisper-tiny-multilingual")
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn legacy_relative_model_dir_is_migrated() {
        let mut settings = Settings {
            model_id: None,
            model_dir: "models/legacy-model".into(),
            ..Default::default()
        };
        settings.normalize();

        assert_eq!(
            settings.model_dir,
            santhra_models::models_root()
                .join("legacy-model")
                .display()
                .to_string()
        );
    }

    #[test]
    fn legacy_absolute_model_dir_is_preserved() {
        let absolute = std::env::temp_dir().join("santhra-legacy-model");
        let mut settings = Settings {
            model_id: None,
            model_dir: absolute.display().to_string(),
            ..Default::default()
        };
        settings.normalize();

        assert_eq!(settings.model_dir, absolute.display().to_string());
    }
}
