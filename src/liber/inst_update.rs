//! Controllo e aggiornamento dei programmi non portabili tramite winget.

use crate::liber::installers::{self, Entry, Installer, Store};
use crate::liber::version;
use crate::liber::winget::{self, Runner};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub enum IStatus {
    Unchecked,
    Checking,
    Current(String),
    Newer(String),
    Downloading,
    Done(String),
    /// Nessun pacchetto winget associato: si può cercare a mano.
    NoId,
    Error(String),
}

/// Versione dell'installer: quella salvata, altrimenti quella nel nome del file.
pub fn local_version(inst: &Installer, e: &Entry) -> Option<String> {
    e.version
        .clone()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| version::from_folder_name(&inst.name))
        .or_else(|| version::extract(&inst.name))
        // Un numero solo («7z2409» -> 7) non è una versione affidabile.
        .filter(|v| version::segments(v).len() >= 2)
}

pub fn check(r: &dyn Runner, inst: &Installer, e: &Entry) -> IStatus {
    let Some(id) = e.winget_id.as_deref().filter(|i| !i.trim().is_empty()) else {
        return IStatus::NoId;
    };
    match winget::latest(r, id) {
        Err(m) => IStatus::Error(m),
        Ok(None) => IStatus::Error(format!("winget non trova «{id}»")),
        Ok(Some(latest)) => match local_version(inst, e) {
            Some(l) if !version::is_newer(&latest, &l) => IStatus::Current(l),
            // Versione locale sconosciuta: si propone comunque l'ultima.
            _ => IStatus::Newer(latest),
        },
    }
}

fn unique(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    if !p.exists() {
        return p;
    }
    for n in 1..1000 {
        let q = dir.join(format!("{n}_{name}"));
        if !q.exists() {
            return q;
        }
    }
    dir.join(format!("{}_{name}", std::process::id()))
}

fn move_file(from: &Path, to: &Path) -> Result<(), String> {
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    fs::copy(from, to).map_err(|e| format!("{}: {e}", to.display()))?;
    fs::remove_file(from).map_err(|e| format!("{}: {e}", from.display()))
}

