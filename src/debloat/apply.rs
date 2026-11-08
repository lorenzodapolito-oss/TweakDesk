//! Applica le voci scelte: app, registro, servizi, attività, funzionalità, file. Funziona sul sistema in uso e sulle immagini.

use crate::clean::sys::Sys;
use crate::debloat::catalog::{Act, Item, RegKind};
use crate::debloat::target::{ps_quote, safe_join, Target};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Info,
    Ok,
    Warn,
    Err,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub done: usize,
    pub skipped: usize,
    pub failed: usize,
}

/// Come tornare indietro su una modifica di registro (solo sistema in uso).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Undo {
    Set {
        key: String,
        name: String,
        kind: String,
        data: String,
    },
    DeleteValue {
        key: String,
        name: String,
    },
    DeleteKey {
        key: String,
    },
}

fn kind_name(k: RegKind) -> &'static str {
    match k {
        RegKind::Dword => "REG_DWORD",
        RegKind::Sz => "REG_SZ",
    }
}

fn reg_args<'a>(key: &'a str, name: &'a str, kind: &'a str, data: &'a str) -> Vec<&'a str> {
    let mut v = vec!["add", key];
    if name.is_empty() {
        v.push("/ve");
    } else {
        v.extend(["/v", name]);
    }
    v.extend(["/t", kind, "/d", data, "/f"]);
    v
}

fn prior(sys: &dyn Sys, key: &str, name: &str) -> Option<Undo> {
    let items = sys.reg_query(key);
    if name.is_empty() {
        return items.is_empty().then(|| Undo::DeleteKey {
            key: key.to_string(),
        });
    }
    match items.iter().find(|i| i.name.eq_ignore_ascii_case(name)) {
        Some(i) => {
            let data = if i.kind == "REG_DWORD" {
                i.data
                    .strip_prefix("0x")
                    .and_then(|h| u64::from_str_radix(h, 16).ok())
                    .map_or_else(|| i.data.clone(), |n| n.to_string())
            } else {
                i.data.clone()
            };
            Some(Undo::Set {
                key: key.to_string(),
                name: name.to_string(),
                kind: i.kind.clone(),
                data,
            })
        }
        None => Some(Undo::DeleteValue {
            key: key.to_string(),
            name: name.to_string(),
        }),
    }
}

fn run_ps(sys: &dyn Sys, script: &str) -> Result<String, String> {
    sys.run(
        "powershell",
        &["-NoProfile", "-NonInteractive", "-Command", script],
    )
}

fn target_arg(t: &Target) -> String {
    match &t.mount {
        Some(m) => format!("-Path {}", ps_quote(&m.to_string_lossy())),
        None => "-Online".to_string(),
    }
}

fn names_array(prefixes: &[&str]) -> String {
    let q: Vec<String> = prefixes.iter().map(|p| ps_quote(p)).collect();
    format!("@({})", q.join(","))
}

fn remove_appx(sys: &dyn Sys, t: &Target, prefixes: &[&str]) -> Result<(), String> {
    let arr = names_array(prefixes);
    let script = match &t.mount {
        Some(m) => {
            let m = ps_quote(&m.to_string_lossy());
            format!(
                "foreach($p in {arr}) {{ Get-AppxProvisionedPackage -Path {m} | Where-Object {{ $_.DisplayName -like ($p + '*') }} | ForEach-Object {{ Remove-AppxProvisionedPackage -Path {m} -PackageName $_.PackageName | Out-Null }} }}"
            )
        }
        None => format!(
            "foreach($p in {arr}) {{ Get-AppxPackage -AllUsers -Name ($p + '*') | Remove-AppxPackage -AllUsers -ErrorAction SilentlyContinue; Get-AppxProvisionedPackage -Online | Where-Object {{ $_.DisplayName -like ($p + '*') }} | ForEach-Object {{ Remove-AppxProvisionedPackage -Online -PackageName $_.PackageName | Out-Null }} }}"
        ),
    };
    run_ps(sys, &script).map(|_| ())
}

fn disable_feature(sys: &dyn Sys, t: &Target, name: &str) -> Result<(), String> {
    let ta = target_arg(t);
    let remove = if t.is_offline() { " -Remove" } else { "" };
    let n = ps_quote(name);
    let script = format!(
        "$f = Get-WindowsOptionalFeature {ta} -FeatureName {n} -ErrorAction SilentlyContinue; if ($f -and $f.State -ne 'Disabled') {{ Disable-WindowsOptionalFeature {ta} -FeatureName {n}{remove} -NoRestart | Out-Null }}"
    );
    run_ps(sys, &script).map(|_| ())
}

