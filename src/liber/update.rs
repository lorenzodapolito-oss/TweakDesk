//! Controllo e installazione degli aggiornamenti, con copia di sicurezza e ripristino.

use crate::liber::archive;
use crate::liber::catalog::{Entry, Source};
use crate::liber::github::{self, Asset};
use crate::liber::net::Http;
use crate::liber::scan::App;
use crate::liber::state::{self, State};
use crate::liber::version;
use crate::liber::winget::{self, Runner};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Info,
    Ok,
    Warn,
    Err,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Check {
    /// L'app non è nel catalogo: si avvia ma non si aggiorna.
    NoSource,
    Current(String),
    Available {
        latest: String,
        asset: Asset,
    },
    /// Non si conosce la versione installata: l'aggiornamento è solo manuale.
    Unknown {
        latest: String,
        asset: Asset,
    },
    /// Versione più nuova trovata con winget; `known` = la versione installata è nota.
    Winget {
        latest: String,
        known: bool,
    },
    Error(String),
}

/// Legge la versione di un eseguibile.
pub trait Probe: Send + Sync {
    fn file_version(&self, exe: &Path) -> Option<String>;
}

pub struct RealProbe;

impl Probe for RealProbe {
    #[cfg(windows)]
    fn file_version(&self, exe: &Path) -> Option<String> {
        use std::os::windows::process::CommandExt;
        let quoted = format!("'{}'", exe.to_string_lossy().replace('\'', "''"));
        let out = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                &format!("(Get-Item -LiteralPath {quoted}).VersionInfo.ProductVersion"),
            ])
            .creation_flags(0x0800_0000)
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        version::extract(text.lines().next().unwrap_or(""))
    }

    #[cfg(not(windows))]
    fn file_version(&self, _exe: &Path) -> Option<String> {
        None
    }
}

/// Versione installata: quella salvata da LiberDesk, poi quella scritta nel nome della cartella
/// (solo per le app del catalogo), poi quella dell'eseguibile.
pub fn local_version(app: &App, st: &State, probe: &dyn Probe) -> Option<String> {
    st.version
        .clone()
        .filter(|v| !version::segments(v).is_empty())
        .or_else(|| {
            if app.entry.is_none() && st.winget_id.is_none() {
                return None;
            }
            let name = app.dir.file_name()?.to_string_lossy().to_string();
            version::from_folder_name(&name)
        })
        .or_else(|| {
            // Leggere la versione dall'eseguibile è lento: solo per le app che si aggiornano.
            if app.entry.is_none() && st.winget_id.is_none() {
                return None;
            }
            app.exe.as_ref().and_then(|e| probe.file_version(e))
        })
}

/// Non legge nulla: per aprire l'elenco subito, senza lanciare programmi esterni.
pub struct NoProbe;

impl Probe for NoProbe {
    fn file_version(&self, _exe: &Path) -> Option<String> {
        None
    }
}

pub fn check(http: &dyn Http, app: &App, local: Option<&str>) -> Check {
    let Some(entry) = app.entry else {
        return Check::NoSource;
    };
    let Source::GitHub { repo, asset } = &entry.source;
    let rel = match github::latest(http, repo) {
        Ok(r) => r,
        Err(e) => return Check::Error(e),
    };
    let Some(latest) = rel.version() else {
        return Check::Error(format!("Versione non riconoscibile: {}", rel.tag_name));
    };
    let found = match github::pick_asset(&rel, asset) {
        Ok(a) => a.clone(),
        Err(e) => return Check::Error(e),
    };
    match local {
        Some(l) if version::is_newer(&latest, l) => Check::Available {
            latest,
            asset: found,
        },
        Some(_) => Check::Current(latest),
        None => Check::Unknown {
            latest,
            asset: found,
        },
    }
}

/// Cartelle e file che l'aggiornamento non tocca mai.
const DEFAULT_KEEP: [&str; 8] = [
    "data",
    "settings",
    "config",
    "profile",
    "profiles",
    "portable",
    "portabledata",
    state::FILE,
];

fn first_component(rel: &Path) -> String {
    rel.components()
        .next()
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

pub fn is_kept(rel: &Path, entry: Option<&Entry>) -> bool {
    let first = first_component(rel);
    DEFAULT_KEEP.contains(&first.as_str())
        || entry.is_some_and(|e| e.keep.iter().any(|k| k.to_lowercase() == first))
}

fn timestamp() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()
}

