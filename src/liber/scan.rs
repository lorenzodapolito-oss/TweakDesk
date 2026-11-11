//! Trova le app nella cartella: ogni sottocartella con un `.exe` è un'app.

use crate::liber::catalog::{self, normalize, Entry};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct App {
    pub name: String,
    pub dir: PathBuf,
    pub exe: Option<PathBuf>,
    pub entry: Option<&'static Entry>,
    /// Cartella principale delle app (dove stanno anche `_temp` e `_backup`).
    pub root: PathBuf,
    /// Se l'app sta dentro una cartella di categoria, il nome di quella cartella.
    pub folder_category: Option<String>,
}

const SKIP_WORDS: [&str; 6] = ["unins", "uninst", "setup", "update", "crash", "helper"];

fn is_exe(p: &Path) -> bool {
    p.extension()
        .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case("exe"))
}

/// File `.exe` della cartella e delle sottocartelle (al massimo 2 livelli).
fn exes_in(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(ft) = e.file_type() else {
            continue;
        };
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() && depth < 2 {
            exes_in(&p, depth + 1, out);
        } else if ft.is_file() && is_exe(&p) {
            out.push(p);
        }
    }
}

/// Eseguibile principale: quello del catalogo, poi quello col nome della cartella,
/// poi il più grande che non sia un disinstallatore o un programma di servizio.
pub fn find_exe(dir: &Path, entry: Option<&Entry>) -> Option<PathBuf> {
    if let Some(e) = entry {
        if !e.exe.is_empty() {
            let p = e
                .exe
                .split(['\\', '/'])
                .fold(dir.to_path_buf(), |a, s| a.join(s));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    let mut all = Vec::new();
    exes_in(dir, 0, &mut all);
    let folder = normalize(
        &dir.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
    );
    let stem = |p: &Path| {
        normalize(
            &p.file_stem()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
        )
    };
    if let Some(p) = all.iter().find(|p| stem(p) == folder) {
        return Some(p.clone());
    }
    let good = |p: &&PathBuf| {
        let s = stem(p);
        !SKIP_WORDS.iter().any(|w| s.contains(w))
    };
    let size = |p: &PathBuf| fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    all.iter().filter(good).max_by_key(|p| size(p)).cloned()
}

/// Cartelle che di solito stanno dentro un programma e non sono programmi a sé.
const GENERIC_SUBDIRS: [&str; 12] = [
    "bin", "app", "apps", "program", "programs", "x64", "x86", "win64", "win32", "32bit", "64bit",
    "files",
];

/// Sottocartelle (senza quelle che cominciano con `_` o `.`).
fn subdirs(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(rd) = fs::read_dir(dir) else {
        return Vec::new();
    };
    rd.flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            (!name.starts_with(['_', '.'])).then(|| (name, e.path()))
        })
        .collect()
}

fn has_direct_exe(dir: &Path) -> bool {
    fs::read_dir(dir).is_ok_and(|rd| {
        rd.flatten()
            .any(|e| e.file_type().is_ok_and(|t| t.is_file()) && is_exe(&e.path()))
    })
}

fn make_app(name: String, dir: PathBuf, root: &Path, category: Option<&str>) -> Option<App> {
    let entry = catalog::find_for_folder(&name);
    let exe = find_exe(&dir, entry);
    exe.as_ref()?;
    Some(App {
        name: entry.map_or(name, |en| en.name.to_string()),
        dir,
        exe,
        entry,
        root: root.to_path_buf(),
        folder_category: category.map(String::from),
    })
}

/// Quanti livelli di cartelle di categoria si accettano (`Categoria\\Sottocategoria\\Programma`).
const MAX_CATEGORY_LEVELS: usize = 2;
pub const SEPARATOR: &str = " › ";

fn walk(dir: &Path, labels: &mut Vec<String>, root: &Path, out: &mut Vec<App>) {
    for (name, d) in subdirs(dir) {
        let known = catalog::find_for_folder(&name).is_some();
        let children: Vec<(String, PathBuf)> = subdirs(&d)
            .into_iter()
            .filter(|(n, _)| !GENERIC_SUBDIRS.contains(&n.to_lowercase().as_str()))
            .collect();
        let is_category = !known
            && labels.len() < MAX_CATEGORY_LEVELS
            && !has_direct_exe(&d)
            && children.iter().any(|(_, c)| find_exe(c, None).is_some());
        if is_category {
            labels.push(name);
            walk(&d, labels, root, out);
            labels.pop();
        } else {
            let cat = (!labels.is_empty()).then(|| labels.join(SEPARATOR));
            out.extend(make_app(name, d, root, cat.as_deref()));
        }
    }
}

