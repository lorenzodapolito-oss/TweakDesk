//! Applica le voci scelte: app, registro, servizi, attività, funzionalità, file. Funziona sul sistema in uso e sulle immagini.

use crate::clean::sys::Sys;
use crate::debloat::catalog::{Act, Group, Item, RegKind};
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
    /// Etichetta di ogni voce applicata con successo (anche se "in parte", con avviso).
    pub done_items: Vec<String>,
    /// Etichetta di ogni voce saltata perché non c'era niente da fare.
    pub skipped_items: Vec<String>,
    /// Etichetta e motivo di ogni voce non riuscita.
    pub failed_items: Vec<String>,
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
    /// Riattiva un'attività pianificata che prima era attiva.
    EnableTask {
        path: String,
    },
    /// Rimette il piano energetico che prima era attivo.
    PowerScheme {
        guid: String,
    },
}

/// Elenco delle modifiche annullabili. Ogni voce viene salvata subito, prima di
/// cambiare davvero il sistema: se il programma si chiude a metà, l'annullamento c'è.
struct Journal<'a> {
    steps: Vec<Undo>,
    save: &'a mut dyn FnMut(&[Undo]),
}

impl Journal<'_> {
    fn record(&mut self, u: Undo) {
        self.steps.push(u);
        (self.save)(&self.steps);
    }
}

/// Gli script scrivono `TD:<n>` (quante cose hanno trovato e cambiato) ed
/// `ERR:<messaggio>` se qualcosa non è riuscito.
fn found(out: &str) -> Option<u32> {
    out.lines()
        .find_map(|l| l.trim().strip_prefix("TD:")?.trim().parse().ok())
}

/// `false` solo se lo script dice di non aver trovato nulla da cambiare.
/// Senza risposta chiara conta come fatto.
fn did_something(out: &str) -> bool {
    found(out).is_none_or(|n| n > 0)
}

fn script_error(out: &str) -> Option<String> {
    out.lines()
        .find_map(|l| l.trim().strip_prefix("ERR:").map(|e| e.trim().to_string()))
}