fn safe_name(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Svuota la cartella dell'app tranne ciò che va tenuto.
fn clear_except_kept(dir: &Path, entry: Option<&Entry>) -> Result<(), String> {
    let rd = fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for e in rd.flatten() {
        let p = e.path();
        let rel = p.strip_prefix(dir).map_err(|e| e.to_string())?;
        if is_kept(rel, entry) {
            continue;
        }
        let ft = e.file_type().map_err(|e| e.to_string())?;
        if ft.is_dir() && !ft.is_symlink() {
            fs::remove_dir_all(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        } else {
            fs::remove_file(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        }
    }
    Ok(())
}

/// Rimette la cartella dell'app com'era nella copia di sicurezza.
pub fn restore_backup(dir: &Path, backup: &Path) -> Result<(), String> {
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            let _ = if e.file_type().is_ok_and(|t| t.is_dir()) {
                fs::remove_dir_all(&p)
            } else {
                fs::remove_file(&p)
            };
        }
    }
    archive::copy_tree(backup, dir, &|_| false).map(|_| ())
}

/// Tiene solo le ultime `keep` copie di sicurezza di una cartella.
fn prune_backups(root: &Path, prefix: &str, keep: usize) {
    let Ok(rd) = fs::read_dir(root) else {
        return;
    };
    let mut v: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with(prefix))
        })
        .collect();
    v.sort();
    while v.len() > keep {
        let old = v.remove(0);
        let _ = fs::remove_dir_all(old);
    }
}

fn extension(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, e)| e.to_lowercase())
        .unwrap_or_default()
}

/// Controlla con winget l'ultima versione di un'app portabile.
pub fn winget_check(r: &dyn Runner, id: &str, local: Option<&str>) -> Check {
    match winget::latest(r, id) {
        Err(e) => Check::Error(e),
        Ok(None) => Check::Error(format!("winget non trova «{id}»")),
        Ok(Some(latest)) => match local {
            Some(l) if !version::is_newer(&latest, l) => Check::Current(l.to_string()),
            Some(_) => Check::Winget {
                latest,
                known: true,
            },
            None => Check::Winget {
                latest,
                known: false,
            },
        },
    }
}

/// Scarica con winget e sostituisce l'app, ma solo se il pacchetto è portabile (zip o exe «portable»).
pub fn winget_apply(
    r: &dyn Runner,
    app: &App,
    local: Option<&str>,
    latest: &str,
    id: &str,
    log: &mut dyn FnMut(Kind, &str),
) -> Result<(), String> {
    let folder = app
        .dir
        .strip_prefix(&app.root)
        .map_err(|_| "Cartella dell'app non valida".to_string())?
        .to_string_lossy()
        .to_string();
    let tmp = app.root.join("_temp").join(safe_name(&folder));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
    let result = (|| -> Result<(), String> {
        log(
            Kind::Info,
            &format!("{}: scarico con winget ({id})…", app.name),
        );
        let got = winget::download_any(r, id, &tmp.join("winget"))?;
        let name = got
            .file
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let ext = extension(&name);
        if !winget::is_portable_package(&ext, got.kind.as_deref()) {
            return Err(format!(
                "winget offre solo un installer ({}), non una versione portabile: scaricala a mano",
                got.kind.as_deref().unwrap_or(&ext)
            ));
        }
        install_from(app, local, latest, &got.file, &ext, &name, &tmp, log)
    })();
    let _ = fs::remove_dir_all(&tmp);
    result?;
    log(
        Kind::Ok,
        &format!("{}: aggiornata alla versione {latest}", app.name),
    );
    Ok(())
}