/// Elenco delle app, in ordine alfabetico. Una cartella senza `.exe` propri che contiene
/// altri programmi è una categoria (anche annidata, fino a due livelli): le app che ci stanno
/// dentro ne prendono il nome, es. `Video › Editor`.
pub fn scan(apps_dir: &Path) -> Vec<App> {
    let mut apps = Vec::new();
    walk(apps_dir, &mut Vec::new(), apps_dir, &mut apps);
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ld_scan_{}_{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn file(p: &Path, size: usize) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, vec![b'x'; size]).unwrap();
    }

    #[test]
    fn finds_apps_and_picks_the_right_exe() {
        let d = tmp("a");
        file(&d.join("Notepad++/notepad++.exe"), 10);
        file(&d.join("Notepad++/updater/gup.exe"), 500);
        file(&d.join("Zeta/zeta.exe"), 5);
        file(&d.join("Zeta/unins000.exe"), 900);
        file(&d.join("Alfa/strumento.exe"), 50);
        file(&d.join("Alfa/setup.exe"), 9000);
        file(&d.join("SoloDati/leggimi.txt"), 5);
        file(&d.join("_backup/x/x.exe"), 5);
        file(&d.join(".nascosta/y.exe"), 5);
        let apps = scan(&d);
        let names: Vec<_> = apps.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["Alfa", "Notepad++", "Zeta"]);
        let exe = |n: &str| {
            apps.iter()
                .find(|a| a.name == n)
                .and_then(|a| a.exe.as_ref())
                .and_then(|p| p.file_name())
                .map(|s| s.to_string_lossy().to_string())
        };
        assert_eq!(exe("Alfa").as_deref(), Some("strumento.exe"));
        assert_eq!(exe("Zeta").as_deref(), Some("zeta.exe"));
        assert_eq!(exe("Notepad++").as_deref(), Some("notepad++.exe"));
        assert!(apps
            .iter()
            .find(|a| a.name == "Notepad++")
            .unwrap()
            .entry
            .is_some());
        assert!(apps
            .iter()
            .find(|a| a.name == "Zeta")
            .unwrap()
            .entry
            .is_none());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn catalog_exe_in_a_subfolder_is_used() {
        let d = tmp("b");
        file(&d.join("OBS/bin/64bit/obs64.exe"), 10);
        file(&d.join("OBS/bin/64bit/obs-ffmpeg-mux.exe"), 900);
        let apps = scan(&d);
        assert_eq!(apps.len(), 1);
        assert!(apps[0].exe.as_ref().unwrap().ends_with("obs64.exe"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn category_folders_hold_several_programs() {
        let d = tmp("cat");
        file(&d.join("Internet/Firefox/firefox.exe"), 10);
        file(&d.join("Internet/Notepad++/notepad++.exe"), 10);
        file(&d.join("Multimedia/VLC/vlc.exe"), 10);
        file(&d.join("Multimedia/_vecchio/x.exe"), 10);
        file(&d.join("OBS/bin/64bit/obs64.exe"), 10);
        file(&d.join("Piatta/piatta.exe"), 10);
        file(&d.join("Utility/Tool/bin/tool.exe"), 10);
        let apps = scan(&d);
        let get = |n: &str| apps.iter().find(|a| a.name == n);
        assert_eq!(
            get("Firefox").unwrap().folder_category.as_deref(),
            Some("Internet")
        );
        assert_eq!(
            get("Notepad++").unwrap().folder_category.as_deref(),
            Some("Internet")
        );
        assert_eq!(
            get("VLC").unwrap().folder_category.as_deref(),
            Some("Multimedia")
        );
        assert_eq!(
            get("Tool").unwrap().folder_category.as_deref(),
            Some("Utility")
        );
        // Programmi senza categoria, anche con exe in sottocartelle.
        assert_eq!(get("OBS Studio").unwrap().folder_category, None);
        assert_eq!(get("Piatta").unwrap().folder_category, None);
        assert!(apps.iter().all(|a| a.root == d));
        assert_eq!(apps.len(), 6);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn nested_categories_become_a_path() {
        let d = tmp("nest");
        file(&d.join("Video/Editor/VirtualDub/vdub.exe"), 10);
        file(&d.join("Video/Editor/Avidemux/avidemux.exe"), 10);
        file(&d.join("Video/Riproduttore/MPC/mpc.exe"), 10);
        file(&d.join("Video/XMediaRecode3271/XMedia Recode.exe"), 10);
        file(&d.join("Utilità di sistema/Strumenti/Tool A/a.exe"), 10);
        file(&d.join("XnConvertPortable/XnConvertPortable.exe"), 10);
        file(&d.join("XnConvertPortable/App/XnConvert/xnconvert.exe"), 10);
        file(&d.join("XnConvertPortable/Data/x.ini"), 1);
        let apps = scan(&d);
        let cat = |n: &str| {
            apps.iter()
                .find(|a| a.name == n)
                .map(|a| a.folder_category.clone())
        };
        assert_eq!(cat("VirtualDub"), Some(Some("Video › Editor".to_string())));
        assert_eq!(cat("MPC"), Some(Some("Video › Riproduttore".to_string())));
        assert_eq!(cat("XMediaRecode3271"), Some(Some("Video".to_string())));
        assert_eq!(
            cat("Tool A"),
            Some(Some("Utilità di sistema › Strumenti".to_string()))
        );
        assert_eq!(cat("XnConvertPortable"), Some(None));
        assert_eq!(apps.len(), 6);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn missing_folder_is_empty() {
        assert!(scan(Path::new("/non/esiste/mai")).is_empty());
    }
}
