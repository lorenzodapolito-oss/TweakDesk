//! Programmi non portabili: cartella `Installers`, riconoscimento del tipo di installer
//! e creazione degli script di installazione silenziosa.

use crate::liber::scan::SEPARATOR;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstKind {
    Msi,
    Inno,
    Nsis,
    InstallShield,
    Burn,
    Unknown,
}

impl InstKind {
    pub fn label(self) -> &'static str {
        match self {
            InstKind::Msi => "MSI",
            InstKind::Inno => "Inno Setup",
            InstKind::Nsis => "NSIS",
            InstKind::InstallShield => "InstallShield",
            InstKind::Burn => "WiX Burn",
            InstKind::Unknown => "tipo da scegliere",
        }
    }

    /// Parametri per l'installazione silenziosa di un `.exe` (per l'MSI si usa msiexec).
    pub fn switches(self) -> Option<&'static str> {
        match self {
            InstKind::Msi | InstKind::Unknown => None,
            InstKind::Inno => Some("/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP-"),
            InstKind::Nsis => Some("/S"),
            InstKind::InstallShield => Some("/s /v\"/qn /norestart\""),
            InstKind::Burn => Some("/quiet /norestart"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Installer {
    pub name: String,
    pub file: PathBuf,
    /// Percorso relativo alla cartella `Installers`, con `/`.
    pub rel: String,
    pub category: Option<String>,
    pub kind: InstKind,
}

/// Quanti byte iniziali si leggono per riconoscere il tipo.
const HEAD_BYTES: u64 = 12 * 1024 * 1024;

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && hay.windows(needle.len()).any(|w| w == needle)
}

/// Riconosce il tipo dai primi byte del file.
pub fn detect_bytes(is_msi: bool, head: &[u8]) -> InstKind {
    if is_msi {
        return InstKind::Msi;
    }
    if head.starts_with(&[0xD0, 0xCF, 0x11, 0xE0]) {
        return InstKind::Msi;
    }
    if contains(head, b".wixburn") || contains(head, b"WixBurn") {
        return InstKind::Burn;
    }
    if contains(head, b"Inno Setup") {
        return InstKind::Inno;
    }
    if contains(head, b"Nullsoft Install System") || contains(head, b"NullsoftInst") {
        return InstKind::Nsis;
    }
    if contains(head, b"InstallShield") {
        return InstKind::InstallShield;
    }
    InstKind::Unknown
}

pub fn detect(path: &Path) -> InstKind {
    let is_msi = path
        .extension()
        .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case("msi"));
    let mut buf = Vec::new();
    if let Ok(f) = fs::File::open(path) {
        let _ = f.take(HEAD_BYTES).read_to_end(&mut buf);
    }
    detect_bytes(is_msi, &buf)
}

fn is_installer_file(p: &Path) -> bool {
    p.extension().is_some_and(|e| {
        let e = e.to_string_lossy().to_lowercase();
        e == "exe" || e == "msi"
    })
}

fn is_msi(p: &Path) -> bool {
    p.extension()
        .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case("msi"))
}

/// Nome del file, con separatore sia `\\` sia `/` (indipendente dal sistema).
pub fn base_name_of(p: &Path) -> String {
    base_name(p).unwrap_or_default()
}

fn base_name(p: &Path) -> Option<String> {
    let s = p.to_string_lossy();
    let n = s.rsplit(['\\', '/']).next()?;
    (!n.is_empty()).then(|| n.to_string())
}

/// Riga che avvia l'installazione silenziosa; `%~dp0` è la cartella dello script.
pub fn command_line(inst: &Installer, custom: Option<&str>) -> Option<String> {
    let file = base_name(&inst.file)?.replace('%', "%%");
    let target = format!("\"%~dp0{file}\"");
    let custom = custom.map(str::trim).filter(|c| !c.is_empty());
    if is_msi(&inst.file) {
        let extra = custom.unwrap_or("/qn /norestart");
        return Some(format!("start /wait \"\" msiexec /i {target} {extra}"));
    }
    let sw = custom.or_else(|| inst.kind.switches())?;
    Some(format!("start /wait \"\" {target} {sw}"))
}