/// Sostituisce l'app con il file scaricato (zip, 7z o exe), con copia di sicurezza e ripristino.
#[allow(clippy::too_many_arguments)]
fn install_from(
    app: &App,
    local: Option<&str>,
    latest: &str,
    file: &Path,
    ext: &str,
    asset_name: &str,
    tmp: &Path,
    log: &mut dyn FnMut(Kind, &str),
) -> Result<(), String> {
    let apps_dir = app.root.clone();
    let folder = app
        .dir
        .strip_prefix(&app.root)
        .map_err(|_| "Cartella dell'app non valida".to_string())?
        .to_string_lossy()
        .to_string();
    let new_dir = tmp.join("nuovo");
    let root = if ext == "zip" || ext == "7z" {
        if ext == "zip" {
            archive::extract_zip(file, &new_dir)?;
        } else {
            archive::extract_7z(file, &new_dir)?;
        }
        archive::content_root(&new_dir)
    } else {
        fs::create_dir_all(&new_dir).map_err(|e| e.to_string())?;
        fs::copy(file, new_dir.join(asset_name)).map_err(|e| e.to_string())?;
        new_dir.clone()
    };
    if fs::read_dir(&root).map_or(true, |mut r| r.next().is_none()) {
        return Err("Il pacchetto scaricato è vuoto".into());
    }

    // Copia di sicurezza.
    let backups = apps_dir.join("_backup");
    let prefix = format!("{}_", safe_name(&folder));
    let backup = backups.join(format!(
        "{prefix}{}_{}",
        safe_name(local.unwrap_or("prima")),
        timestamp()
    ));
    archive::copy_tree(&app.dir, &backup, &|_| false)?;
    log(
        Kind::Info,
        &format!("{}: copia di sicurezza fatta", app.name),
    );

    // Sostituzione: tolgo il vecchio (tranne i dati) e copio il nuovo.
    let replaced = clear_except_kept(&app.dir, app.entry).and_then(|()| {
        archive::copy_tree(&root, &app.dir, &|rel| {
            is_kept(rel, app.entry) && app.dir.join(rel).exists()
        })
    });
    if let Err(e) = replaced {
        log(
            Kind::Warn,
            &format!("{}: ripristino la versione precedente…", app.name),
        );
        return match restore_backup(&app.dir, &backup) {
            Ok(()) => Err(format!("{e} (versione precedente ripristinata)")),
            Err(r) => Err(format!("{e}; ripristino non riuscito: {r}")),
        };
    }
    let previous = state::load(&app.dir);
    let st = State {
        version: Some(latest.to_string()),
        checked: chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
        category: previous.category,
        winget_id: previous.winget_id,
    };
    state::save(&app.dir, &st)?;
    prune_backups(&backups, &prefix, 2);
    Ok(())
}

