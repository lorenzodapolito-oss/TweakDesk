//! Confronto delle versioni.

use std::cmp::Ordering;

/// Segmenti numerici di una versione (`v1.2.3-beta` -> [1,2,3]).
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

/// Prima versione con punti trovata in un testo (`release-8.7.1` -> `8.7.1`).
pub fn extract(text: &str) -> Option<String> {
    let re = regex::Regex::new(r"\d+(?:\.\d+)+").ok()?;
    if let Some(m) = re.find(text) {
        return Some(m.as_str().to_string());
    }
    let single = regex::Regex::new(r"\d+").ok()?;
    single.find(text).map(|m| m.as_str().to_string())
}

/// Versione con i punti scritta nel nome di una cartella (`KeePassXC-2.7.11-Win64` -> `2.7.11`).
pub fn from_folder_name(name: &str) -> Option<String> {
    let re = regex::Regex::new(r"\d+(?:\.\d+)+").ok()?;
    re.find(name).map(|m| m.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_numbers_not_text() {
        assert!(is_newer("8.10.0", "8.9.9"));
        assert!(is_newer("v2.0", "1.99.9"));
        assert!(!is_newer("1.2", "1.2.0"));
        assert!(!is_newer("1.2.3", "1.2.4"));
        assert!(!is_newer("senza numeri", "1.0"));
    }

    #[test]
    fn folder_name_versions() {
        assert_eq!(
            from_folder_name("KeePassXC-2.7.11-Win64").as_deref(),
            Some("2.7.11")
        );
        assert_eq!(from_folder_name("Rufus 2.11").as_deref(), Some("2.11"));
        assert_eq!(
            from_folder_name("qbittorrent_4.6.3_x64").as_deref(),
            Some("4.6.3")
        );
        assert_eq!(from_folder_name("MusicBee"), None);
        assert_eq!(from_folder_name("mp3gain-win-1_2_5"), None);
    }

    #[test]
    fn extracts_versions() {
        assert_eq!(extract("v8.7.1").as_deref(), Some("8.7.1"));
        assert_eq!(extract("release-2.7.10").as_deref(), Some("2.7.10"));
        assert_eq!(extract("build 42").as_deref(), Some("42"));
        assert_eq!(extract("latest"), None);
    }
}
