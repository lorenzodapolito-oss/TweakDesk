//! Interfaccia con winget (versione più recente e download degli installer).

use std::path::Path;
use std::process::{Command, Stdio};

/// Programma installato nel PC, come lo elenca `winget list`.
#[derive(Clone, Debug, PartialEq)]
pub struct Installed {
    pub name: String,
    pub id: String,
    pub version: String,
    /// Versione più recente disponibile, se winget la segnala.
    pub available: Option<String>,
}

pub trait Winget {
    /// Programmi installati nel PC che winget riconosce (hanno un id scaricabile).
    fn list_installed(&self) -> Result<Vec<Installed>, String>;
    /// Ultima versione disponibile per l'id.
    fn latest_version(&self, id: &str) -> Result<String, String>;
    /// Scarica l'installer dell'id dentro `dir`.
    fn download(&self, id: &str, dir: &Path) -> Result<(), String>;
}

pub struct RealWinget;

fn run(args: &[&str]) -> Result<String, String> {
    let out = Command::new("winget")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("winget non disponibile: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    if out.status.success() {
        Ok(text)
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(format!(
            "winget codice {:?}: {}{}",
            out.status.code(),
            text.trim(),
            err.trim()
        ))
    }
}

impl Winget for RealWinget {
    fn list_installed(&self) -> Result<Vec<Installed>, String> {
        let out = run(&[
            "list",
            "--source",
            "winget",
            "--accept-source-agreements",
            "--disable-interactivity",
        ])?;
        Ok(parse_list(&out))
    }

    fn latest_version(&self, id: &str) -> Result<String, String> {
        let out = run(&[
            "show",
            "--id",
            id,
            "--exact",
            "--source",
            "winget",
            "--accept-source-agreements",
            "--disable-interactivity",
        ])?;
        parse_show_version(&out).ok_or_else(|| format!("versione non trovata per {id}"))
    }

    fn download(&self, id: &str, dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let d = dir.to_string_lossy().to_string();
        run(&[
            "download",
            "--id",
            id,
            "--exact",
            "--source",
            "winget",
            "--download-directory",
            &d,
            "--accept-package-agreements",
            "--accept-source-agreements",
            "--disable-interactivity",
        ])
        .map(|_| ())
    }
}

/// Estrae `Versione: x` / `Version: x` dall'output di `winget show`.
pub fn parse_show_version(out: &str) -> Option<String> {
    out.lines().find_map(|line| {
        let line = line.trim();
        let (key, val) = line.split_once(':')?;
        let key = key.trim().to_lowercase();
        let val = val.trim();
        ((key == "version" || key == "versione") && !val.is_empty()).then(|| val.to_string())
    })
}

/// Legge la tabella di `winget list` (inglese o italiano): le colonne si ricavano dalle
/// posizioni delle intestazioni, quindi i nomi con spazi non creano problemi.
pub fn parse_list(out: &str) -> Vec<Installed> {
    let lines: Vec<Vec<char>> = out
        .lines()
        .map(|l| l.rsplit('\r').next().unwrap_or(l).chars().collect())
        .collect();
    let is_sep = |l: &Vec<char>| l.len() >= 10 && l.iter().all(|c| *c == '-');
    let Some(sep) = lines.iter().position(is_sep) else {
        return Vec::new();
    };
    let Some(header) = sep.checked_sub(1).and_then(|i| lines.get(i)) else {
        return Vec::new();
    };
    let mut starts: Vec<usize> = Vec::new();
    for (i, c) in header.iter().enumerate() {
        if *c != ' ' && (i == 0 || header.get(i - 1) == Some(&' ')) {
            starts.push(i);
        }
    }
    if starts.len() < 3 {
        return Vec::new();
    }
    let word = |k: usize| -> String {
        let a = starts.get(k).copied().unwrap_or(0);
        let b = starts.get(k + 1).copied().unwrap_or(header.len());
        header
            .get(a..b.min(header.len()))
            .map(|c| c.iter().collect::<String>().trim().to_lowercase())
            .unwrap_or_default()
    };
    let has_available = matches!(word(3).as_str(), "available" | "disponibile");
    let cell = |line: &[char], k: usize| -> String {
        let a = starts.get(k).copied().unwrap_or(line.len()).min(line.len());
        let b = starts
            .get(k + 1)
            .copied()
            .unwrap_or(line.len())
            .min(line.len());
        line.get(a..b)
            .map(|c| c.iter().collect::<String>().trim().to_string())
            .unwrap_or_default()
    };
    let clean = |v: String| v.trim_start_matches(['<', '>', '~', ' ']).to_string();
    let mut out_rows = Vec::new();
    for line in lines.iter().skip(sep + 1) {
        let (name, id, version) = (cell(line, 0), cell(line, 1), clean(cell(line, 2)));
        if name.is_empty() || id.is_empty() || id.contains(' ') || version.is_empty() {
            continue;
        }
        let available = if has_available {
            Some(cell(line, 3)).filter(|a| !a.is_empty())
        } else {
            None
        };
        out_rows.push(Installed {
            name,
            id,
            version,
            available,
        });
    }
    out_rows
}