/// Scarica e installa. Se qualcosa va storto dopo aver toccato l'app, la ripristina.
pub fn apply(
    http: &dyn Http,
    app: &App,
    local: Option<&str>,
    latest: &str,
    asset: &Asset,
    log: &mut dyn FnMut(Kind, &str),
) -> Result<(), String> {
    let apps_dir = app.root.clone();
    let folder = app
        .dir
        .strip_prefix(&app.root)
        .map_err(|_| "Cartella dell'app non valida".to_string())?
        .to_string_lossy()
        .to_string();
    let ext = extension(&asset.name);
    if ext != "zip" && ext != "7z" && ext != "exe" {
        return Err(format!(
            "Formato non supportato (.{ext}): serve uno zip, un .7z o un file .exe"
        ));
    }
    let tmp = apps_dir.join("_temp").join(safe_name(&folder));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
    let result = (|| -> Result<(), String> {
        let file = tmp.join(format!("scaricato.{ext}"));
        log(
            Kind::Info,
            &format!("{}: scarico {}…", app.name, asset.name),
        );
        let bytes = http.download(&asset.browser_download_url, &file)?;
        if bytes == 0 {
            return Err("Il file scaricato è vuoto".into());
        }
        if asset.size > 0 && bytes != asset.size {
            return Err(format!(
                "Download incompleto: {bytes} byte invece di {}",
                asset.size
            ));
        }
        install_from(app, local, latest, &file, &ext, &asset.name, &tmp, log)?;
        Ok(())
    })();
    let _ = fs::remove_dir_all(&tmp);
    result?;
    log(
        Kind::Ok,
        &format!("{}: aggiornata alla versione {latest}", app.name),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::liber::archive::tests::make_zip;
    use crate::liber::catalog;
    use crate::liber::net::fake::FakeHttp;

    struct P(Option<&'static str>);
    impl Probe for P {
        fn file_version(&self, _: &Path) -> Option<String> {
            self.0.map(String::from)
        }
    }

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ld_upd_{}_{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn app_in(d: &Path, folder: &str) -> App {
        let dir = d.join(folder);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("notepad++.exe"), b"vecchio").unwrap();
        App {
            name: "Notepad++".into(),
            dir,
            exe: None,
            entry: catalog::find_for_folder(folder),
            root: d.to_path_buf(),
            folder_category: None,
        }
    }

    fn http_with(zip: &Path) -> FakeHttp {
        let h = FakeHttp::default();
        let size = fs::metadata(zip).unwrap().len();
        let json = format!(
            r#"{{"tag_name":"v8.8.0","assets":[{{"name":"npp.8.8.0.portable.x64.zip","browser_download_url":"https://x/p.zip","size":{size}}}]}}"#
        );
        h.texts
            .lock()
            .unwrap()
            .push(("notepad-plus-plus".into(), Ok(json)));
        h.files
            .lock()
            .unwrap()
            .push(("p.zip".into(), fs::read(zip).unwrap()));
        h
    }

    #[test]
    fn local_version_prefers_saved_then_probe() {
        let d = tmp("lv");
        let app = app_in(&d, "Notepad++");
        let app = App {
            exe: Some(app.dir.join("notepad++.exe")),
            ..app
        };
        let st = State {
            version: Some("8.7.1".into()),
            ..State::default()
        };
        assert_eq!(
            local_version(&app, &st, &P(Some("1.0"))).as_deref(),
            Some("8.7.1")
        );
        assert_eq!(
            local_version(&app, &State::default(), &P(Some("1.0"))).as_deref(),
            Some("1.0")
        );
        assert_eq!(local_version(&app, &State::default(), &P(None)), None);
        // La versione nel nome della cartella conta solo per le app del catalogo.
        let kp = App {
            dir: d.join("KeePassXC-2.7.11-Win64"),
            entry: catalog::find_for_folder("KeePassXC"),
            ..app.clone()
        };
        assert_eq!(
            local_version(&kp, &State::default(), &P(Some("9.9"))).as_deref(),
            Some("2.7.11")
        );
        let other = App { entry: None, ..kp };
        // Le app senza aggiornamenti automatici non lanciano mai la lettura dell'eseguibile.
        assert_eq!(
            local_version(&other, &State::default(), &P(Some("9.9"))),
            None
        );
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn check_reports_each_state() {
        let d = tmp("ck");
        let zip = d.join("p.zip");
        make_zip(&zip, &[("notepad++.exe", b"nuovo")]);
        let h = http_with(&zip);
        let app = app_in(&d, "Notepad++");
        assert!(matches!(
            check(&h, &app, Some("8.7.1")),
            Check::Available { .. }
        ));
        assert_eq!(
            check(&h, &app, Some("8.8.0")),
            Check::Current("8.8.0".into())
        );
        assert_eq!(check(&h, &app, Some("9.0")), Check::Current("8.8.0".into()));
        assert!(matches!(check(&h, &app, None), Check::Unknown { .. }));
        let other = App {
            entry: None,
            ..app.clone()
        };
        assert_eq!(check(&h, &other, Some("1")), Check::NoSource);
        let bad = FakeHttp::default();
        assert!(matches!(check(&bad, &app, Some("1")), Check::Error(_)));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn apply_replaces_files_keeps_data_and_makes_backup() {
        let d = tmp("ap");
        let zip = d.join("p.zip");
        make_zip(
            &zip,
            &[
                ("notepad++.exe", b"nuovo"),
                ("novita.dll", b"d"),
                ("config.xml", b"di fabbrica"),
            ],
        );
        let h = http_with(&zip);
        let app = app_in(&d, "Notepad++");
        fs::write(app.dir.join("vecchio.dll"), b"x").unwrap();
        state::save(
            &app.dir,
            &State {
                category: Some("Giochi".into()),
                ..State::default()
            },
        )
        .unwrap();
        fs::create_dir_all(app.dir.join("Data")).unwrap();
        fs::write(app.dir.join("Data/mio.txt"), b"dati miei").unwrap();
        fs::write(app.dir.join("config.xml"), b"mie impostazioni").unwrap();
        let Check::Available { latest, asset } = check(&h, &app, Some("8.7.1")) else {
            panic!("atteso Available");
        };
        let mut log = Vec::new();
        apply(&h, &app, Some("8.7.1"), &latest, &asset, &mut |k, m| {
            log.push((k, m.to_string()))
        })
        .unwrap();
        assert_eq!(fs::read(app.dir.join("notepad++.exe")).unwrap(), b"nuovo");
        assert!(app.dir.join("novita.dll").is_file());
        assert!(!app.dir.join("vecchio.dll").exists());
        assert_eq!(
            fs::read(app.dir.join("Data/mio.txt")).unwrap(),
            b"dati miei"
        );
        assert_eq!(
            fs::read(app.dir.join("config.xml")).unwrap(),
            b"mie impostazioni"
        );
        assert_eq!(state::load(&app.dir).version.as_deref(), Some("8.8.0"));
        assert_eq!(state::load(&app.dir).category.as_deref(), Some("Giochi"));
        let backups: Vec<_> = fs::read_dir(d.join("_backup")).unwrap().flatten().collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(
            fs::read(backups[0].path().join("notepad++.exe")).unwrap(),
            b"vecchio"
        );
        assert!(!d.join("_temp/Notepad__").exists());
        assert!(log.iter().any(|(k, _)| *k == Kind::Ok));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn failed_download_or_bad_zip_leaves_the_app_untouched() {
        let d = tmp("fail");
        let app = app_in(&d, "Notepad++");
        let a = Asset {
            name: "x.zip".into(),
            browser_download_url: "https://x/manca.zip".into(),
            size: 0,
        };
        let h = FakeHttp::default();
        assert!(apply(&h, &app, None, "9", &a, &mut |_, _| {}).is_err());
        assert_eq!(fs::read(app.dir.join("notepad++.exe")).unwrap(), b"vecchio");
        // Zip rotto.
        h.files
            .lock()
            .unwrap()
            .push(("manca.zip".into(), b"non zip".to_vec()));
        assert!(apply(&h, &app, None, "9", &a, &mut |_, _| {}).is_err());
        assert_eq!(fs::read(app.dir.join("notepad++.exe")).unwrap(), b"vecchio");
        assert!(
            !d.join("_backup").exists()
                || fs::read_dir(d.join("_backup")).unwrap().next().is_none()
        );
        // Download incompleto.
        let z = d.join("z.zip");
        make_zip(&z, &[("a.exe", b"1")]);
        h.files
            .lock()
            .unwrap()
            .push(("ok.zip".into(), fs::read(&z).unwrap()));
        let a2 = Asset {
            name: "x.zip".into(),
            browser_download_url: "https://x/ok.zip".into(),
            size: 999_999,
        };
        assert!(apply(&h, &app, None, "9", &a2, &mut |_, _| {})
            .unwrap_err()
            .contains("incompleto"));
        // Formato non supportato.
        let a3 = Asset {
            name: "x.rar".into(),
            browser_download_url: "https://x/ok.zip".into(),
            size: 0,
        };
        assert!(apply(&h, &app, None, "9", &a3, &mut |_, _| {})
            .unwrap_err()
            .contains("non supportato"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn single_exe_assets_are_installed() {
        let d = tmp("exe");
        let dir = d.join("Rufus");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("rufus-4.4.exe"), b"old").unwrap();
        let app = App {
            name: "Rufus".into(),
            dir: dir.clone(),
            exe: None,
            entry: catalog::find_for_folder("Rufus"),
            root: d.clone(),
            folder_category: None,
        };
        let h = FakeHttp::default();
        h.files
            .lock()
            .unwrap()
            .push(("rufus-4.5.exe".into(), b"nuovo exe".to_vec()));
        let a = Asset {
            name: "rufus-4.5.exe".into(),
            browser_download_url: "https://x/rufus-4.5.exe".into(),
            size: 9,
        };
        apply(&h, &app, Some("4.4"), "4.5", &a, &mut |_, _| {}).unwrap();
        assert!(dir.join("rufus-4.5.exe").is_file());
        assert!(!dir.join("rufus-4.4.exe").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn restore_brings_back_the_old_files() {
        let d = tmp("rs");
        let dir = d.join("App");
        let bak = d.join("bak");
        fs::create_dir_all(&dir).unwrap();
        fs::create_dir_all(&bak).unwrap();
        fs::write(bak.join("a.exe"), b"originale").unwrap();
        fs::write(dir.join("rotto.tmp"), b"x").unwrap();
        restore_backup(&dir, &bak).unwrap();
        assert_eq!(fs::read(dir.join("a.exe")).unwrap(), b"originale");
        assert!(!dir.join("rotto.tmp").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn only_the_last_two_backups_are_kept() {
        let d = tmp("pr");
        for i in 1..=4 {
            fs::create_dir_all(d.join(format!("App_1.{i}_20260101-00000{i}"))).unwrap();
        }
        fs::create_dir_all(d.join("Altra_1_20260101-000001")).unwrap();
        prune_backups(&d, "App_", 2);
        let n = fs::read_dir(&d).unwrap().count();
        assert_eq!(n, 3);
        assert!(d.join("Altra_1_20260101-000001").is_dir());
        assert!(d.join("App_1.4_20260101-000004").is_dir());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn keep_rules() {
        let e = catalog::find_for_folder("Notepad++");
        assert!(is_kept(Path::new("Data/x"), e));
        assert!(is_kept(Path::new("DATA"), None));
        assert!(is_kept(Path::new("plugins/a.dll"), e));
        assert!(!is_kept(Path::new("plugins/a.dll"), None));
        assert!(!is_kept(Path::new("app.exe"), e));
        assert!(is_kept(Path::new("liberdesk.json"), None));
    }

    fn wg_runner(
        zip: &Path,
        yaml: &'static str,
        table: &'static str,
    ) -> crate::liber::winget::fake::FakeRunner {
        let r = crate::liber::winget::fake::FakeRunner::default();
        r.responses
            .lock()
            .unwrap()
            .push(("--exact".into(), 0, table.into()));
        let zip = zip.to_path_buf();
        r.hooks.lock().unwrap().push((
            "download".into(),
            Box::new(move |args: &[&str]| {
                if let Some(p) = args.iter().position(|a| *a == "--download-directory") {
                    let d = PathBuf::from(args[p + 1]);
                    let _ = fs::create_dir_all(&d);
                    let _ = fs::copy(&zip, d.join("Pacchetto_2.0.zip"));
                    let _ = fs::write(d.join("Pacchetto.yaml"), yaml);
                }
            }),
        ));
        r
    }

    const WG_TABLE: &str = "Nome  ID  Versione  Origine\r\n--------------------------------\r\nApp  Ven.App  2.0  winget\r\n";

    #[test]
    fn winget_check_compares() {
        let r = crate::liber::winget::fake::FakeRunner::default();
        r.responses
            .lock()
            .unwrap()
            .push(("--exact".into(), 0, WG_TABLE.into()));
        assert_eq!(
            winget_check(&r, "Ven.App", Some("1.0")),
            Check::Winget {
                latest: "2.0".into(),
                known: true
            }
        );
        assert_eq!(
            winget_check(&r, "Ven.App", Some("2.0")),
            Check::Current("2.0".into())
        );
        assert_eq!(
            winget_check(&r, "Ven.App", None),
            Check::Winget {
                latest: "2.0".into(),
                known: false
            }
        );
        assert!(matches!(
            winget_check(&r, "No.Id", Some("1")),
            Check::Error(_)
        ));
    }

    #[test]
    fn winget_apply_replaces_portable_zip_and_keeps_data() {
        let d = tmp("wga");
        let app = app_in(&d, "Ventoy");
        fs::create_dir_all(app.dir.join("Data")).unwrap();
        fs::write(app.dir.join("Data/utente.ini"), b"mio").unwrap();
        let zip = d.join("p.zip");
        make_zip(&zip, &[("nuovo.exe", b"nuovo"), ("lib.dll", b"x")]);
        let r = wg_runner(
            &zip,
            "InstallerType: zip\nNestedInstallerType: portable\n",
            WG_TABLE,
        );
        winget_apply(&r, &app, Some("1.0"), "2.0", "Ven.App", &mut |_, _| {}).unwrap();
        assert!(app.dir.join("nuovo.exe").exists());
        assert!(!app.dir.join("notepad++.exe").exists());
        assert_eq!(fs::read(app.dir.join("Data/utente.ini")).unwrap(), b"mio");
        assert_eq!(state::load(&app.dir).version.as_deref(), Some("2.0"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn winget_apply_refuses_installers_and_keeps_the_app() {
        let d = tmp("wgb");
        let app = app_in(&d, "Ventoy");
        let zip = d.join("p.zip");
        make_zip(&zip, &[("setup.exe", b"x")]);
        let r = wg_runner(
            &zip,
            "InstallerType: zip\nNestedInstallerType: nullsoft\n",
            WG_TABLE,
        );
        let e = winget_apply(&r, &app, Some("1.0"), "2.0", "Ven.App", &mut |_, _| {}).unwrap_err();
        assert!(e.contains("non una versione portabile"), "{e}");
        assert_eq!(fs::read(app.dir.join("notepad++.exe")).unwrap(), b"vecchio");
        assert!(!d.join("_temp").join("Ventoy").exists());
        let _ = fs::remove_dir_all(&d);
    }
}
