//! Ricerca manuale di un aggiornamento per le app che non si aggiornano da sole.

use regex::Regex;

/// Testo da cercare: il nome senza versioni e architetture, più «portable download ultima versione».
pub fn query(name: &str) -> String {
    let cleaned = name.replace(['_', '-', '.'], " ");
    let skip = Regex::new(
        r"(?i)^(v?\d+[\d.,]*|x64|x86|x32|win64|win32|64bit|32bit|bin|paf|portable|english|ita)$",
    )
    .ok();
    let words: Vec<&str> = cleaned
        .split_whitespace()
        .filter(|w| skip.as_ref().is_none_or(|r| !r.is_match(w)))
        .collect();
    let base = if words.is_empty() {
        name.trim().to_string()
    } else {
        words.join(" ")
    };
    format!("{base} portable download ultima versione")
}

/// Codifica per un indirizzo web (solo caratteri sicuri, il resto in %XX).
fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else if b == b' ' {
            out.push('+');
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub fn url(name: &str) -> String {
    format!("https://www.google.com/search?q={}", encode(&query(name)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_drops_versions_and_architectures() {
        assert_eq!(
            query("Mini Lyrics 7.6.47"),
            "Mini Lyrics portable download ultima versione"
        );
        assert_eq!(
            query("processhacker-2.39-bin"),
            "processhacker portable download ultima versione"
        );
        assert_eq!(
            query("Lupas Rename 5.0"),
            "Lupas Rename portable download ultima versione"
        );
        assert_eq!(
            query("qbittorrent_4.6.3_x64"),
            "qbittorrent portable download ultima versione"
        );
        assert_eq!(query("2.2.0"), "2.2.0 portable download ultima versione");
    }

    #[test]
    fn url_is_encoded() {
        let u = url("Wise Disk Cleaner 10.4.1.789");
        assert_eq!(
            u,
            "https://www.google.com/search?q=Wise+Disk+Cleaner+portable+download+ultima+versione"
        );
        assert!(!url("a&b \"c\"").contains(['&', '"', ' ']) || url("a&b").contains("%26"));
        assert!(url("a&b").contains("a%26b"));
    }
}