/// Scarica l'ultima versione, sposta il vecchio installer in `_vecchi`, mette il nuovo al suo posto,
/// rigenera lo script e aggiorna l'archivio. Restituisce il nuovo file.
pub fn update(
    r: &dyn Runner,
    root: &Path,
    inst: &Installer,
    store: &mut Store,
    latest: &str,
) -> Result<PathBuf, String> {
    let entry = store.items.get(&inst.rel).cloned().unwrap_or_default();
    let id = entry
        .winget_id
        .clone()
        .filter(|i| !i.trim().is_empty())
        .ok_or("Nessun pacchetto winget associato")?;
    let tmp = root.join("_temp").join(installers::safe_name(&id));
    let _ = fs::remove_dir_all(&tmp);
    let got = winget::download(r, &id, &tmp)?;
    let dir = inst.file.parent().ok_or("Percorso non valido")?;
    let old_dir = root.join("_vecchi");
    fs::create_dir_all(&old_dir).map_err(|e| format!("{}: {e}", old_dir.display()))?;
    let fname = got
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or("File scaricato senza nome")?;
    let dest = if dir.join(&fname).exists() && dir.join(&fname) != inst.file {
        unique(dir, &fname)
    } else {
        dir.join(&fname)
    };
    if inst.file.exists() {
        move_file(
            &inst.file,
            &unique(&old_dir, &installers::base_name_of(&inst.file)),
        )?;
    }
    let old_script = dir.join(installers::script_name(inst));
    let _ = fs::remove_file(old_script);
    move_file(&got, &dest)?;
    let _ = fs::remove_dir_all(&tmp);
    let rel = dest
        .strip_prefix(root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| fname.clone());
    let new = Installer {
        name: dest
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or(fname),
        kind: installers::detect(&dest),
        file: dest.clone(),
        rel: rel.clone(),
        category: inst.category.clone(),
    };
    store.items.remove(&inst.rel);
    let ne = Entry {
        version: Some(latest.to_string()),
        ..entry
    };
    let custom = ne.switches.clone();
    store.items.insert(rel, ne);
    installers::write_script(&new, custom.as_deref())?;
    installers::save_store(root, store)?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::liber::winget::fake::FakeRunner;

    const TABLE: &str = "Nome  ID  Versione  Origine\r\n----------------------------------\r\nFirefox  Mozilla.Firefox  150.0  winget\r\n";

    fn inst(name: &str) -> Installer {
        Installer {
            name: name.into(),
            file: PathBuf::from(format!("/x/{name}.exe")),
            rel: format!("{name}.exe"),
            category: None,
            kind: installers::InstKind::Nsis,
        }
    }

    fn entry(id: Option<&str>, v: Option<&str>) -> Entry {
        Entry {
            winget_id: id.map(Into::into),
            version: v.map(Into::into),
            switches: None,
        }
    }

    #[test]
    fn local_version_sources() {
        assert_eq!(
            local_version(&inst("x"), &entry(None, Some("1.2"))).as_deref(),
            Some("1.2")
        );
        assert_eq!(
            local_version(&inst("Firefox Setup 149.0"), &entry(None, None)).as_deref(),
            Some("149.0")
        );
        assert_eq!(local_version(&inst("Firefox"), &entry(None, None)), None);
        assert_eq!(local_version(&inst("7z2409"), &entry(None, None)), None);
    }

    #[test]
    fn check_compares_versions() {
        let r = FakeRunner::default();
        r.responses
            .lock()
            .unwrap()
            .push(("--exact".into(), 0, TABLE.into()));
        let i = inst("Firefox Setup 149.0");
        assert_eq!(check(&r, &i, &entry(None, None)), IStatus::NoId);
        assert_eq!(
            check(&r, &i, &entry(Some("Mozilla.Firefox"), None)),
            IStatus::Newer("150.0".into())
        );
        assert_eq!(
            check(&r, &i, &entry(Some("Mozilla.Firefox"), Some("150.0"))),
            IStatus::Current("150.0".into())
        );
        assert!(matches!(
            check(&r, &i, &entry(Some("Altro.Id"), None)),
            IStatus::Error(_)
        ));
        // versione sconosciuta: si propone l'ultima
        assert_eq!(
            check(&r, &inst("Firefox"), &entry(Some("Mozilla.Firefox"), None)),
            IStatus::Newer("150.0".into())
        );
    }

    #[test]
    fn update_swaps_installer_and_rewrites_script() {
        let root = std::env::temp_dir().join(format!("ld_iu_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("Internet")).unwrap();
        let old = root.join("Internet/Firefox Setup 149.0.exe");
        fs::write(&old, b"MZ Nullsoft Install System").unwrap();
        fs::write(
            root.join("Internet/Installa_Firefox_Setup_149.0.cmd"),
            b"vecchio",
        )
        .unwrap();
        let i = installers::scan(&root).remove(0);
        let mut st = Store::default();
        st.items
            .insert(i.rel.clone(), entry(Some("Mozilla.Firefox"), Some("149.0")));
        let r = FakeRunner::default();
        r.hooks.lock().unwrap().push((
            "download".into(),
            Box::new(|args: &[&str]| {
                if let Some(p) = args.iter().position(|a| *a == "--download-directory") {
                    let d = PathBuf::from(args[p + 1]);
                    let _ = fs::create_dir_all(&d);
                    let _ = fs::write(
                        d.join("Mozilla.Firefox_150.0.exe"),
                        b"MZ Nullsoft Install System nuovo",
                    );
                }
            }),
        ));
        let n = update(&r, &root, &i, &mut st, "150.0").unwrap();
        assert_eq!(n, root.join("Internet/Mozilla.Firefox_150.0.exe"));
        assert!(n.exists());
        assert!(!old.exists());
        assert!(root.join("_vecchi/Firefox Setup 149.0.exe").exists());
        assert!(!root
            .join("Internet/Installa_Firefox_Setup_149.0.cmd")
            .exists());
        assert!(
            fs::read_to_string(root.join("Internet/Installa_Mozilla.Firefox_150.0.cmd"))
                .unwrap()
                .contains(" /S")
        );
        assert!(!st.items.contains_key("Internet/Firefox Setup 149.0.exe"));
        let e = st.items.get("Internet/Mozilla.Firefox_150.0.exe").unwrap();
        assert_eq!(e.version.as_deref(), Some("150.0"));
        assert_eq!(e.winget_id.as_deref(), Some("Mozilla.Firefox"));
        assert_eq!(installers::load_store(&root), st);
        // La cartella dei vecchi non viene riletta come installer.
        assert_eq!(installers::scan(&root).len(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn update_without_id_fails() {
        let root = std::env::temp_dir().join(format!("ld_iu2_{}", std::process::id()));
        let r = FakeRunner::default();
        let mut st = Store::default();
        assert!(update(&r, &root, &inst("a"), &mut st, "1").is_err());
    }
}
