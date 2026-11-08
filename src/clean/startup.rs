//! Programmi che partono con Windows: si vedono, si disattivano (come il Task Manager), si riattivano o si eliminano.

use crate::clean::engine::Env;
use crate::clean::sys::Sys;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Chiave Run (valore `hive` + `key`), con la relativa chiave StartupApproved.
    Registry,
    Folder,
    Task,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub command: String,
    pub source: Source,
    /// Etichetta leggibile: «Registro (utente)», «Cartella Esecuzione automatica»...
    pub origin: String,
    pub enabled: bool,
    /// Chiave Run / chiave StartupApproved / percorso file / nome attività.
    pub location: String,
    pub approved_key: String,
    /// Voce di Microsoft/Windows: di solito è meglio lasciarla.
    pub system: bool,
    pub needs_admin: bool,
}

const RUN_USER: &str = "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_PC: &str = "HKLM\\Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_PC32: &str = "HKLM\\Software\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Run";
const APPR_USER_RUN: &str =
    "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run";
const APPR_PC_RUN: &str =
    "HKLM\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run";
const APPR_PC_RUN32: &str =
    "HKLM\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run32";
const APPR_USER_FOLDER: &str =
    "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\StartupFolder";
const APPR_PC_FOLDER: &str =
    "HKLM\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\StartupFolder";

const ENABLED_HEX: &str = "020000000000000000000000";
const DISABLED_HEX: &str = "030000000000000000000000";

/// Windows: primo byte pari = attivo (02, 06), dispari = disattivato (03).
pub fn is_enabled_hex(hex: &str) -> bool {
    match hex.get(0..2).and_then(|h| u8::from_str_radix(h, 16).ok()) {
        Some(b) => b % 2 == 0,
        None => true,
    }
}

fn approved(sys: &dyn Sys, key: &str, name: &str) -> bool {
    sys.reg_query(key)
        .iter()
        .find(|i| i.name.eq_ignore_ascii_case(name))
        .is_none_or(|i| is_enabled_hex(&i.data))
}

fn is_system_command(cmd: &str) -> bool {
    let c = cmd.to_lowercase();
    c.contains("\\windows\\system32\\") || c.contains("\\windows\\syswow64\\")
}

fn registry_entries(
    sys: &dyn Sys,
    run_key: &str,
    appr_key: &str,
    origin: &str,
    admin: bool,
    out: &mut Vec<Entry>,
) {
    for i in sys.reg_query(run_key) {
        if !i.kind.starts_with("REG_") || i.name.is_empty() {
            continue;
        }
        out.push(Entry {
            enabled: approved(sys, appr_key, &i.name),
            system: is_system_command(&i.data),
            command: i.data,
            name: i.name,
            source: Source::Registry,
            origin: origin.into(),
            location: run_key.into(),
            approved_key: appr_key.into(),
            needs_admin: admin,
        });
    }
}

fn folder_entries(
    sys: &dyn Sys,
    dir: &Path,
    appr_key: &str,
    origin: &str,
    admin: bool,
    out: &mut Vec<Entry>,
) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name.eq_ignore_ascii_case("desktop.ini") {
            continue;
        }
        let path = e.path();
        if !path.is_file() {
            continue;
        }
        out.push(Entry {
            enabled: approved(sys, appr_key, &name),
            command: path.to_string_lossy().to_string(),
            name,
            source: Source::Folder,
            origin: origin.into(),
            location: path.to_string_lossy().to_string(),
            approved_key: appr_key.into(),
            system: false,
            needs_admin: admin,
        });
    }
}

fn tag_text<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let i = xml.find(&open)? + open.len();
    let j = xml[i..].find(&close)? + i;
    xml.get(i..j)
}

