use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

const FAMILY: &str = "santhra-caption";

/// Installed font families reported by fontconfig.
pub fn installed() -> Vec<String> {
    match Command::new("fc-list")
        .args(["--format", "%{file}\t%{family}\t%{style}\n"])
        .output()
    {
        Ok(output) if output.status.success() => {
            parse_families(&String::from_utf8_lossy(&output.stdout))
        }
        _ => Vec::new(),
    }
}

/// The caption font family, or egui's default when no font is selected.
pub fn family(name: Option<&str>) -> FontFamily {
    match name {
        Some(_) => FontFamily::Name(FAMILY.into()),
        None => FontFamily::Proportional,
    }
}

pub fn apply(ctx: &egui::Context, name: Option<&str>) {
    let mut definitions = FontDefinitions::default();
    let mut stack = definitions
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    if let Some(bytes) = name
        .and_then(match_file)
        .and_then(|file| std::fs::read(file).ok())
    {
        definitions
            .font_data
            .insert(FAMILY.to_owned(), Arc::new(FontData::from_owned(bytes)));
        stack.insert(0, FAMILY.to_owned());
    }
    definitions
        .families
        .insert(FontFamily::Name(FAMILY.into()), stack);
    ctx.set_fonts(definitions);
}

fn match_file(name: &str) -> Option<PathBuf> {
    let output = Command::new("fc-match")
        .args(["--format", "%{file}", name])
        .output()
        .ok()?;
    let path = String::from_utf8(output.stdout).ok()?;
    let path = path.trim();
    (!path.is_empty()).then(|| PathBuf::from(path))
}

fn usable(file: &str) -> bool {
    let file = file.to_ascii_lowercase();
    file.ends_with(".ttf") || file.ends_with(".otf") || file.ends_with(".ttc")
}

fn parse_families(listing: &str) -> Vec<String> {
    let mut families: Vec<String> = listing
        .lines()
        .filter(|line| line.split('\t').next().is_some_and(usable))
        .filter_map(|line| line.split('\t').nth(1))
        .flat_map(|names| names.split(','))
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect();
    families.sort_unstable();
    families.dedup();
    families
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_families_from_fc_list() {
        let listing = "/usr/share/fonts/x.ttf\tNoto Sans,Noto Sans UI\tRegular\n\
                       /usr/share/fonts/y.ttf\tNoto Sans\tBold\n\
                       /usr/share/fonts/z.ttf\tAdwaita Sans\tRegular\n\
                       /usr/share/fonts/misc/cursor.pcf.gz\tcursor.pcf\tRegular\n\
                       /usr/share/fonts/w.ttf\t\tRegular\n";
        assert_eq!(
            parse_families(listing),
            ["Adwaita Sans", "Noto Sans", "Noto Sans UI"]
        );
    }
}
