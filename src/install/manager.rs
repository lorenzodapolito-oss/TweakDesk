//! Controllo aggiornamenti, download verificato e creazione degli script.

use crate::install::detect;
use crate::install::scan::Item;
use crate::install::scripts;
use crate::install::version;
use crate::install::winget::Winget;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    UpToDate,
    Outdated { latest: String },
    Unknown(String),
}

pub fn check(w: &dyn Winget, item: &Item) -> Status {
    let Some(id) = &item.winget_id else {
        return Status::Unknown("non nel catalogo".into());
    };
    match w.latest_version(id) {
        Err(e) => Status::Unknown(e),
        Ok(latest) => match &item.version {
            None => Status::Outdated { latest },
            Some(local) if version::is_newer(&latest, local) => Status::Outdated { latest },
            Some(_) => Status::UpToDate,
        },
    }
}

fn installers_in(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
                .map(|e| e.path())
                .filter(|p| {
                    let n = p.to_string_lossy().to_lowercase();
                    [
                        ".exe",
                        ".msi",
                        ".msix",
                        ".appx",
                        ".msixbundle",
                        ".appxbundle",
                    ]
                    .iter()
                    .any(|x| n.ends_with(x))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// True se `<radice>\\<programma>\\` contiene già un installer di versione uguale o più recente di
/// `target` (versione ricavata dal nome file): in quel caso non serve scaricare di nuovo.
pub fn is_current(root: &Path, program: &str, target: &str) -> bool {
    let folder = root.join(scripts::safe_name(program));
    installers_in(&folder).iter().any(|p| {
        p.file_name()
            .and_then(|n| version::from_filename(&n.to_string_lossy()))
            .is_some_and(|have| have == target || version::is_newer(&have, target))
    })
}

/// Gli installer che stanno nelle cartelle dei programmi indicati (nome del programma di
/// winget): la cartella di ognuno è `safe_name(nome)`.
pub fn items_for_names(items: &[Item], names: &[String]) -> Vec<Item> {
    let prefixes: Vec<String> = names
        .iter()
        .map(|n| format!("{}\\", scripts::safe_name(n)).to_lowercase())
        .collect();
    items
        .iter()
        .filter(|i| {
            let rel = i.rel_path.to_lowercase();
            prefixes.iter().any(|p| rel.starts_with(p))
        })
        .cloned()
        .collect()
}

/// Scrive lo script che installa in sequenza `items` (con una sola richiesta di
/// amministratore) e ne restituisce il percorso.
pub fn write_run_script(root: &Path, items: &[Item]) -> Result<PathBuf, String> {
    let jobs: Vec<scripts::Job<'_>> = items
        .iter()
        .map(|i| scripts::Job {
            display: &i.program,
            rel_path: &i.rel_path,
            kind: i.kind,
            args: &i.args,
        })
        .collect();
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let path = root.join("Installa-selezionati.cmd");
    scripts::write(&path, &scripts::install_all(&jobs))?;
    Ok(path)
}

/// Scarica la nuova versione in `<radice>\<Programma>\`.
/// Il download passa da una cartella temporanea: se il file non sembra un installer
/// viene scartato. Le versioni precedenti vengono spostate in `Vecchie\`, mai cancellate.
pub fn download_update(
    w: &dyn Winget,
    root: &Path,
    program: &str,
    winget_id: &str,
) -> Result<PathBuf, String> {
    let folder = root.join(scripts::safe_name(program));
    let tmp = folder.join(".nuovo");
    let _ = std::fs::remove_dir_all(&tmp);
    w.download(winget_id, &tmp)?;
    let fresh: Vec<PathBuf> = installers_in(&tmp);
    let Some(file) = fresh
        .iter()
        .find(|p| detect::looks_like_installer(p))
        .cloned()
    else {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err("il file scaricato non è un installer valido".into());
    };
    let name = file.file_name().ok_or("nome file mancante")?.to_os_string();
    let dest = folder.join(&name);
    if dest.exists() {
        let _ = std::fs::remove_dir_all(&tmp);
        return Ok(dest);
    }
    let old_dir = folder.join("Vecchie");
    for old in installers_in(&folder) {
        std::fs::create_dir_all(&old_dir).map_err(|e| e.to_string())?;
        if let Some(n) = old.file_name() {
            std::fs::rename(&old, old_dir.join(n))
                .map_err(|e| format!("{}: {e}", old.display()))?;
        }
    }
    std::fs::rename(&file, &dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(dest)
}

/// Crea un `.cmd` accanto a ogni installer e `Installa-tutto.cmd` nella radice.
/// Restituisce il numero di script singoli creati.
pub fn write_scripts(root: &Path, items: &[Item]) -> Result<usize, String> {
    let mut n = 0;
    for it in items {
        let stem = scripts::safe_name(&format!("Installa {}", it.program));
        let dir = it.path.parent().unwrap_or(root);
        let content = scripts::single(&it.program, &it.file_name, it.kind, &it.args);
        scripts::write(&dir.join(format!("{stem}.cmd")), &content)?;
        n += 1;
    }
    let jobs: Vec<scripts::Job<'_>> = items
        .iter()
        .map(|i| scripts::Job {
            display: &i.program,
            rel_path: &i.rel_path,
            kind: i.kind,
            args: &i.args,
        })
        .collect();
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    scripts::write(
        &root.join("Installa-tutto.cmd"),
        &scripts::install_all(&jobs),
    )?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::config::AppConfig;
    use crate::install::detect::Kind;
    use crate::install::scan;
    use crate::install::winget::fake::FakeWinget;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("idesk_mgr_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn item(version: Option<&str>, id: Option<&str>) -> Item {
        Item {
            program: "Git".into(),
            winget_id: id.map(str::to_string),
            path: PathBuf::from("Git/Git-2.0.exe"),
            file_name: "Git-2.0.exe".into(),
            rel_path: "Git\\Git-2.0.exe".into(),
            version: version.map(str::to_string),
            kind: Kind::Inno,
            args: "/VERYSILENT".into(),
        }
    }

    fn fake(version: &str) -> FakeWinget {
        let mut f = FakeWinget {
            file_name: "Git-2.47.1-64-bit.exe".into(),
            ..Default::default()
        };
        f.versions.insert("Git.Git".into(), version.into());
        f
    }

    #[test]
    fn status_cases() {
        let w = fake("2.47.1");
        assert_eq!(
            check(&w, &item(Some("2.40.0"), Some("Git.Git"))),
            Status::Outdated {
                latest: "2.47.1".into()
            }
        );
        assert_eq!(
            check(&w, &item(Some("2.47.1"), Some("Git.Git"))),
            Status::UpToDate
        );
        assert_eq!(
            check(&w, &item(None, Some("Git.Git"))),
            Status::Outdated {
                latest: "2.47.1".into()
            }
        );
        assert!(matches!(
            check(&w, &item(Some("1"), None)),
            Status::Unknown(_)
        ));
        assert!(matches!(
            check(&w, &item(Some("1"), Some("Altro.Id"))),
            Status::Unknown(_)
        ));
    }

    #[test]
    fn download_moves_old_versions_to_backup_folder() {
        let root = dir("dl");
        std::fs::create_dir_all(root.join("Git")).unwrap();
        std::fs::write(root.join("Git").join("Git-2.40.0.exe"), b"MZ old").unwrap();
        let path = download_update(&fake("2.47.1"), &root, "Git", "Git.Git").unwrap();
        assert_eq!(path, root.join("Git").join("Git-2.47.1-64-bit.exe"));
        assert!(path.exists());
        assert!(root
            .join("Git")
            .join("Vecchie")
            .join("Git-2.40.0.exe")
            .exists());
        assert!(!root.join("Git").join("Git-2.40.0.exe").exists());
        assert!(!root.join("Git").join(".nuovo").exists());
    }

    #[test]
    fn download_is_idempotent() {
        let root = dir("idem");
        let w = fake("2.47.1");
        let a = download_update(&w, &root, "Git", "Git.Git").unwrap();
        let b = download_update(&w, &root, "Git", "Git.Git").unwrap();
        assert_eq!(a, b);
        assert!(!root.join("Git").join("Vecchie").exists());
    }

    #[test]
    fn invalid_download_is_rejected_and_old_file_kept() {
        let root = dir("bad");
        std::fs::create_dir_all(root.join("Git")).unwrap();
        std::fs::write(root.join("Git").join("Git-2.40.0.exe"), b"MZ old").unwrap();
        struct Html;
        impl Winget for Html {
            fn list_installed(&self) -> Result<Vec<crate::install::winget::Installed>, String> {
                Ok(Vec::new())
            }
            fn latest_version(&self, _: &str) -> Result<String, String> {
                Ok("9".into())
            }
            fn download(&self, _: &str, d: &Path) -> Result<(), String> {
                std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
                std::fs::write(d.join("x.exe"), b"<html>errore</html>").map_err(|e| e.to_string())
            }
        }
        assert!(download_update(&Html, &root, "Git", "Git.Git").is_err());
        assert!(root.join("Git").join("Git-2.40.0.exe").exists());
        assert!(!root.join("Git").join(".nuovo").exists());
    }

    #[test]
    fn download_error_is_reported() {
        let root = dir("err");
        let mut w = fake("2.47.1");
        w.fail_download = true;
        assert!(download_update(&w, &root, "Git", "Git.Git")
            .unwrap_err()
            .contains("rete"));
    }

    #[test]
    fn is_current_compares_versions_in_file_names() {
        let root = dir("cur");
        std::fs::create_dir_all(root.join("Git")).unwrap();
        std::fs::write(root.join("Git").join("Git-2.47.1-64-bit.exe"), b"MZ").unwrap();
        assert!(is_current(&root, "Git", "2.47.1"));
        assert!(is_current(&root, "Git", "2.40.0"));
        assert!(!is_current(&root, "Git", "2.50.0"));
        assert!(!is_current(&root, "Altro", "1.0"));
    }

    #[test]
    fn items_are_matched_by_program_folder_and_run_script_is_written() {
        let root = dir("run");
        for (f, n) in [
            ("Git", "Git-2.47.1.exe"),
            ("VLC media player", "vlc-3.0.21.exe"),
        ] {
            std::fs::create_dir_all(root.join(f)).unwrap();
            std::fs::write(root.join(f).join(n), b"MZ").unwrap();
        }
        let items = scan::scan(&root, &AppConfig::default());
        assert_eq!(items.len(), 2);
        let sel = items_for_names(&items, &["VLC media player".into()]);
        assert_eq!(sel.len(), 1);
        assert!(items_for_names(&items, &[]).is_empty());
        let p = write_run_script(&root, &sel).unwrap();
        let text = std::fs::read_to_string(p).unwrap();
        assert!(text.contains("vlc-3.0.21.exe") && !text.contains("Git-2.47.1"));
    }

    #[test]
    fn scripts_are_written_next_to_installers_and_in_root() {
        let root = dir("scr");
        std::fs::create_dir_all(root.join("Git")).unwrap();
        std::fs::write(root.join("Git").join("Git-2.47.1.exe"), b"MZ Inno Setup").unwrap();
        let items = scan::scan(&root, &AppConfig::default());
        assert_eq!(write_scripts(&root, &items).unwrap(), 1);
        let single = std::fs::read_to_string(root.join("Git").join("Installa Git.cmd")).unwrap();
        assert!(single.contains("/VERYSILENT"));
        let all = std::fs::read_to_string(root.join("Installa-tutto.cmd")).unwrap();
        assert!(all.contains("Git\\Git-2.47.1.exe"));
    }
}
