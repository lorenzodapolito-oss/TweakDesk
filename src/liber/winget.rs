//! Aggiornamento dei programmi non portabili con winget (cerca, confronta, scarica).

use crate::liber::version;
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Out {
    pub code: i32,
    pub text: String,
}

pub trait Runner: Send + Sync {
    fn run(&self, program: &str, args: &[&str]) -> Result<Out, String>;
}

pub struct RealRunner;

impl Runner for RealRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<Out, String> {
        let mut c = std::process::Command::new(program);
        c.args(args);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(0x0800_0000);
        }
        let o = c.output().map_err(|e| format!("{program}: {e}"))?;
        Ok(Out {
            code: o.status.code().unwrap_or(-1),
            text: String::from_utf8_lossy(&o.stdout).to_string(),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pkg {
    pub name: String,
    pub id: String,
    pub version: String,
}

/// Legge la tabella stampata da `winget search` (le colonne sono separate da almeno due spazi).
pub fn parse_table(text: &str) -> Vec<Pkg> {
    let Ok(split) = Regex::new(r"\s{2,}") else {
        return Vec::new();
    };
    let mut after_rule = false;
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = raw.trim_end_matches('\r');
        let t = line.trim();
        if t.len() > 5 && t.chars().all(|c| c == '-') {
            after_rule = true;
            continue;
        }
        if !after_rule || t.is_empty() {
            continue;
        }
        let parts: Vec<&str> = split.split(t).collect();
        if parts.len() >= 3 && !parts[1].contains(' ') {
            out.push(Pkg {
                name: parts[0].to_string(),
                id: parts[1].to_string(),
                version: parts[2].trim_start_matches(['<', '>', ' ']).to_string(),
            });
        }
    }
    out
}

const COMMON: [&str; 3] = ["--source", "winget", "--accept-source-agreements"];

pub fn available(r: &dyn Runner) -> bool {
    r.run("winget", &["--version"]).is_ok_and(|o| o.code == 0)
}

pub fn search(r: &dyn Runner, query: &str) -> Result<Vec<Pkg>, String> {
    let mut args = vec!["search", query];
    args.extend(COMMON);
    args.push("--disable-interactivity");
    Ok(parse_table(&r.run("winget", &args)?.text))
}

/// Ultima versione di un pacchetto (ricerca esatta per ID, indipendente dalla lingua).
pub fn latest(r: &dyn Runner, id: &str) -> Result<Option<String>, String> {
    let mut args = vec!["search", "--id", id, "--exact"];
    args.extend(COMMON);
    args.push("--disable-interactivity");
    let pk = parse_table(&r.run("winget", &args)?.text);
    Ok(pk
        .into_iter()
        .find(|p| p.id.eq_ignore_ascii_case(id))
        .and_then(|p| version::extract(&p.version)))
}

/// Scarica l'installer (senza installarlo) nella cartella e restituisce il file `.exe` o `.msi`.
pub fn download(r: &dyn Runner, id: &str, dir: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let d = dir.to_string_lossy().to_string();
    let mut args = vec!["download", "--id", id, "--exact"];
    args.extend(COMMON);
    args.extend([
        "--download-directory",
        d.as_str(),
        "--accept-package-agreements",
        "--disable-interactivity",
    ]);
    let o = r.run("winget", &args)?;
    let best = fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .filter(|e| {
            e.path().extension().is_some_and(|x| {
                let x = x.to_string_lossy().to_lowercase();
                x == "exe" || x == "msi"
            })
        })
        .filter_map(|e| Some((e.metadata().ok()?.len(), e.path())))
        .max_by_key(|(n, _)| *n)
        .map(|(_, p)| p);
    best.ok_or_else(|| {
        format!(
            "winget non ha scaricato nessun installer (codice {}): {}",
            o.code,
            o.text
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("")
                .trim()
        )
    })
}

/// File scaricato da winget e tipo di installer dichiarato nel manifest.
pub struct Got {
    pub file: PathBuf,
    pub kind: Option<String>,
}

/// Tipo dichiarato nel manifest (`NestedInstallerType` se presente, altrimenti `InstallerType`).
pub fn manifest_kind(yaml: &str) -> Option<String> {
    let find = |key: &str| {
        yaml.lines().find_map(|l| {
            let t = l.trim().trim_start_matches("- ").trim();
            t.strip_prefix(key)
                .map(|v| v.trim().trim_matches(['"', '\'']).to_lowercase())
                .filter(|v| !v.is_empty())
        })
    };
    find("NestedInstallerType:").or_else(|| find("InstallerType:"))
}

/// Si può usare come app portabile? Solo archivi o exe dichiarati «portable»: gli installer no.
pub fn is_portable_package(ext: &str, kind: Option<&str>) -> bool {
    matches!(
        (ext, kind),
        ("zip" | "7z" | "exe", Some("portable")) | ("zip", Some("zip")) | ("zip" | "7z", None)
    )
}

/// Come `download`, ma accetta anche zip e 7z e legge il tipo dal manifest.
pub fn download_any(r: &dyn Runner, id: &str, dir: &Path) -> Result<Got, String> {
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let d = dir.to_string_lossy().to_string();
    let mut args = vec!["download", "--id", id, "--exact"];
    args.extend(COMMON);
    args.extend([
        "--download-directory",
        d.as_str(),
        "--accept-package-agreements",
        "--disable-interactivity",
    ]);
    let o = r.run("winget", &args)?;
    let mut best: Option<(u64, PathBuf)> = None;
    let mut kind = None;
    for e in fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        let p = e.path();
        let x = p
            .extension()
            .map(|x| x.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if x == "yaml" || x == "yml" {
            if let Ok(t) = fs::read_to_string(&p) {
                kind = kind.or_else(|| manifest_kind(&t));
            }
        } else if matches!(x.as_str(), "exe" | "msi" | "zip" | "7z" | "msix" | "appx") {
            let n = e.metadata().map(|m| m.len()).unwrap_or(0);
            if best.as_ref().is_none_or(|(b, _)| n > *b) {
                best = Some((n, p));
            }
        }
    }
    match best {
        Some((_, file)) => Ok(Got { file, kind }),
        None => Err(format!(
            "winget non ha scaricato nessun file (codice {}): {}",
            o.code,
            o.text
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("")
                .trim()
        )),
    }
}

#[cfg(test)]
pub mod fake {
    use super::*;
    use std::sync::Mutex;

    type Hook = Box<dyn Fn(&[&str]) + Send + Sync>;

    #[derive(Default)]
    pub struct FakeRunner {
        pub responses: Mutex<Vec<(String, i32, String)>>,
        pub hooks: Mutex<Vec<(String, Hook)>>,
        pub calls: Mutex<Vec<String>>,
    }

    impl Runner for FakeRunner {
        fn run(&self, _program: &str, args: &[&str]) -> Result<Out, String> {
            let line = args.join(" ");
            self.calls.lock().unwrap().push(line.clone());
            for (k, h) in self.hooks.lock().unwrap().iter() {
                if line.contains(k.as_str()) {
                    h(args);
                }
            }
            let rs = self.responses.lock().unwrap();
            let (code, text) = rs
                .iter()
                .find(|(k, _, _)| line.contains(k.as_str()))
                .map_or((1, String::new()), |(_, c, t)| (*c, t.clone()));
            Ok(Out { code, text })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeRunner;
    use super::*;

    const TABLE: &str = "Nome                       ID                    Versione  Origine\r\n\
-----------------------------------------------------------------\r\n\
Mozilla Firefox (it)       Mozilla.Firefox.it    150.0     winget\r\n\
Firefox Developer Edition  Mozilla.Firefox.DeveloperEdition.it  151.0b1  winget\r\n\
7-Zip                      7zip.7zip             < 24.09   winget\r\n";

    #[test]
    fn parses_the_table() {
        let v = parse_table(TABLE);
        assert_eq!(v.len(), 3);
        assert_eq!(
            v[0],
            Pkg {
                name: "Mozilla Firefox (it)".into(),
                id: "Mozilla.Firefox.it".into(),
                version: "150.0".into()
            }
        );
        assert_eq!(v[2].version, "24.09");
        assert!(parse_table("Nessun pacchetto trovato.\n").is_empty());
        assert!(parse_table("").is_empty());
    }

    #[test]
    fn latest_uses_exact_id() {
        let r = FakeRunner::default();
        r.responses
            .lock()
            .unwrap()
            .push(("--exact".into(), 0, TABLE.into()));
        assert_eq!(
            latest(&r, "Mozilla.Firefox.it").unwrap().as_deref(),
            Some("150.0")
        );
        assert_eq!(latest(&r, "7zip.7zip").unwrap().as_deref(), Some("24.09"));
        assert_eq!(latest(&r, "Non.Esiste").unwrap(), None);
        assert!(r.calls.lock().unwrap()[0].contains("--id Mozilla.Firefox.it --exact"));
    }

    #[test]
    fn search_and_availability() {
        let r = FakeRunner::default();
        r.responses
            .lock()
            .unwrap()
            .push(("search".into(), 0, TABLE.into()));
        assert_eq!(search(&r, "firefox").unwrap().len(), 3);
        assert!(!available(&r));
        r.responses
            .lock()
            .unwrap()
            .push(("--version".into(), 0, "v1.9".into()));
        assert!(available(&r));
    }

    #[test]
    fn manifest_kinds_and_portable_rule() {
        assert_eq!(
            manifest_kind("InstallerType: zip\nNestedInstallerType: portable\n").as_deref(),
            Some("portable")
        );
        assert_eq!(
            manifest_kind("Installers:\n- Architecture: x64\n  InstallerType: nullsoft\n")
                .as_deref(),
            Some("nullsoft")
        );
        assert_eq!(manifest_kind("a: b"), None);
        assert!(is_portable_package("zip", Some("portable")));
        assert!(is_portable_package("exe", Some("portable")));
        assert!(is_portable_package("zip", None));
        assert!(!is_portable_package("exe", None));
        assert!(!is_portable_package("exe", Some("nullsoft")));
        assert!(!is_portable_package("zip", Some("inno")));
        assert!(!is_portable_package("msi", Some("portable")));
    }

    #[test]
    fn download_picks_the_installer_file() {
        let d = std::env::temp_dir().join(format!("ld_wg_{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        let r = FakeRunner::default();
        let dd = d.clone();
        r.hooks.lock().unwrap().push((
            "download".into(),
            Box::new(move |_| {
                let _ = fs::create_dir_all(&dd);
                let _ = fs::write(dd.join("Mozilla.Firefox_150.0.exe"), vec![1u8; 500]);
                let _ = fs::write(dd.join("Mozilla.Firefox.yaml"), b"x");
            }),
        ));
        let p = download(&r, "Mozilla.Firefox.it", &d).unwrap();
        assert!(p.ends_with("Mozilla.Firefox_150.0.exe"));
        assert!(r.calls.lock().unwrap()[0].contains("--download-directory"));
        // Senza file scaricati è un errore.
        let r2 = FakeRunner::default();
        let d2 = d.join("vuota");
        assert!(download(&r2, "X.Y", &d2)
            .unwrap_err()
            .contains("nessun installer"));
        let _ = fs::remove_dir_all(&d);
    }
}