pub fn script_text(inst: &Installer, custom: Option<&str>) -> Option<String> {
    let cmd = command_line(inst, custom)?;
    let lines = [
        "@echo off".to_string(),
        format!(
            "rem Installazione silenziosa di {} (generato da LiberDesk)",
            inst.name.replace(['\r', '\n'], " ")
        ),
        "net session >nul 2>&1".to_string(),
        "if errorlevel 1 (".to_string(),
        "    echo Servono i permessi di amministratore: riavvia come amministratore.".to_string(),
        "    pause".to_string(),
        "    exit /b 1".to_string(),
        ")".to_string(),
        cmd,
        "exit /b %errorlevel%".to_string(),
    ];
    Some(lines.join("\r\n") + "\r\n")
}

pub fn safe_name(s: &str) -> String {
    let t: String = s
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '.' | '+') {
                c
            } else {
                '_'
            }
        })
        .collect();
    t.trim_matches('_').to_string()
}

pub fn script_name(inst: &Installer) -> String {
    let base = base_name(&inst.file).unwrap_or_default();
    let stem = base
        .rsplit_once('.')
        .map_or(base.as_str(), |(s, _)| s)
        .to_string();
    format!("Installa_{}.cmd", safe_name(&stem))
}

/// Scrive lo script accanto all'installer; `None` se il tipo non è noto e non c'è un parametro.
pub fn write_script(inst: &Installer, custom: Option<&str>) -> Result<Option<PathBuf>, String> {
    let Some(text) = script_text(inst, custom) else {
        return Ok(None);
    };
    let dir = inst.file.parent().ok_or("Percorso non valido")?;
    let path = dir.join(script_name(inst));
    fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Some(path))
}

/// Script unico che richiama quelli di ogni programma, nell'ordine dato.
pub fn master_text(scripts: &[(String, String)]) -> String {
    let mut l = vec![
        "@echo off".to_string(),
        "rem Installa tutti i programmi in modo silenzioso (generato da LiberDesk)".to_string(),
        "net session >nul 2>&1".to_string(),
        "if errorlevel 1 (".to_string(),
        "    echo Servono i permessi di amministratore: riavvia come amministratore.".to_string(),
        "    pause".to_string(),
        "    exit /b 1".to_string(),
        ")".to_string(),
    ];
    for (name, rel) in scripts {
        let rel = rel.replace('/', "\\").replace('%', "%%");
        l.push(format!(
            "echo Installo {}...",
            name.replace(['&', '|', '<', '>', '^', '%', '"'], " ")
        ));
        l.push(format!("call \"%~dp0{rel}\""));
    }
    l.push("echo Fatto.".to_string());
    l.push("pause".to_string());
    l.join("\r\n") + "\r\n"
}

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Entry {
    /// Parametri silenziosi scelti dall'utente (vincono su quelli riconosciuti).
    pub switches: Option<String>,
    pub winget_id: Option<String>,
    pub version: Option<String>,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Store {
    pub items: BTreeMap<String, Entry>,
}

pub const STORE_FILE: &str = "_liberdesk.json";

pub fn load_store(root: &Path) -> Store {
    fs::read_to_string(root.join(STORE_FILE))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save_store(root: &Path, st: &Store) -> Result<(), String> {
    fs::create_dir_all(root).map_err(|e| format!("{}: {e}", root.display()))?;
    let text = serde_json::to_string_pretty(st).map_err(|e| e.to_string())?;
    fs::write(root.join(STORE_FILE), text).map_err(|e| format!("{}: {e}", root.display()))
}

const MAX_LEVELS: usize = 2;

fn walk(dir: &Path, labels: &mut Vec<String>, root: &Path, out: &mut Vec<Installer>) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with(['_', '.']) {
            continue;
        }
        let Ok(ft) = e.file_type() else {
            continue;
        };
        let p = e.path();
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            // Oltre il secondo livello le cartelle non aggiungono categorie.
            if labels.len() < MAX_LEVELS {
                labels.push(name);
                walk(&p, labels, root, out);
                labels.pop();
            } else {
                walk(&p, labels, root, out);
            }
        } else if ft.is_file() && is_installer_file(&p) {
            let lower = name.to_lowercase();
            if lower.contains("unins") {
                continue;
            }
            let rel = p
                .strip_prefix(root)
                .map(|r| r.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| name.clone());
            let stem = p
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or(name);
            out.push(Installer {
                name: stem,
                kind: detect(&p),
                file: p,
                rel,
                category: (!labels.is_empty()).then(|| labels.join(SEPARATOR)),
            });
        }
    }
}