fn remove_capability(sys: &dyn Sys, t: &Target, prefix: &str) -> Result<(), String> {
    let ta = target_arg(t);
    let p = ps_quote(prefix);
    let script = format!(
        "Get-WindowsCapability {ta} | Where-Object {{ $_.Name -like ({p} + '*') -and $_.State -eq 'Installed' }} | ForEach-Object {{ Remove-WindowsCapability {ta} -Name $_.Name | Out-Null }}"
    );
    run_ps(sys, &script).map(|_| ())
}

fn delete_path(sys: &dyn Sys, path: &Path) -> Result<(), String> {
    let p = ps_quote(&path.to_string_lossy());
    let script = format!(
        "$p = {p}; if (Test-Path -Path $p) {{ icacls $p /setowner *S-1-5-32-544 /t /c /q | Out-Null; icacls $p /grant *S-1-5-32-544:F /t /c /q | Out-Null; Remove-Item -Path $p -Recurse -Force -ErrorAction SilentlyContinue }}"
    );
    run_ps(sys, &script).map(|_| ())
}

/// Esegue una singola azione.
fn run_act(act: &Act, t: &Target, sys: &dyn Sys, undo: &mut Vec<Undo>) -> Result<bool, String> {
    match act {
        Act::Appx(p) => remove_appx(sys, t, &[p]).map(|()| true),
        Act::Feature(f) => disable_feature(sys, t, f).map(|()| true),
        Act::Capability(c) => remove_capability(sys, t, c).map(|()| true),
        Act::Reg {
            key,
            name,
            kind,
            data,
        } => {
            let k = t.map_key(key)?;
            if !t.is_offline() {
                if let Some(u) = prior(sys, &k, name) {
                    undo.push(u);
                }
            }
            sys.run("reg", &reg_args(&k, name, kind_name(*kind), data))
                .map(|_| true)
        }
        Act::RegDel { key, name } => {
            let k = t.map_key(key)?;
            if sys
                .reg_query(&k)
                .iter()
                .all(|i| !i.name.eq_ignore_ascii_case(name))
            {
                return Ok(false);
            }
            if !t.is_offline() {
                if let Some(u) = prior(sys, &k, name) {
                    undo.push(u);
                }
            }
            sys.reg_delete_value(&k, name).map(|()| true)
        }
        Act::Svc { name, start } => {
            let k = t.map_key(&format!(
                "HKLM\\SYSTEM\\CurrentControlSet\\Services\\{name}"
            ))?;
            if sys.reg_query(&k).is_empty() {
                return Ok(false);
            }
            if !t.is_offline() {
                if let Some(u) = prior(sys, &k, "Start") {
                    undo.push(u);
                }
            }
            sys.run(
                "reg",
                &reg_args(&k, "Start", "REG_DWORD", &start.to_string()),
            )
            .map(|_| true)
        }
        Act::Task(path) => {
            if t.is_offline() {
                let p = safe_join(&t.windows, &format!("System32\\Tasks{path}"))
                    .ok_or("percorso attività non valido")?;
                delete_path(sys, &p).map(|()| true)
            } else {
                sys.run("schtasks", &["/change", "/tn", path, "/disable"])
                    .map(|_| true)
            }
        }
        Act::Delete(rel) => {
            let p =
                safe_join(&t.windows, rel).ok_or_else(|| format!("percorso non valido: {rel}"))?;
            delete_path(sys, &p).map(|()| true)
        }
        Act::Cmd(prog, args) => {
            if t.is_offline() {
                return Ok(false);
            }
            sys.run(prog, args).map(|_| true)
        }
        Act::ResetBase => match &t.mount {
            Some(m) => sys
                .run(
                    "dism",
                    &[
                        &format!("/Image:{}", m.display()),
                        "/Cleanup-Image",
                        "/StartComponentCleanup",
                        "/ResetBase",
                    ],
                )
                .map(|_| true),
            None => Ok(false),
        },
    }
}

