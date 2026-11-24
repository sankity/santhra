use std::process::Command;
use std::sync::OnceLock;

/// Installed font families, enumerated once per process. `fc-list` can report thousands.
pub fn families() -> &'static [String] {
    static FAMILIES: OnceLock<Vec<String>> = OnceLock::new();
    FAMILIES.get_or_init(query)
}

fn query() -> Vec<String> {
    let output = match Command::new("fc-list")
        .args(["--format", "%{file}\t%{family}\t%{style}\n"])
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };
    parse_families(&String::from_utf8_lossy(&output.stdout))
}

fn parse_families(text: &str) -> Vec<String> {
    let mut families: Vec<String> = Vec::new();
    for line in text.lines() {
        let mut fields = line.split('\t');
        let file = fields.next().unwrap_or("");
        if !file.ends_with(".ttf") && !file.ends_with(".otf") && !file.ends_with(".ttc") {
            continue;
        }
        let Some(names) = fields.next() else { continue };
        for name in names.split(',') {
            let name = name.trim();
            if !name.is_empty() && !families.iter().any(|f| f == name) {
                families.push(name.to_owned());
            }
        }
    }
    families.sort();
    families
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_families_and_ignores_bitmaps() {
        let text = "/x/A.ttf\tDejaVu Serif, Serif\tBook\n/x/B.ttc\tNoto Sans\n/x/C.pcf\tBad\n";
        assert_eq!(parse_families(text), ["DejaVu Serif", "Noto Sans", "Serif"]);
    }
}