/// Attività pianificate che partono all'accesso o all'avvio (dal testo XML di `schtasks /query /xml`).
pub fn parse_tasks(xml: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    for block in xml.split("</Task>") {
        let Some(start) = block.find("<Task") else {
            continue;
        };
        let body = &block[start..];
        if !(body.contains("<LogonTrigger") || body.contains("<BootTrigger")) {
            continue;
        }
        let Some(uri) = tag_text(body, "URI") else {
            continue;
        };
        let uri = uri.trim().to_string();
        let command = tag_text(body, "Command").unwrap_or("").trim().to_string();
        let args = tag_text(body, "Arguments").unwrap_or("").trim();
        let command = if args.is_empty() {
            command
        } else {
            format!("{command} {args}")
        };
        let enabled = !tag_text(body, "Settings")
            .and_then(|s| tag_text(s, "Enabled"))
            .is_some_and(|e| e.trim().eq_ignore_ascii_case("false"));
        let system = uri.starts_with("\\Microsoft\\");
        out.push(Entry {
            name: uri.trim_start_matches('\\').to_string(),
            command,
            source: Source::Task,
            origin: "Attività pianificata".into(),
            enabled,
            location: uri,
            approved_key: String::new(),
            system,
            needs_admin: true,
        });
    }
    out
}

/// Tutte le voci di avvio.
pub fn list(sys: &dyn Sys, env: &dyn Env) -> Vec<Entry> {
    let mut out = Vec::new();
    registry_entries(
        sys,
        RUN_USER,
        APPR_USER_RUN,
        "Registro (utente)",
        false,
        &mut out,
    );
    registry_entries(
        sys,
        RUN_PC,
        APPR_PC_RUN,
        "Registro (tutti gli utenti)",
        true,
        &mut out,
    );
    registry_entries(
        sys,
        RUN_PC32,
        APPR_PC_RUN32,
        "Registro (32 bit)",
        true,
        &mut out,
    );
    if let Some(a) = env.var("APPDATA") {
        let dir = PathBuf::from(a).join("Microsoft\\Windows\\Start Menu\\Programs\\Startup");
        folder_entries(
            sys,
            &dir,
            APPR_USER_FOLDER,
            "Cartella Esecuzione automatica",
            false,
            &mut out,
        );
    }
    if let Some(p) = env.var("ProgramData") {
        let dir = PathBuf::from(p).join("Microsoft\\Windows\\Start Menu\\Programs\\StartUp");
        folder_entries(
            sys,
            &dir,
            APPR_PC_FOLDER,
            "Cartella Esecuzione automatica (tutti)",
            true,
            &mut out,
        );
    }
    if let Ok(xml) = sys.run("schtasks", &["/query", "/xml", "ONE"]) {
        out.extend(parse_tasks(&xml));
    }
    out.sort_by_key(|e| (e.system, e.name.to_lowercase()));
    out
}

/// Disattiva o riattiva una voce (reversibile).
pub fn set_enabled(sys: &dyn Sys, e: &Entry, enable: bool) -> Result<(), String> {
    match e.source {
        Source::Registry | Source::Folder => {
            let name = match e.source {
                Source::Folder => Path::new(&e.location)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| e.name.clone()),
                _ => e.name.clone(),
            };
            sys.reg_set_binary(
                &e.approved_key,
                &name,
                if enable { ENABLED_HEX } else { DISABLED_HEX },
            )
        }
        Source::Task => sys
            .run(
                "schtasks",
                &[
                    "/change",
                    "/tn",
                    &e.location,
                    if enable { "/enable" } else { "/disable" },
                ],
            )
            .map(|_| ()),
    }
}