/// Applica le voci. Restituisce il resoconto e le istruzioni per annullare (registro).
pub fn apply(
    items: &[&Item],
    t: &Target,
    sys: &dyn Sys,
    log: &mut dyn FnMut(Kind, &str),
) -> (Report, Vec<Undo>) {
    let mut rep = Report::default();
    let mut undo = Vec::new();
    if let Err(e) = t.load_hives(sys) {
        log(Kind::Err, &e);
        rep.failed = items.len();
        return (rep, undo);
    }
    for item in items {
        let mut applied = false;
        let mut failed: Option<String> = None;
        for act in item.acts {
            match run_act(act, t, sys, &mut undo) {
                Ok(true) => applied = true,
                Ok(false) => {}
                Err(e) => failed = Some(e),
            }
        }
        match (applied, failed) {
            (false, Some(e)) => {
                rep.failed += 1;
                log(Kind::Err, &format!("{}: {e}", item.label));
            }
            (true, Some(e)) => {
                rep.done += 1;
                log(Kind::Warn, &format!("{}: fatto in parte ({e})", item.label));
            }
            (true, None) => {
                rep.done += 1;
                log(Kind::Ok, &format!("{}: fatto", item.label));
            }
            (false, None) => {
                rep.skipped += 1;
                log(Kind::Info, &format!("{}: niente da fare", item.label));
            }
        }
    }
    t.unload_hives(sys);
    (rep, undo)
}

/// Riporta il registro com'era prima.
pub fn undo(steps: &[Undo], sys: &dyn Sys) -> Report {
    let mut rep = Report::default();
    for s in steps.iter().rev() {
        let r = match s {
            Undo::Set {
                key,
                name,
                kind,
                data,
            } => sys.run("reg", &reg_args(key, name, kind, data)).map(|_| ()),
            Undo::DeleteValue { key, name } => sys.reg_delete_value(key, name),
            Undo::DeleteKey { key } => sys.run("reg", &["delete", key, "/f"]).map(|_| ()),
        };
        if r.is_ok() {
            rep.done += 1;
        } else {
            rep.failed += 1;
        }
    }
    rep
}

/// Compatta i rami di registro di un'immagine montata (risparmio piccolo, facoltativo).
pub fn compact_hives(t: &Target, sys: &dyn Sys, work: &Path) -> Result<usize, String> {
    t.load_hives(sys)?;
    let mut saved = Vec::new();
    let mut err = None;
    for (name, file) in t.hive_files() {
        let tmp = work.join(format!("{name}.compatto"));
        match sys.run(
            "reg",
            &[
                "save",
                &format!("HKLM\\{name}"),
                &tmp.to_string_lossy(),
                "/y",
            ],
        ) {
            Ok(_) => saved.push((tmp, file)),
            Err(e) => err = Some(e),
        }
    }
    t.unload_hives(sys);
    if let Some(e) = err {
        return Err(e);
    }
    let mut n = 0;
    for (tmp, file) in saved {
        std::fs::copy(&tmp, &file).map_err(|e| format!("{}: {e}", file.display()))?;
        let _ = std::fs::remove_file(&tmp);
        n += 1;
    }
    Ok(n)
}

pub fn save_undo(steps: &[Undo], file: &Path) -> Result<(), String> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let json = serde_json::to_string_pretty(steps).map_err(|e| e.to_string())?;
    std::fs::write(file, json).map_err(|e| format!("{}: {e}", file.display()))
}

