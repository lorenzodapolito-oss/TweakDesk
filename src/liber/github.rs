//! Ultima release di un repository GitHub e scelta del file da scaricare.

use crate::liber::net::Http;
use crate::liber::version;
use regex::Regex;
use serde::Deserialize;

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub size: u64,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

impl Release {
    /// Versione numerica ricavata dall'etichetta della release.
    pub fn version(&self) -> Option<String> {
        version::extract(&self.tag_name)
    }
}

pub fn latest(http: &dyn Http, repo: &str) -> Result<Release, String> {
    if !repo.contains('/') || repo.contains("..") || repo.contains(' ') {
        return Err(format!("Repository non valido: {repo}"));
    }
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let text = http.get_text(&url)?;
    let rel: Release =
        serde_json::from_str(&text).map_err(|e| format!("Risposta di GitHub non valida: {e}"))?;
    if rel.draft {
        return Err("L'ultima release è una bozza".into());
    }
    Ok(rel)
}

/// Il file della release che corrisponde all'espressione regolare.
pub fn pick_asset<'a>(rel: &'a Release, pattern: &str) -> Result<&'a Asset, String> {
    let re = Regex::new(pattern).map_err(|e| format!("Modello non valido: {e}"))?;
    rel.assets
        .iter()
        .find(|a| re.is_match(&a.name) && a.browser_download_url.starts_with("https://"))
        .ok_or_else(|| {
            format!(
                "Nella release {} non c'è nessun file che corrisponde a «{pattern}»",
                rel.tag_name
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::liber::net::fake::FakeHttp;

    const JSON: &str = r#"{"tag_name":"v8.7.1","draft":false,"assets":[
      {"name":"npp.8.7.1.Installer.x64.exe","browser_download_url":"https://x/i.exe","size":5},
      {"name":"npp.8.7.1.portable.x64.zip","browser_download_url":"https://x/p.zip","size":9}]}"#;

    #[test]
    fn parses_release_and_picks_asset() {
        let h = FakeHttp::default();
        h.texts
            .lock()
            .unwrap()
            .push(("notepad-plus-plus".into(), Ok(JSON.into())));
        let r = latest(&h, "notepad-plus-plus/notepad-plus-plus").unwrap();
        assert_eq!(r.version().as_deref(), Some("8.7.1"));
        let a = pick_asset(&r, r"(?i)\.portable\.x64\.zip$").unwrap();
        assert_eq!(a.browser_download_url, "https://x/p.zip");
        assert!(pick_asset(&r, r"nessuno")
            .unwrap_err()
            .contains("nessun file"));
        assert!(pick_asset(&r, "(").unwrap_err().contains("non valido"));
    }

    #[test]
    fn bad_repo_and_bad_json_are_errors() {
        let h = FakeHttp::default();
        assert!(latest(&h, "../x").is_err());
        assert!(latest(&h, "senza-slash").is_err());
        h.texts
            .lock()
            .unwrap()
            .push(("a/b".into(), Ok("non json".into())));
        assert!(latest(&h, "a/b").unwrap_err().contains("non valida"));
        h.texts
            .lock()
            .unwrap()
            .push(("c/d".into(), Ok(r#"{"tag_name":"v1","draft":true}"#.into())));
        assert!(latest(&h, "c/d").unwrap_err().contains("bozza"));
    }

    #[test]
    fn insecure_download_links_are_ignored() {
        let rel = Release {
            tag_name: "1".into(),
            draft: false,
            assets: vec![Asset {
                name: "a.zip".into(),
                browser_download_url: "http://x/a.zip".into(),
                size: 1,
            }],
        };
        assert!(pick_asset(&rel, r"\.zip$").is_err());
    }
}