#[cfg(test)]
pub mod fake {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    #[derive(Default)]
    pub struct FakeWinget {
        pub versions: HashMap<String, String>,
        pub downloaded: RefCell<Vec<String>>,
        /// Nome del file da creare nella cartella al download.
        pub file_name: String,
        pub fail_download: bool,
        pub installed: Vec<Installed>,
    }

    impl Winget for FakeWinget {
        fn list_installed(&self) -> Result<Vec<Installed>, String> {
            Ok(self.installed.clone())
        }
        fn latest_version(&self, id: &str) -> Result<String, String> {
            self.versions
                .get(id)
                .cloned()
                .ok_or_else(|| "sconosciuto".into())
        }
        fn download(&self, id: &str, dir: &Path) -> Result<(), String> {
            if self.fail_download {
                return Err("rete assente".into());
            }
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            std::fs::write(dir.join(&self.file_name), b"MZ fake installer")
                .map_err(|e| e.to_string())?;
            self.downloaded.borrow_mut().push(id.to_string());
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_winget_list_table() {
        let out = "\r-\r\\\r   \r\nNome                 ID                 Versione   Disponibile  Origine\r\n-----------------------------------------------------------------\r\nGit                  Git.Git            2.40.0     2.47.1       winget\r\nVLC media player     VideoLAN.VLC       3.0.21                  winget\r\nStrumento X          Vendor.X           < 1.2      1.3          winget\r\n2 aggiornamenti disponibili.\r\n";
        let v = parse_list(out);
        assert_eq!(v.len(), 3);
        assert_eq!(v[0].name, "Git");
        assert_eq!(v[0].available.as_deref(), Some("2.47.1"));
        assert_eq!(v[1].name, "VLC media player");
        assert_eq!(v[1].available, None);
        assert_eq!(v[2].version, "1.2");
        let no_avail = "Name   Id    Version  Source\n----------------------------\nGit    Git.Git   2.1   winget\n";
        let v = parse_list(no_avail);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].available, None);
        assert!(parse_list("nessuna tabella").is_empty());
        assert!(parse_list("").is_empty());
    }

    #[test]
    fn parses_english_and_italian_output() {
        let en = "Found Git [Git.Git]\nVersion: 2.47.1\nPublisher: The Git Development Community\n";
        let it = "Trovato VLC [VideoLAN.VLC]\nVersione: 3.0.21\n";
        assert_eq!(parse_show_version(en).as_deref(), Some("2.47.1"));
        assert_eq!(parse_show_version(it).as_deref(), Some("3.0.21"));
        assert_eq!(parse_show_version("niente"), None);
        assert_eq!(parse_show_version("Version:\n"), None);
    }
}
