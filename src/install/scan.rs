//! Lettura della cartella degli installer: un'eventuale sottocartella per programma.

use crate::install::catalog;
use crate::install::config::AppConfig;
use crate::install::detect::{self, Kind};
use crate::install::version;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Item {
    pub program: String,
    pub winget_id: Option<String>,
    pub path: PathBuf,
    pub file_name: String,
    /// Percorso relativo alla cartella principale, con `\` (per gli script).
    pub rel_path: String,
    pub version: Option<String>,
    pub kind: Kind,
    pub args: String,
}

fn is_installer_name(name: &str) -> bool {
    let l = name.to_lowercase();
    [
        ".exe",
        ".msi",
        ".msix",
        ".appx",
        ".msixbundle",
        ".appxbundle",
    ]
    .iter()
    .any(|e| l.ends_with(e))
}

/// Opzioni effettive: personalizzate > catalogo > tipo di installer.
pub fn effective_args(
    program: &str,
    entry_args: Option<&str>,
    kind: Kind,
    cfg: &AppConfig,
) -> String {
    if let Some(o) = cfg.silent_overrides.get(program) {
        if !o.trim().is_empty() {
            return o.trim().to_string();
        }
    }
    if kind == Kind::Msi {
        return kind.default_args().into();
    }
    entry_args
        .unwrap_or_else(|| kind.default_args())
        .to_string()
}

fn make_item(root: &Path, path: &Path, folder_hint: Option<&str>, cfg: &AppConfig) -> Item {
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let entry = catalog::find(&file_name).or_else(|| folder_hint.and_then(catalog::find));
    let program = entry
        .map(|e| e.name.to_string())
        .or_else(|| folder_hint.map(str::to_string))
        .unwrap_or_else(|| {
            file_name
                .rsplit_once('.')
                .map_or(file_name.clone(), |(s, _)| s.to_string())
        });
    let kind = detect::detect(path);
    let rel = path
        .strip_prefix(root)
        .map(|p| p.to_string_lossy().replace('/', "\\"))
        .unwrap_or_else(|_| file_name.clone());
    Item {
        args: effective_args(&program, entry.and_then(|e| e.silent_args), kind, cfg),
        winget_id: entry.map(|e| e.winget_id.to_string()),
        version: version::from_filename(&file_name),
        program,
        path: path.to_path_buf(),
        file_name,
        rel_path: rel,
        kind,
    }
}

/// Elenco degli installer: per ogni programma tiene solo la versione più recente
/// (le vecchie in `Vecchie\` o in altre sottocartelle annidate vengono ignorate).
pub fn scan(root: &Path, cfg: &AppConfig) -> Vec<Item> {
    let mut found: Vec<Item> = Vec::new();
    let Ok(rd) = std::fs::read_dir(root) else {
        return found;
    };
    let mut entries: Vec<_> = rd.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_file() {
            if is_installer_name(&e.file_name().to_string_lossy()) {
                found.push(make_item(root, &p, None, cfg));
            }
        } else if ft.is_dir() {
            let folder = e.file_name().to_string_lossy().to_string();
            let Ok(inner) = std::fs::read_dir(&p) else {
                continue;
            };
            for f in inner.filter_map(Result::ok) {
                if f.file_type().map(|t| t.is_file()).unwrap_or(false)
                    && is_installer_name(&f.file_name().to_string_lossy())
                {
                    found.push(make_item(root, &f.path(), Some(&folder), cfg));
                }
            }
        }
    }
    keep_latest(found)
}

fn keep_latest(items: Vec<Item>) -> Vec<Item> {
    let mut out: Vec<Item> = Vec::new();
    for it in items {
        match out.iter_mut().find(|o| o.program == it.program) {
            Some(o) => {
                let newer = match (&it.version, &o.version) {
                    (Some(a), Some(b)) => version::is_newer(a, b),
                    (Some(_), None) => true,
                    _ => false,
                };
                if newer {
                    *o = it;
                }
            }
            None => out.push(it),
        }
    }
    out.sort_by_key(|i| i.program.to_lowercase());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(name: &str, files: &[(&str, &[u8])]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("idesk_scan_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (rel, data) in files {
            let p = root.join(rel);
            let _ = std::fs::create_dir_all(p.parent().unwrap_or(&root));
            std::fs::write(p, data).unwrap();
        }
        root
    }

    #[test]
    fn reads_files_and_program_folders() {
        let root = tree(
            "a",
            &[
                ("vlc-3.0.20-win64.exe", b"MZ"),
                ("Git/Git-2.47.1-64-bit.exe", b"MZ Inno Setup"),
                ("Altro/Strano.exe", b"MZ"),
                ("nota.txt", b"x"),
            ],
        );
        let items = scan(&root, &AppConfig::default());
        let names: Vec<&str> = items.iter().map(|i| i.program.as_str()).collect();
        assert_eq!(names, ["Altro", "Git", "VLC media player"]);
        let git = items.iter().find(|i| i.program == "Git").unwrap();
        assert_eq!(git.kind, Kind::Inno);
        assert_eq!(git.version.as_deref(), Some("2.47.1"));
        assert_eq!(git.winget_id.as_deref(), Some("Git.Git"));
        assert_eq!(git.rel_path, "Git\\Git-2.47.1-64-bit.exe");
        assert!(items[0].winget_id.is_none());
    }

    #[test]
    fn keeps_only_the_newest_version_per_program() {
        let root = tree(
            "b",
            &[
                ("Git/Git-2.9.0.exe", b"MZ"),
                ("Git/Git-2.47.1.exe", b"MZ"),
                ("Git/Vecchie/Git-1.0.0.exe", b"MZ"),
            ],
        );
        let items = scan(&root, &AppConfig::default());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].version.as_deref(), Some("2.47.1"));
    }

    #[test]
    fn args_priority_override_catalog_kind() {
        let mut cfg = AppConfig::default();
        assert_eq!(effective_args("X", None, Kind::Nsis, &cfg), "/S");
        assert_eq!(effective_args("X", Some("-s"), Kind::Nsis, &cfg), "-s");
        assert_eq!(
            effective_args("X", Some("-s"), Kind::Msi, &cfg),
            "/qn /norestart"
        );
        cfg.silent_overrides.insert("X".into(), " /mio ".into());
        assert_eq!(effective_args("X", Some("-s"), Kind::Nsis, &cfg), "/mio");
        cfg.silent_overrides.insert("X".into(), "  ".into());
        assert_eq!(
            effective_args("X", None, Kind::Inno, &cfg),
            Kind::Inno.default_args()
        );
    }

    #[test]
    fn missing_folder_gives_empty_list() {
        assert!(scan(Path::new("/non/esiste/mai"), &AppConfig::default()).is_empty());
    }
}