fn is_guid(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == 5
        && [8, 4, 4, 4, 12]
            .iter()
            .zip(&parts)
            .all(|(n, p)| p.len() == *n && p.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Estrae il GUID dall'output di `powercfg /getactivescheme` (il nome è tradotto, il GUID no).
pub fn parse_scheme_guid(text: &str) -> Option<String> {
    text.split_whitespace()
        .find(|w| is_guid(w))
        .map(str::to_string)
}

fn active_scheme(sys: &dyn Sys) -> Option<String> {
    let out = sys.run("powercfg", &["/getactivescheme"]).ok()?;
    parse_scheme_guid(&out)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TaskState {
    Missing,
    Disabled,
    Enabled,
    Unknown,
}

fn task_state(sys: &dyn Sys, path: &str) -> TaskState {
    let (dir, name) = match path.rsplit_once('\\') {
        Some((d, n)) => (format!("{d}\\"), n.to_string()),
        None => ("\\".to_string(), path.to_string()),
    };
    let script = format!(
        "$t = Get-ScheduledTask -TaskPath {} -TaskName {} -ErrorAction SilentlyContinue; if ($t) {{ Write-Output ('TD:' + $t.State) }} else {{ Write-Output 'TD:Missing' }}",
        ps_quote(&dir),
        ps_quote(&name)
    );
    match run_ps(sys, &script) {
        Ok(out) if out.contains("TD:Missing") => TaskState::Missing,
        Ok(out) if out.contains("TD:Disabled") => TaskState::Disabled,
        Ok(out) if out.contains("TD:") => TaskState::Enabled,
        _ => TaskState::Unknown,
    }
}

fn disable_task(sys: &dyn Sys, path: &str, undo: &mut Journal) -> Result<bool, String> {
    match task_state(sys, path) {
        TaskState::Missing | TaskState::Disabled => return Ok(false),
        TaskState::Enabled => undo.record(Undo::EnableTask {
            path: path.to_string(),
        }),
        TaskState::Unknown => {}
    }
    sys.run("schtasks", &["/change", "/tn", path, "/disable"])
        .map(|_| true)
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

fn remove_appx(sys: &dyn Sys, t: &Target, prefixes: &[&str]) -> Result<bool, String> {
    let arr = names_array(prefixes);
    let script = match &t.mount {
        Some(m) => {
            let m = ps_quote(&m.to_string_lossy());
            format!(
                "$n = 0; $err = ''; foreach($p in {arr}) {{ $v = @(Get-AppxProvisionedPackage -Path {m} | Where-Object {{ $_.DisplayName -like ($p + '*') }}); $n += $v.Count; foreach($x in $v) {{ try {{ Remove-AppxProvisionedPackage -Path {m} -PackageName $x.PackageName -ErrorAction Stop | Out-Null }} catch {{ $err = $_.Exception.Message }} }} }}; Write-Output ('TD:' + $n); if ($err) {{ Write-Output ('ERR:' + $err) }}"
            )
        }
        None => format!(
            "$n = 0; $err = ''; foreach($p in {arr}) {{ $a = @(Get-AppxPackage -AllUsers -Name ($p + '*')); $v = @(Get-AppxProvisionedPackage -Online | Where-Object {{ $_.DisplayName -like ($p + '*') }}); $n += $a.Count + $v.Count; foreach($x in $a) {{ try {{ Remove-AppxPackage -Package $x.PackageFullName -AllUsers -ErrorAction Stop }} catch {{ $err = $_.Exception.Message }} }}; foreach($x in $v) {{ try {{ Remove-AppxProvisionedPackage -Online -PackageName $x.PackageName -ErrorAction Stop | Out-Null }} catch {{ $err = $_.Exception.Message }} }} }}; Write-Output ('TD:' + $n); if ($err) {{ Write-Output ('ERR:' + $err) }}"
        ),
    };
    let out = run_ps(sys, &script)?;
    match script_error(&out) {
        Some(e) => Err(e),
        None => Ok(did_something(&out)),
    }
}

fn disable_feature(sys: &dyn Sys, t: &Target, name: &str) -> Result<bool, String> {
    let ta = target_arg(t);
    let remove = if t.is_offline() { " -Remove" } else { "" };
    let n = ps_quote(name);
    let script = format!(
        "$f = Get-WindowsOptionalFeature {ta} -FeatureName {n} -ErrorAction SilentlyContinue; if ($f -and $f.State -notlike 'Disabled*') {{ Disable-WindowsOptionalFeature {ta} -FeatureName {n}{remove} -NoRestart -ErrorAction Stop | Out-Null; Write-Output 'TD:1' }} else {{ Write-Output 'TD:0' }}"
    );
    run_ps(sys, &script).map(|o| did_something(&o))
}

fn remove_capability(sys: &dyn Sys, t: &Target, prefix: &str) -> Result<bool, String> {
    let ta = target_arg(t);
    let p = ps_quote(prefix);
    let script = format!(
        "$c = @(Get-WindowsCapability {ta} | Where-Object {{ $_.Name -like ({p} + '*') -and $_.State -eq 'Installed' }}); foreach($x in $c) {{ Remove-WindowsCapability {ta} -Name $x.Name -ErrorAction Stop | Out-Null }}; Write-Output ('TD:' + $c.Count)"
    );
    run_ps(sys, &script).map(|o| did_something(&o))
}

fn delete_path(sys: &dyn Sys, path: &Path) -> Result<bool, String> {
    let p = ps_quote(&path.to_string_lossy());
    let script = format!(
        "$p = {p}; if (Test-Path -Path $p) {{ icacls $p /setowner *S-1-5-32-544 /t /c /q | Out-Null; icacls $p /grant *S-1-5-32-544:F /t /c /q | Out-Null; Remove-Item -Path $p -Recurse -Force -ErrorAction SilentlyContinue; Write-Output 'TD:1' }} else {{ Write-Output 'TD:0' }}"
    );
    run_ps(sys, &script).map(|o| did_something(&o))
}

/// Esegue una singola azione.
fn run_act(act: &Act, t: &Target, sys: &dyn Sys, undo: &mut Journal) -> Result<bool, String> {
    match act {
        Act::Appx(p) => remove_appx(sys, t, &[p]),
        Act::Feature(f) => disable_feature(sys, t, f),
        Act::Capability(c) => remove_capability(sys, t, c),
        Act::Reg {
            key,
            name,
            kind,
            data,
        } => {
            let k = t.map_key(key)?;
            if !t.is_offline() {
                if let Some(u) = prior(sys, &k, name) {
                    undo.record(u);
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
                    undo.record(u);
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
                    undo.record(u);
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
                delete_path(sys, &p)
            } else {
                disable_task(sys, path, undo)
            }
        }
        Act::Delete(rel) => {
            let p =
                safe_join(&t.windows, rel).ok_or_else(|| format!("percorso non valido: {rel}"))?;
            delete_path(sys, &p)
        }
        Act::Cmd(prog, args) => {
            if t.is_offline() {
                return Ok(false);
            }
            if *prog == "powercfg" && args.first() == Some(&"/setactive") {
                if let Some(guid) = active_scheme(sys) {
                    undo.record(Undo::PowerScheme { guid });
                }
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

/// Cosa resta da fare all'utente dopo aver applicato le voci.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Followup {
    /// Alcune modifiche all'interfaccia si vedono solo riavviando Esplora risorse.
    pub explorer: bool,
    /// Servizi e funzionalità cambiano solo dopo il riavvio del PC.
    pub reboot: bool,
}

pub fn followup(items: &[&Item]) -> Followup {
    Followup {
        explorer: items.iter().any(|i| i.group == Group::Interfaccia),
        reboot: items.iter().any(|i| {
            i.acts
                .iter()
                .any(|a| matches!(a, Act::Svc { .. } | Act::Feature(_) | Act::Capability(_)))
        }),
    }
}

impl Followup {
    pub fn message(self) -> Option<&'static str> {
        match (self.explorer, self.reboot) {
            (false, false) => None,
            (true, false) => Some("Per vedere le modifiche all'interfaccia riavvia Esplora risorse (pulsante «Riavvia Esplora risorse») oppure esci e rientra in Windows."),
            (false, true) => Some("Servizi e funzionalità cambiano solo dopo il riavvio del PC."),
            (true, true) => Some("Riavvia Esplora risorse (pulsante «Riavvia Esplora risorse») per l'interfaccia e poi il PC: servizi e funzionalità cambiano solo dopo il riavvio."),
        }
    }
}

/// Chiude Esplora risorse: Windows lo riapre da solo.
pub fn restart_explorer(sys: &dyn Sys) -> Result<(), String> {
    run_ps(sys, "Stop-Process -Name explorer -Force").map(|_| ())
}

/// Applica le voci. Restituisce il resoconto e le istruzioni per annullare.
pub fn apply(
    items: &[&Item],
    t: &Target,
    sys: &dyn Sys,
    log: &mut dyn FnMut(Kind, &str),
) -> (Report, Vec<Undo>) {
    apply_with_progress(items, t, sys, log, &mut |_, _, _| {})
}

/// Come `apply`, e chiama `progress(fatte, totali, voce_in_corso)` prima di ogni voce (con la
/// sua etichetta) e alla fine (con etichetta vuota: non c'è più una voce "in corso").
pub fn apply_with_progress(
    items: &[&Item],
    t: &Target,
    sys: &dyn Sys,
    log: &mut dyn FnMut(Kind, &str),
    progress: &mut dyn FnMut(usize, usize, &str),
) -> (Report, Vec<Undo>) {
    apply_full(items, t, sys, log, progress, &mut |_| {})
}

/// Come `apply_with_progress`, e chiama `save(modifiche_annullabili)` ogni volta che ne
/// aggiunge una, PRIMA di cambiare il sistema: chi chiama può scrivere subito il file «Annulla».
pub fn apply_full(
    items: &[&Item],
    t: &Target,
    sys: &dyn Sys,
    log: &mut dyn FnMut(Kind, &str),
    progress: &mut dyn FnMut(usize, usize, &str),
    save: &mut dyn FnMut(&[Undo]),
) -> (Report, Vec<Undo>) {
    let mut rep = Report::default();
    let mut undo = Journal {
        steps: Vec::new(),
        save,
    };
    if let Err(e) = t.load_hives(sys) {
        log(Kind::Err, &e);
        rep.failed = items.len();
        return (rep, Vec::new());
    }
    let total = items.len();
    for (n, item) in items.iter().enumerate() {
        progress(n, total, item.label);
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
                rep.failed_items.push(format!("{}: {e}", item.label));
                log(Kind::Err, &format!("{}: {e}", item.label));
            }
            (true, Some(e)) => {
                rep.done += 1;
                rep.done_items
                    .push(format!("{} (con avviso: {e})", item.label));
                log(Kind::Warn, &format!("{}: fatto in parte ({e})", item.label));
            }
            (true, None) => {
                rep.done += 1;
                rep.done_items.push(item.label.to_string());
                log(Kind::Ok, &format!("{}: fatto", item.label));
            }
            (false, None) => {
                rep.skipped += 1;
                rep.skipped_items.push(item.label.to_string());
                log(Kind::Info, &format!("{}: niente da fare", item.label));
            }
        }
    }
    progress(total, total, "");
    t.unload_hives(sys);
    (rep, undo.steps)
}

/// Riporta indietro le modifiche: registro, servizi, attività pianificate e piano energetico.
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
            Undo::EnableTask { path } => sys
                .run("schtasks", &["/change", "/tn", path, "/enable"])
                .map(|_| ()),
            Undo::PowerScheme { guid } => sys.run("powercfg", &["/setactive", guid]).map(|_| ()),
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

fn kind_label(k: Kind) -> &'static str {
    match k {
        Kind::Info => "INFO",
        Kind::Ok => "OK",
        Kind::Warn => "ATTENZIONE",
        Kind::Err => "ERRORE",
    }
}

/// Il log come testo semplice, una riga per voce (`[OK] messaggio`): usato per salvarlo su
/// file dopo un'applicazione o la creazione di una ISO/install.wim, così resta anche dopo
/// aver chiuso il programma.
pub fn report_text(lines: &[(Kind, String)]) -> String {
    lines
        .iter()
        .map(|(k, m)| format!("[{}] {m}", kind_label(*k)))
        .collect::<Vec<_>>()
        .join("\r\n")
}

/// Resoconto finale leggibile: elenco delle voci riuscite, poi quelle saltate, poi quelle
/// fallite. Usato a fine applicazione (e a fine creazione ISO/install.wim) per mostrare subito
/// cosa è andato a buon fine e cosa no, senza dover scorrere tutto il log.
pub fn summary_text(rep: &Report) -> String {
    let mut out = String::new();
    out.push_str(&format!("Riuscite ({}):\r\n", rep.done_items.len()));
    if rep.done_items.is_empty() {
        out.push_str("  (nessuna)\r\n");
    } else {
        for i in &rep.done_items {
            out.push_str(&format!("  - {i}\r\n"));
        }
    }
    out.push_str(&format!(
        "\r\nSaltate, niente da fare ({}):\r\n",
        rep.skipped_items.len()
    ));
    if rep.skipped_items.is_empty() {
        out.push_str("  (nessuna)\r\n");
    } else {
        for i in &rep.skipped_items {
            out.push_str(&format!("  - {i}\r\n"));
        }
    }
    out.push_str(&format!("\r\nFallite ({}):\r\n", rep.failed_items.len()));
    if rep.failed_items.is_empty() {
        out.push_str("  (nessuna)\r\n");
    } else {
        for i in &rep.failed_items {
            out.push_str(&format!("  - {i}\r\n"));
        }
    }
    out
}

/// Salva il log su file (crea la cartella se non esiste, sovrascrive un log precedente con lo
/// stesso nome).
pub fn save_log(file: &Path, lines: &[(Kind, String)]) -> Result<(), String> {
    if let Some(dir) = file.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
    }
    std::fs::write(file, report_text(lines)).map_err(|e| format!("{}: {e}", file.display()))
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
    fn report_text_has_one_labelled_line_per_entry() {
        let v = vec![
            (Kind::Info, "inizio".to_string()),
            (Kind::Ok, "fatto".to_string()),
            (Kind::Warn, "fatto in parte".to_string()),
            (Kind::Err, "fallito".to_string()),
        ];
        assert_eq!(
            report_text(&v),
            "[INFO] inizio\r\n[OK] fatto\r\n[ATTENZIONE] fatto in parte\r\n[ERRORE] fallito"
        );
    }

    #[test]
    fn save_log_writes_the_file_creating_the_folder() {
        let d = std::env::temp_dir().join(format!("tdlog_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let file = d.join("sub").join("log.txt");
        save_log(&file, &[(Kind::Ok, "voce applicata".into())]).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("[OK] voce applicata"), "{text}");
        let _ = std::fs::remove_dir_all(&d);
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

    #[test]
    fn progress_is_reported_per_item_and_at_the_end() {
        let sys = FakeSys::default();
        let mut seen = Vec::new();
        apply_with_progress(
            &[item("ad_id"), item("ext")],
            &online(),
            &sys,
            &mut noop(),
            &mut |n, t, label| seen.push((n, t, label.to_string())),
        );
        assert_eq!(
            seen,
            vec![
                (0, 2, item("ad_id").label.to_string()),
                (1, 2, item("ext").label.to_string()),
                (2, 2, String::new()),
            ]
        );
    }

    #[test]
    fn report_item_lists_match_the_counts() {
        let sys = FakeSys::default();
        let (rep, _) = apply_with_progress(
            &[item("ad_id"), item("ext")],
            &online(),
            &sys,
            &mut noop(),
            &mut |_, _, _| {},
        );
        // Qualunque sia l'esito reale di ciascuna voce con FakeSys di default, ogni voce deve
        // finire in esattamente una delle tre liste, e il conteggio di ciascuna lista deve
        // corrispondere al contatore numerico gemello.
        assert_eq!(rep.done_items.len(), rep.done);
        assert_eq!(rep.skipped_items.len(), rep.skipped);
        assert_eq!(rep.failed_items.len(), rep.failed);
        assert_eq!(
            rep.done_items.len() + rep.skipped_items.len() + rep.failed_items.len(),
            2
        );
    }

    #[test]
    fn summary_text_lists_done_skipped_and_failed_with_counts() {
        let rep = Report {
            done: 1,
            skipped: 1,
            failed: 1,
            done_items: vec!["A".to_string()],
            skipped_items: vec!["B".to_string()],
            failed_items: vec!["C: motivo".to_string()],
        };
        let t = summary_text(&rep);
        assert!(t.contains("Riuscite (1)"));
        assert!(t.contains("- A"));
        assert!(t.contains("Saltate, niente da fare (1)"));
        assert!(t.contains("- B"));
        assert!(t.contains("Fallite (1)"));
        assert!(t.contains("- C: motivo"));
    }

    #[test]
    fn summary_text_says_none_for_empty_buckets() {
        let t = summary_text(&Report::default());
        assert_eq!(t.matches("(nessuna)").count(), 3);
    }

    fn reply(sys: &FakeSys, contains: &str, out: &str) {
        sys.responses
            .borrow_mut()
            .push((contains.to_string(), out.to_string()));
    }

    #[test]
    fn undo_is_saved_before_each_change() {
        use std::cell::{Cell, RefCell};
        use std::rc::Rc;
        let sys = FakeSys::default();
        let saved = Rc::new(Cell::new(0usize));
        let seen = Rc::new(RefCell::new(Vec::new()));
        {
            let (s, n) = (saved.clone(), seen.clone());
            sys.hooks.borrow_mut().push((
                "reg add".into(),
                Box::new(move |_| n.borrow_mut().push(s.get())),
            ));
        }
        let s2 = saved.clone();
        let (_, undo) = apply_full(
            &[item("ad_id"), item("ext")],
            &online(),
            &sys,
            &mut noop(),
            &mut |_, _, _| {},
            &mut |steps| s2.set(steps.len()),
        );
        // Al momento di ogni «reg add» l'annullamento era già stato salvato.
        assert_eq!(*seen.borrow(), vec![1, 2]);
        assert_eq!(saved.get(), undo.len());
    }

    #[test]
    fn enabled_tasks_are_disabled_and_can_be_enabled_again() {
        let sys = FakeSys::default();
        reply(&sys, "Get-ScheduledTask", "TD:Ready");
        let (rep, undo) = apply(&[item("tasks_telemetry")], &online(), &sys, &mut noop());
        assert_eq!(rep.done, 1);
        assert_eq!(undo.len(), 7);
        assert!(undo.iter().all(|u| matches!(u, Undo::EnableTask { .. })));
        let sys2 = FakeSys::default();
        assert_eq!(super::undo(&undo, &sys2).done, 7);
        let c = sys2.commands.borrow().clone();
        assert!(c.iter().all(|x| x.starts_with("schtasks /change /tn")));
        assert!(c.iter().all(|x| x.ends_with("/enable")));
    }

    #[test]
    fn missing_or_already_disabled_tasks_are_skipped() {
        for state in ["TD:Missing", "TD:Disabled"] {
            let sys = FakeSys::default();
            reply(&sys, "Get-ScheduledTask", state);
            let (rep, undo) = apply(&[item("tasks_telemetry")], &online(), &sys, &mut noop());
            assert_eq!((rep.done, rep.skipped), (0, 1), "{state}");
            assert!(undo.is_empty());
            assert!(sys
                .commands
                .borrow()
                .iter()
                .all(|c| !c.starts_with("schtasks")));
        }
    }

    #[test]
    fn task_path_is_split_into_folder_and_name() {
        let sys = FakeSys::default();
        reply(&sys, "Get-ScheduledTask", "TD:Missing");
        apply(&[item("tasks_telemetry")], &online(), &sys, &mut noop());
        let c = sys.commands.borrow().clone();
        assert!(c[0].contains(
            "-TaskPath '\\Microsoft\\Windows\\Application Experience\\' -TaskName 'Microsoft Compatibility Appraiser'"
        ), "{}", c[0]);
    }

    #[test]
    fn power_plan_is_restored_by_undo() {
        let sys = FakeSys::default();
        reply(
            &sys,
            "getactivescheme",
            "Power Scheme GUID: 381b4222-f694-41f0-9685-ff5bb260df2e  (Bilanciato)",
        );
        let (rep, undo) = apply(&[item("power")], &online(), &sys, &mut noop());
        assert_eq!(rep.done, 1);
        assert_eq!(
            undo,
            vec![Undo::PowerScheme {
                guid: "381b4222-f694-41f0-9685-ff5bb260df2e".into()
            }]
        );
        let sys2 = FakeSys::default();
        super::undo(&undo, &sys2);
        assert_eq!(
            sys2.commands.borrow()[0],
            "powercfg /setactive 381b4222-f694-41f0-9685-ff5bb260df2e"
        );
    }

    #[test]
    fn scheme_guid_parsing() {
        assert_eq!(
            parse_scheme_guid("GUID combinazione per il risparmio energia: 8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c  (Prestazioni elevate)"),
            Some("8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c".into())
        );
        assert_eq!(parse_scheme_guid("nessun guid qui"), None);
        assert_eq!(parse_scheme_guid("1234-5678"), None);
    }

    #[test]
    fn nothing_to_remove_is_reported_as_nothing_to_do() {
        for (id, probe) in [
            ("news", "Get-AppxPackage"),
            ("ps2", "Get-WindowsOptionalFeature"),
            ("quickassist", "Get-WindowsCapability"),
            ("clean_backup", "Test-Path"),
        ] {
            let sys = FakeSys::default();
            reply(&sys, probe, "TD:0");
            let (rep, _) = apply(&[item(id)], &online(), &sys, &mut noop());
            assert_eq!((rep.done, rep.skipped, rep.failed), (0, 1, 0), "{id}");
        }
    }

    #[test]
    fn something_removed_is_reported_as_done() {
        let sys = FakeSys::default();
        reply(&sys, "Get-AppxPackage", "TD:2");
        let (rep, _) = apply(&[item("news")], &online(), &sys, &mut noop());
        assert_eq!(rep.done, 1);
    }

    #[test]
    fn appx_errors_are_no_longer_hidden() {
        let sys = FakeSys::default();
        reply(&sys, "Get-AppxPackage", "TD:1\nERR:accesso negato");
        let mut lines = Vec::new();
        let (rep, _) = apply(&[item("news")], &online(), &sys, &mut |k, m| {
            lines.push((k, m.to_string()))
        });
        assert_eq!((rep.done, rep.failed), (0, 1));
        assert!(lines
            .iter()
            .any(|(k, m)| *k == Kind::Err && m.contains("accesso negato")));
        let sys = FakeSys::default();
        let c = {
            apply(&[item("news")], &online(), &sys, &mut noop());
            sys.commands.borrow().clone()
        };
        assert!(!c[0].contains("SilentlyContinue"), "{}", c[0]);
    }

    #[test]
    fn script_output_parsing() {
        assert_eq!(found("TD:3"), Some(3));
        assert_eq!(found("rumore\nTD:0\n"), Some(0));
        assert_eq!(found(""), None);
        assert!(did_something(""));
        assert!(did_something("TD:1"));
        assert!(!did_something("TD:0"));
        assert_eq!(script_error("TD:1\nERR: boom "), Some("boom".into()));
        assert_eq!(script_error("TD:1"), None);
    }

    #[test]
    fn new_undo_steps_roundtrip_and_old_files_still_load() {
        let d = std::env::temp_dir().join(format!("tdun2_{}", std::process::id()));
        let steps = vec![
            Undo::EnableTask {
                path: "\\A\\B".into(),
            },
            Undo::PowerScheme { guid: "x".into() },
        ];
        let f = d.join("b.json");
        save_undo(&steps, &f).unwrap();
        assert_eq!(load_undo(&f).unwrap(), steps);
        let old =
            r#"[{"DeleteValue":{"key":"HKCU\\A","name":"x"}},{"DeleteKey":{"key":"HKCU\\B"}}]"#;
        std::fs::write(&f, old).unwrap();
        assert_eq!(load_undo(&f).unwrap().len(), 2);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn followup_tells_what_is_needed() {
        assert_eq!(
            followup(&[item("ext")]),
            Followup {
                explorer: true,
                reboot: false
            }
        );
        assert_eq!(
            followup(&[item("svc_misc")]),
            Followup {
                explorer: false,
                reboot: true
            }
        );
        assert_eq!(
            followup(&[item("ext"), item("ie")]),
            Followup {
                explorer: true,
                reboot: true
            }
        );
        let none = followup(&[item("news"), item("ad_id")]);
        assert_eq!(none, Followup::default());
        assert!(none.message().is_none());
        assert!(followup(&[item("ext")])
            .message()
            .is_some_and(|m| m.contains("Esplora risorse")));
        assert!(followup(&[item("svc_misc")])
            .message()
            .is_some_and(|m| m.contains("riavvio del PC")));
    }

    #[test]
    fn explorer_restart_runs_and_reports_failure() {
        let sys = FakeSys::default();
        assert!(restart_explorer(&sys).is_ok());
        assert!(sys.commands.borrow()[0].contains("Stop-Process -Name explorer"));
        sys.failing.borrow_mut().push("powershell".into());
        assert!(restart_explorer(&sys).is_err());
    }

    #[test]
    fn dark_item_keeps_apps_light_and_windows_dark() {
        let sys = FakeSys::default();
        let (rep, _) = apply(&[item("dark")], &online(), &sys, &mut noop());
        assert_eq!(rep.done, 1);
        let all = sys.commands.borrow().join("\n");
        assert!(
            all.contains("SystemUsesLightTheme /t REG_DWORD /d 0"),
            "{all}"
        );
        assert!(all.contains("AppsUseLightTheme /t REG_DWORD /d 1"), "{all}");
    }
}