fn safe_name(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Elimina definitivamente una voce, dopo averne salvato una copia in `backup_dir`.
pub fn delete(sys: &dyn Sys, e: &Entry, backup_dir: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(backup_dir).map_err(|e| format!("{}: {e}", backup_dir.display()))?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let base = format!("{stamp}-{}", safe_name(&e.name));
    match e.source {
        Source::Registry => {
            let file = backup_dir.join(format!("{base}.reg"));
            sys.reg_export(&e.location, &file)?;
            sys.reg_delete_value(&e.location, &e.name)?;
            let _ = sys.reg_delete_value(&e.approved_key, &e.name);
            Ok(file)
        }
        Source::Folder => {
            let src = PathBuf::from(&e.location);
            let file_name = src
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| e.name.clone());
            let dest = backup_dir.join(format!("{base}-{file_name}"));
            if fs::rename(&src, &dest).is_err() {
                fs::copy(&src, &dest).map_err(|e| format!("copia: {e}"))?;
                fs::remove_file(&src).map_err(|e| format!("eliminazione: {e}"))?;
            }
            let _ = sys.reg_delete_value(&e.approved_key, &file_name);
            Ok(dest)
        }
        Source::Task => {
            let file = backup_dir.join(format!("{base}.xml"));
            let xml = sys.run("schtasks", &["/query", "/tn", &e.location, "/xml"])?;
            fs::write(&file, xml).map_err(|e| format!("{}: {e}", file.display()))?;
            sys.run("schtasks", &["/delete", "/tn", &e.location, "/f"])?;
            Ok(file)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clean::sys::fake::FakeSys;
    use std::collections::HashMap;

    struct TestEnv(HashMap<String, String>);
    impl Env for TestEnv {
        fn var(&self, n: &str) -> Option<String> {
            self.0.get(n).cloned()
        }
    }

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("tdstart_{}_{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    const TASKS: &str = r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2"><RegistrationInfo><URI>\GoogleUpdateTaskUser</URI></RegistrationInfo>
<Triggers><LogonTrigger><Enabled>true</Enabled></LogonTrigger></Triggers>
<Settings><Enabled>true</Enabled></Settings>
<Actions><Exec><Command>C:\Google\Update.exe</Command><Arguments>/ua</Arguments></Exec></Actions></Task>
<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2"><RegistrationInfo><URI>\Microsoft\Windows\X\Y</URI></RegistrationInfo>
<Triggers><BootTrigger/></Triggers><Settings><Enabled>false</Enabled></Settings>
<Actions><Exec><Command>sys.exe</Command></Exec></Actions></Task>
<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2"><RegistrationInfo><URI>\Giornaliera</URI></RegistrationInfo>
<Triggers><CalendarTrigger/></Triggers><Settings><Enabled>true</Enabled></Settings>
<Actions><Exec><Command>a.exe</Command></Exec></Actions></Task>"#;

    #[test]
    fn enabled_flag_from_hex() {
        assert!(is_enabled_hex("020000000000000000000000"));
        assert!(is_enabled_hex("060000000000000000000000"));
        assert!(!is_enabled_hex("030000000000000000000000"));
        assert!(is_enabled_hex(""));
    }

    #[test]
    fn parses_only_logon_and_boot_tasks() {
        let v = parse_tasks(TASKS);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].name, "GoogleUpdateTaskUser");
        assert_eq!(v[0].command, "C:\\Google\\Update.exe /ua");
        assert!(v[0].enabled && !v[0].system);
        assert!(v[1].system && !v[1].enabled);
    }

    #[test]
    fn lists_registry_folder_and_tasks() {
        let d = tmp("list");
        let appdata = d.join("AppData");
        let startup = appdata.join("Microsoft\\Windows\\Start Menu\\Programs\\Startup");
        // Su Linux il separatore `\` fa parte del nome: usiamo lo stesso percorso della funzione.
        fs::create_dir_all(&startup).unwrap();
        fs::write(startup.join("Mio.lnk"), b"x").unwrap();
        fs::write(startup.join("desktop.ini"), b"x").unwrap();
        let env = TestEnv([("APPDATA".to_string(), appdata.to_string_lossy().to_string())].into());
        let sys = FakeSys::default();
        sys.add(RUN_USER, "Spotify", "REG_SZ", "C:\\Spotify.exe");
        sys.add(RUN_USER, "Discord", "REG_SZ", "C:\\Discord.exe");
        sys.add(APPR_USER_RUN, "Discord", "REG_BINARY", DISABLED_HEX);
        sys.add(RUN_PC, "Driver", "REG_SZ", "C:\\Windows\\System32\\drv.exe");
        *sys.tasks_xml.borrow_mut() = TASKS.to_string();
        let v = list(&sys, &env);
        let by = |n: &str| v.iter().find(|e| e.name == n).unwrap().clone();
        assert!(by("Spotify").enabled);
        assert!(!by("Discord").enabled);
        assert!(by("Driver").system && by("Driver").needs_admin);
        assert_eq!(by("Mio.lnk").source, Source::Folder);
        assert!(v.iter().all(|e| e.name != "desktop.ini"));
        // Le voci di sistema vanno in fondo.
        let first_system = v.iter().position(|e| e.system).unwrap();
        assert!(v[first_system..].iter().all(|e| e.system));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn disable_and_enable_registry_entry() {
        let sys = FakeSys::default();
        sys.add(RUN_USER, "Spotify", "REG_SZ", "C:\\Spotify.exe");
        let env = TestEnv(HashMap::new());
        let e = list(&sys, &env)
            .into_iter()
            .find(|e| e.name == "Spotify")
            .unwrap();
        set_enabled(&sys, &e, false).unwrap();
        assert_eq!(
            sys.get(APPR_USER_RUN, "Spotify").as_deref(),
            Some(DISABLED_HEX)
        );
        assert!(
            !list(&sys, &env)
                .iter()
                .find(|e| e.name == "Spotify")
                .unwrap()
                .enabled
        );
        set_enabled(&sys, &e, true).unwrap();
        assert!(
            list(&sys, &env)
                .iter()
                .find(|e| e.name == "Spotify")
                .unwrap()
                .enabled
        );
        // La voce originale non è stata toccata.
        assert!(sys.get(RUN_USER, "Spotify").is_some());
    }

    #[test]
    fn task_toggle_uses_schtasks() {
        let sys = FakeSys::default();
        let e = parse_tasks(TASKS).remove(0);
        set_enabled(&sys, &e, false).unwrap();
        set_enabled(&sys, &e, true).unwrap();
        let c = sys.commands.borrow().clone();
        assert_eq!(c[0], "schtasks /change /tn \\GoogleUpdateTaskUser /disable");
        assert_eq!(c[1], "schtasks /change /tn \\GoogleUpdateTaskUser /enable");
    }

    #[test]
    fn delete_registry_entry_keeps_backup() {
        let d = tmp("del");
        let sys = FakeSys::default();
        sys.add(RUN_USER, "Spotify", "REG_SZ", "C:\\Spotify.exe");
        let env = TestEnv(HashMap::new());
        let e = list(&sys, &env).remove(0);
        let b = delete(&sys, &e, &d).unwrap();
        assert!(b.exists());
        assert!(sys.get(RUN_USER, "Spotify").is_none());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn delete_folder_entry_moves_the_file() {
        let d = tmp("delf");
        let f = d.join("Mio.lnk");
        fs::write(&f, b"ciao").unwrap();
        let e = Entry {
            name: "Mio.lnk".into(),
            command: f.to_string_lossy().to_string(),
            source: Source::Folder,
            origin: String::new(),
            enabled: true,
            location: f.to_string_lossy().to_string(),
            approved_key: APPR_USER_FOLDER.into(),
            system: false,
            needs_admin: false,
        };
        let backup = d.join("backup");
        let b = delete(&FakeSys::default(), &e, &backup).unwrap();
        assert!(!f.exists());
        assert_eq!(fs::read(&b).unwrap(), b"ciao");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn delete_task_saves_xml_first() {
        let d = tmp("delt");
        let sys = FakeSys::default();
        let e = parse_tasks(TASKS).remove(0);
        let b = delete(&sys, &e, &d).unwrap();
        assert!(b.exists());
        let c = sys.commands.borrow().clone();
        assert!(c.last().unwrap().contains("/delete"));
        let _ = fs::remove_dir_all(&d);
    }
}