pub fn load_undo(file: &Path) -> Result<Vec<Undo>, String> {
    let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", file.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clean::sys::fake::FakeSys;
    use crate::debloat::catalog::ITEMS;
    use std::path::PathBuf;

    fn item(id: &str) -> &'static Item {
        ITEMS.iter().find(|i| i.id == id).unwrap()
    }
    fn online() -> Target {
        Target::online(PathBuf::from("C:\\Windows"))
    }
    fn noop() -> impl FnMut(Kind, &str) {
        |_, _| {}
    }

    #[test]
    fn registry_item_online_records_undo() {
        let sys = FakeSys::default();
        sys.add(
            "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\AdvertisingInfo",
            "Enabled",
            "REG_DWORD",
            "0x1",
        );
        let (rep, undo) = apply(&[item("ad_id")], &online(), &sys, &mut noop());
        assert_eq!(rep.done, 1);
        let cmds = sys.commands.borrow().clone();
        assert!(cmds[0].contains("reg add HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\AdvertisingInfo /v Enabled /t REG_DWORD /d 0 /f"));
        assert_eq!(undo.len(), 1);
        assert!(matches!(&undo[0], Undo::Set { data, .. } if data == "1"));
    }

    #[test]
    fn new_value_undo_is_delete() {
        let sys = FakeSys::default();
        let (_, undo) = apply(&[item("ext")], &online(), &sys, &mut noop());
        assert!(matches!(&undo[0], Undo::DeleteValue { name, .. } if name == "HideFileExt"));
        let r = super::undo(&undo, &sys);
        assert_eq!(r.failed, 0);
    }

    #[test]
    fn default_value_item_uses_ve_and_undo_deletes_key() {
        let sys = FakeSys::default();
        let (_, undo) = apply(&[item("ctx")], &online(), &sys, &mut noop());
        let c = sys.commands.borrow().clone();
        assert!(c[0].contains("/ve /t REG_SZ /d  /f"));
        assert!(matches!(&undo[0], Undo::DeleteKey { .. }));
    }

    #[test]
    fn services_missing_are_skipped_present_are_changed() {
        let sys = FakeSys::default();
        sys.add(
            "HKLM\\SYSTEM\\CurrentControlSet\\Services\\Fax",
            "Start",
            "REG_DWORD",
            "0x3",
        );
        let (rep, undo) = apply(&[item("svc_misc")], &online(), &sys, &mut noop());
        assert_eq!(rep.done, 1);
        let c = sys.commands.borrow().clone();
        assert_eq!(c.len(), 1);
        assert!(c[0].contains("Services\\Fax /v Start /t REG_DWORD /d 4 /f"));
        assert!(matches!(&undo[0], Undo::Set { data, .. } if data == "3"));
        let (rep, _) = apply(
            &[item("svc_xbox")],
            &online(),
            &FakeSys::default(),
            &mut noop(),
        );
        assert_eq!((rep.done, rep.skipped), (0, 1));
    }

    #[test]
    fn offline_registry_goes_through_loaded_hives() {
        let sys = FakeSys::default();
        let t = Target::offline(Path::new("D:\\mnt"));
        let (rep, undo) = apply(&[item("telemetry")], &t, &sys, &mut noop());
        assert_eq!(rep.done, 1);
        assert!(undo.is_empty(), "offline non registra l'annullamento");
        let c = sys.commands.borrow().clone();
        assert!(c[0].starts_with("reg load HKLM\\zTD_SOFTWARE"));
        assert!(c.iter().any(|x| x.contains("HKLM\\zTD_SOFTWARE\\SOFTWARE")
            || x.contains("HKLM\\zTD_SOFTWARE\\Policies")));
        assert!(c.last().unwrap().starts_with("reg unload"));
    }

    #[test]
    fn offline_hive_failure_does_not_touch_anything() {
        let sys = FakeSys::default();
        sys.failing.borrow_mut().push("reg load".into());
        let t = Target::offline(Path::new("D:\\mnt"));
        let (rep, _) = apply(&[item("telemetry"), item("ext")], &t, &sys, &mut noop());
        assert_eq!(rep.failed, 2);
        assert!(sys
            .commands
            .borrow()
            .iter()
            .all(|c| !c.starts_with("reg add")));
    }

    #[test]
    fn apps_features_and_capabilities_use_powershell() {
        let sys = FakeSys::default();
        apply(
            &[item("news"), item("ie"), item("recall")],
            &online(),
            &sys,
            &mut noop(),
        );
        let c = sys.commands.borrow().clone();
        assert!(c[0].contains("Get-AppxPackage -AllUsers") && c[0].contains("Microsoft.BingNews"));
        assert!(c.iter().any(|x| x.contains("Get-WindowsCapability -Online")
            && x.contains("Browser.InternetExplorer")));
        assert!(c.iter().any(
            |x| x.contains("Disable-WindowsOptionalFeature -Online") && x.contains("'Recall'")
        ));
        let sys = FakeSys::default();
        let t = Target::offline(Path::new("D:\\mnt"));
        apply(&[item("news"), item("hyperv")], &t, &sys, &mut noop());
        let c = sys.commands.borrow().clone();
        assert!(c
            .iter()
            .any(|x| x.contains("Get-AppxProvisionedPackage -Path 'D:\\mnt'")));
        assert!(c
            .iter()
            .any(|x| x.contains("-Path 'D:\\mnt'") && x.contains("-Remove -NoRestart")));
    }

    #[test]
    fn delete_goes_through_safe_join_with_ownership() {
        let sys = FakeSys::default();
        let t = Target::offline(Path::new("/mnt/img"));
        apply(&[item("clean_files")], &t, &sys, &mut noop());
        let c = sys.commands.borrow().clone();
        let d = c.iter().find(|x| x.contains("Remove-Item")).unwrap();
        assert!(d.contains("icacls") && d.contains("S-1-5-32-544"));
        assert!(d.contains("/mnt/img/Windows/Logs/*"));
    }

    #[test]
    fn offline_tasks_are_deleted_online_are_disabled() {
        let sys = FakeSys::default();
        apply(&[item("tasks_telemetry")], &online(), &sys, &mut noop());
        assert!(sys
            .commands
            .borrow()
            .iter()
            .any(|c| c.starts_with("schtasks /change /tn")));
        let sys = FakeSys::default();
        let t = Target::offline(Path::new("/mnt/img"));
        apply(&[item("tasks_telemetry")], &t, &sys, &mut noop());
        let c = sys.commands.borrow().clone();
        assert!(c.iter().any(|x| x.contains("System32/Tasks")));
    }

    #[test]
    fn online_only_and_iso_only_acts() {
        let sys = FakeSys::default();
        let t = Target::offline(Path::new("D:\\mnt"));
        let (rep, _) = apply(&[item("power")], &t, &sys, &mut noop());
        assert_eq!(rep.skipped, 1);
        let (rep, _) = apply(
            &[item("resetbase")],
            &online(),
            &FakeSys::default(),
            &mut noop(),
        );
        assert_eq!(rep.skipped, 1);
        let sys = FakeSys::default();
        apply(&[item("resetbase")], &t, &sys, &mut noop());
        assert!(sys
            .commands
            .borrow()
            .iter()
            .any(|c| c.contains("/Image:D:\\mnt") && c.contains("/ResetBase")));
    }

    #[test]
    fn failures_are_reported_not_fatal() {
        let sys = FakeSys::default();
        sys.failing.borrow_mut().push("powershell".into());
        let (rep, _) = apply(&[item("news"), item("ext")], &online(), &sys, &mut noop());
        assert_eq!((rep.failed, rep.done), (1, 1));
    }

    #[test]
    fn every_catalog_item_runs_without_panicking_on_both_targets() {
        let all: Vec<&Item> = ITEMS.iter().collect();
        for t in [online(), Target::offline(Path::new("/mnt/img"))] {
            let sys = FakeSys::default();
            let (rep, _) = apply(&all, &t, &sys, &mut noop());
            assert_eq!(rep.done + rep.skipped + rep.failed, all.len());
        }
    }

    #[test]
    fn undo_file_roundtrip() {
        let d = std::env::temp_dir().join(format!("tdun_{}", std::process::id()));
        let steps = vec![
            Undo::DeleteValue {
                key: "HKCU\\A".into(),
                name: "x".into(),
            },
            Undo::DeleteKey {
                key: "HKCU\\B".into(),
            },
        ];
        let f = d.join("a.json");
        save_undo(&steps, &f).unwrap();
        assert_eq!(load_undo(&f).unwrap(), steps);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn compact_hives_replaces_files() {
        let d = std::env::temp_dir().join(format!("tdcmp_{}", std::process::id()));
        let mnt = d.join("mnt");
        for rel in ["Windows/System32/config", "Users/Default"] {
            std::fs::create_dir_all(mnt.join(rel)).unwrap();
        }
        for rel in [
            "Windows/System32/config/SOFTWARE",
            "Windows/System32/config/SYSTEM",
            "Users/Default/NTUSER.DAT",
        ] {
            std::fs::write(mnt.join(rel), b"old").unwrap();
        }
        let sys = FakeSys::default();
        let work = d.join("w");
        std::fs::create_dir_all(&work).unwrap();
        // `reg save` finto: crea i file compatti.
        for n in ["zTD_SOFTWARE", "zTD_SYSTEM", "zTD_NTUSER"] {
            std::fs::write(work.join(format!("{n}.compatto")), b"new").unwrap();
        }
        let t = Target::offline(&mnt);
        assert_eq!(compact_hives(&t, &sys, &work).unwrap(), 3);
        assert_eq!(
            std::fs::read(mnt.join("Windows/System32/config/SOFTWARE")).unwrap(),
            b"new"
        );
        let _ = std::fs::remove_dir_all(&d);
    }
}
