use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const DEFAULT_MODEL_DIR: &str = "models/sherpa-onnx-streaming-zipformer-en-20M-2023-02-17";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    pub model_dir: String,
    pub threads: i32,
    pub max_lines: usize,
    pub show_partial: bool,
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
            threads: 2,
            max_lines: 2,
            show_partial: true,
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
        base.join("voxera").join("config.toml")
    }

    pub fn load() -> Self {
        Self::load_from(&Self::path())
    }

    pub fn load_from(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
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
        app_core::SessionOptions {
            target: self.target.clone(),
            model_dir: PathBuf::from(&self.model_dir),
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
        let dir = std::env::temp_dir().join(format!("voxera-settings-{}", std::process::id()));
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
    fn defaults_save_and_load() {
        let dir = std::env::temp_dir().join(format!("voxera-defaults-{}", std::process::id()));
        let path = dir.join("config.toml");
        let settings = Settings::default();
        settings.save_to(&path).expect("save defaults");
        assert_eq!(Settings::load_from(&path), settings);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn corrupt_file_falls_back_to_defaults() {
        let dir = std::env::temp_dir().join(format!("voxera-corrupt-{}", std::process::id()));
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
    fn colours_parse_and_format() {
        assert_eq!(parse_rgba("#FF000080"), [255, 0, 0, 128]);
        assert_eq!(parse_rgba("00FF00"), [0, 255, 0, 255]);
        assert_eq!(parse_rgba("oops"), [255, 255, 255, 255]);
        assert_eq!(to_hex([255, 0, 0, 128]), "#FF000080");
    }
}