/// Tutti gli installer della cartella, in ordine alfabetico.
pub fn scan(root: &Path) -> Vec<Installer> {
    let mut out = Vec::new();
    walk(root, &mut Vec::new(), root, &mut out);
    out.sort_by_key(|i| i.name.to_lowercase());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ld_inst_{}_{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn inst(file: &str, kind: InstKind) -> Installer {
        Installer {
            name: "App".into(),
            file: PathBuf::from(file),
            rel: file.into(),
            category: None,
            kind,
        }
    }

    #[test]
    fn detects_installer_types() {
        assert_eq!(detect_bytes(true, b"qualsiasi"), InstKind::Msi);
        assert_eq!(
            detect_bytes(false, &[0xD0, 0xCF, 0x11, 0xE0, 1, 2]),
            InstKind::Msi
        );
        assert_eq!(
            detect_bytes(false, b"MZ....Inno Setup Setup Data (6.2)"),
            InstKind::Inno
        );
        assert_eq!(
            detect_bytes(false, b"MZ..Nullsoft Install System v3.08"),
            InstKind::Nsis
        );
        assert_eq!(
            detect_bytes(false, b"MZ..InstallShield Setup"),
            InstKind::InstallShield
        );
        assert_eq!(detect_bytes(false, b"MZ..\x00.wixburn\x00"), InstKind::Burn);
        assert_eq!(
            detect_bytes(false, b"MZ solo un programma"),
            InstKind::Unknown
        );
    }

    #[test]
    fn silent_command_lines() {
        let l = |f: &str, k| command_line(&inst(f, k), None);
        assert_eq!(
            l("C:\\x\\app.msi", InstKind::Msi).unwrap(),
            "start /wait \"\" msiexec /i \"%~dp0app.msi\" /qn /norestart"
        );
        assert_eq!(
            l("C:\\x\\setup.exe", InstKind::Inno).unwrap(),
            "start /wait \"\" \"%~dp0setup.exe\" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP-"
        );
        assert!(l("a.exe", InstKind::Nsis).unwrap().ends_with("/S"));
        assert!(l("a.exe", InstKind::Burn).unwrap().contains("/quiet"));
        assert!(l("a.exe", InstKind::Unknown).is_none());
        // Il parametro scelto dall'utente vince.
        assert!(
            command_line(&inst("a.exe", InstKind::Unknown), Some("/silent"))
                .unwrap()
                .ends_with("/silent")
        );
        assert!(command_line(&inst("a.exe", InstKind::Nsis), Some("  "))
            .unwrap()
            .ends_with("/S"));
        // Il percento si raddoppia negli script.
        assert!(l("50%.exe", InstKind::Nsis).unwrap().contains("50%%.exe"));
    }

    #[test]
    fn script_has_admin_check_and_crlf() {
        let t = script_text(&inst("C:\\x\\app.msi", InstKind::Msi), None).unwrap();
        assert!(t.starts_with("@echo off\r\n"));
        assert!(t.contains("net session"));
        assert!(t.contains("msiexec /i"));
        assert!(t.ends_with("exit /b %errorlevel%\r\n"));
        assert!(!t.replace("\r\n", "").contains('\n'));
        assert!(script_text(&inst("a.exe", InstKind::Unknown), None).is_none());
    }

    #[test]
    fn script_names_are_safe() {
        let i = Installer {
            file: PathBuf::from("C:\\x\\Mio Programma (v2) & co.exe"),
            ..inst("x", InstKind::Nsis)
        };
        let n = script_name(&i);
        assert_eq!(n, "Installa_Mio_Programma__v2____co.cmd");
        assert!(!n.contains([' ', '&', '(']));
    }

    #[test]
    fn master_script_calls_every_script() {
        let t = master_text(&[
            ("Firefox".into(), "Internet/Installa_Firefox.cmd".into()),
            ("A&B".into(), "Installa_AB.cmd".into()),
        ]);
        assert!(t.contains("call \"%~dp0Internet\\Installa_Firefox.cmd\""));
        assert!(t.contains("call \"%~dp0Installa_AB.cmd\""));
        assert!(t.contains("echo Installo A B..."));
        assert!(t.contains("net session"));
    }

    #[test]
    fn scan_finds_installers_with_categories() {
        let d = tmp("scan");
        fs::create_dir_all(d.join("Internet/Browser")).unwrap();
        fs::create_dir_all(d.join("_vecchi")).unwrap();
        fs::write(
            d.join("Internet/Browser/Firefox Setup.exe"),
            b"MZ Nullsoft Install System",
        )
        .unwrap();
        fs::write(d.join("Internet/Thunderbird.msi"), b"x").unwrap();
        fs::write(d.join("7zip.exe"), b"MZ Inno Setup").unwrap();
        fs::write(d.join("Internet/unins000.exe"), b"x").unwrap();
        fs::write(d.join("_vecchi/vecchio.exe"), b"x").unwrap();
        fs::write(d.join("Internet/note.txt"), b"x").unwrap();
        fs::write(d.join("Installa_7zip.cmd"), b"x").unwrap();
        let v = scan(&d);
        let names: Vec<_> = v.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, vec!["7zip", "Firefox Setup", "Thunderbird"]);
        let ff = &v[1];
        assert_eq!(ff.category.as_deref(), Some("Internet › Browser"));
        assert_eq!(ff.rel, "Internet/Browser/Firefox Setup.exe");
        assert_eq!(ff.kind, InstKind::Nsis);
        assert_eq!(v[0].kind, InstKind::Inno);
        assert_eq!(v[0].category, None);
        assert_eq!(v[2].kind, InstKind::Msi);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn writes_script_next_to_the_installer() {
        let d = tmp("ws");
        fs::write(d.join("a.exe"), b"MZ Inno Setup").unwrap();
        let i = scan(&d).remove(0);
        let p = write_script(&i, None).unwrap().unwrap();
        assert_eq!(p, d.join("Installa_a.cmd"));
        assert!(fs::read_to_string(&p).unwrap().contains("/VERYSILENT"));
        fs::write(d.join("b.exe"), b"MZ boh").unwrap();
        let b = scan(&d).into_iter().find(|i| i.name == "b").unwrap();
        assert_eq!(write_script(&b, None).unwrap(), None);
        assert!(write_script(&b, Some("/Q")).unwrap().is_some());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn store_round_trip() {
        let d = tmp("store");
        assert_eq!(load_store(&d), Store::default());
        let mut s = Store::default();
        s.items.insert(
            "a.exe".into(),
            Entry {
                winget_id: Some("X.Y".into()),
                ..Entry::default()
            },
        );
        save_store(&d, &s).unwrap();
        assert_eq!(load_store(&d), s);
        fs::write(d.join(STORE_FILE), "rotto").unwrap();
        assert_eq!(load_store(&d), Store::default());
        let _ = fs::remove_dir_all(&d);
    }
}
