//! Confronto e lettura delle versioni dai nomi dei file.

use regex::Regex;
use std::cmp::Ordering;
use std::sync::OnceLock;

/// Segmenti numerici di una versione (`1.2.3-beta` -> [1,2,3]).
pub fn segments(v: &str) -> Vec<u64> {
    v.trim()
        .trim_start_matches(['v', 'V'])
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<u64>().ok())
        .collect()
}

/// Confronto numerico segmento per segmento (i segmenti mancanti valgono 0).
pub fn compare(a: &str, b: &str) -> Ordering {
    let (sa, sb) = (segments(a), segments(b));
    for i in 0..sa.len().max(sb.len()) {
        let (x, y) = (
            sa.get(i).copied().unwrap_or(0),
            sb.get(i).copied().unwrap_or(0),
        );
        match x.cmp(&y) {
            Ordering::Equal => {}
            o => return o,
        }
    }
    Ordering::Equal
}

pub fn is_newer(latest: &str, local: &str) -> bool {
    !segments(latest).is_empty() && compare(latest, local) == Ordering::Greater
}

fn dotted_re() -> Option<&'static Regex> {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)(?:^|[^0-9a-z])v?(\d+(?:\.\d+){1,3})(?:[^0-9]|$)").ok())
        .as_ref()
}

fn underscored_re() -> Option<&'static Regex> {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)(?:^|[^0-9a-z])v?(\d+(?:_\d+){2,3})(?:[^0-9]|$)").ok())
        .as_ref()
}

fn sevenzip_re() -> Option<&'static Regex> {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)^7z(\d{2})(\d{2})").ok())
        .as_ref()
}

/// Versione ricavata dal nome del file: sceglie il numero con più punti
/// (così `win64` o `x64` non vengono scambiati per versioni).
pub fn from_filename(name: &str) -> Option<String> {
    let stem = name
        .rsplit_once('.')
        .map_or(name, |(s, ext)| if ext.len() <= 8 { s } else { name });
    if let Some(c) = sevenzip_re().and_then(|re| re.captures(stem)) {
        return Some(format!("{}.{}", &c[1], &c[2]));
    }
    let best = dotted_re().and_then(|re| {
        let mut best: Option<String> = None;
        let mut from = 0usize;
        while let Some(c) = re.captures(&stem[from..]) {
            let m = c.get(1)?;
            let cand = m.as_str().to_string();
            let better = best
                .as_ref()
                .is_none_or(|b| cand.matches('.').count() > b.matches('.').count());
            if better {
                best = Some(cand);
            }
            from += m.end();
            if from >= stem.len() {
                break;
            }
        }
        best
    });
    best.or_else(|| {
        underscored_re()
            .and_then(|re| re.captures(stem))
            .map(|c| c[1].replace('_', "."))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_numerically_not_as_text() {
        assert_eq!(compare("1.10.0", "1.9.9"), Ordering::Greater);
        assert_eq!(compare("2.0", "2.0.0"), Ordering::Equal);
        assert_eq!(compare("v3.1", "3.0.9"), Ordering::Greater);
        assert!(is_newer("131.0.6778.86", "130.0.6723.117"));
        assert!(!is_newer("1.0", "1.0.0"));
        assert!(!is_newer("boh", "1.0"));
    }

    #[test]
    fn reads_versions_from_real_world_file_names() {
        let cases = [
            ("vlc-3.0.20-win64.exe", "3.0.20"),
            ("python-3.13.1-amd64.exe", "3.13.1"),
            ("Git-2.47.1-64-bit.exe", "2.47.1"),
            ("7z2408-x64.exe", "24.08"),
            ("Firefox Setup 133.0.exe", "133.0"),
            ("node-v22.11.0-x64.msi", "22.11.0"),
            ("OBS-Studio-31.0.0-Windows-Installer.exe", "31.0.0"),
            ("winscp-6.3.5-setup.exe", "6.3.5"),
            ("Git_2_47_1.exe", "2.47.1"),
            ("npp.8.7.1.Installer.x64.exe", "8.7.1"),
        ];
        for (name, want) in cases {
            assert_eq!(from_filename(name).as_deref(), Some(want), "{name}");
        }
    }

    #[test]
    fn no_version_when_the_name_has_only_architecture_numbers() {
        assert_eq!(from_filename("ChromeSetup.exe"), None);
        assert_eq!(from_filename("setup-x64.exe"), None);
        assert_eq!(from_filename("tool_win64.msi"), None);
    }
}
