//! Crea la ISO a partire dallo zip di UUP dump: lo estrae in una cartella corta e semplice
//! ed esegue lo script `uup_download_windows.cmd` da lì (gli spazi e gli accenti lo rompono).

use crate::clean::sys::Sys;
use crate::debloat::apply::Kind;
use crate::debloat::netfix;
use crate::debloat::paths;
use crate::debloat::target::ps_quote;
use std::fs;
use std::path::{Path, PathBuf};

const SCRIPT: &str = "uup_download_windows.cmd";
const NEED_BYTES: u64 = 30_000_000_000;

fn ps(sys: &dyn Sys, script: &str) -> Result<String, String> {
    sys.run(
        "powershell",
        &[
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ],
    )
}

/// Cerca lo script nella cartella o in una sottocartella (alcuni zip ne hanno una).
pub fn find_script(dir: &Path) -> Option<PathBuf> {
    let direct = dir.join(SCRIPT);
    if direct.is_file() {
        return Some(direct);
    }
    fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.path().join(SCRIPT))
        .find(|p| p.is_file())
}

/// La ISO più grande (e più recente a parità) tra i file `.iso` della cartella.
pub fn find_iso(dir: &Path) -> Option<PathBuf> {
    fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|x| x.to_string_lossy().eq_ignore_ascii_case("iso"))
        })
        .filter_map(|e| Some((e.metadata().ok()?.len(), e.path())))
        .max_by_key(|(len, _)| *len)
        .map(|(_, p)| p)
}

/// Estrae lo zip ed esegue lo script; restituisce il percorso della ISO creata.
pub fn run(
    sys: &dyn Sys,
    zip: &Path,
    base: &Path,
    hosts: &Path,
    log: &mut dyn FnMut(Kind, &str),
) -> Result<PathBuf, String> {
    if !zip.is_file()
        || !zip
            .extension()
            .is_some_and(|x| x.to_string_lossy().eq_ignore_ascii_case("zip"))
    {
        return Err("Scegli lo zip scaricato da UUP dump".into());
    }
    if !sys.is_admin() {
        return Err(
            "Servono i permessi di amministratore (riavvia TweakDesk come amministratore)".into(),
        );
    }
    log(Kind::Info, "Controllo la rete (file hosts e DNS)…");
    netfix::preflight(sys, hosts, log)?;
    let root = paths::safe_root(base);
    if root != base {
        log(
            Kind::Warn,
            &format!(
                "La cartella scelta ha spazi, accenti o è troppo lunga: uso {}",
                root.display()
            ),
        );
    }
    let dir = root.join("UUP");
    paths::check_work(&dir)?;
    paths::check_space(sys, &root, NEED_BYTES)?;
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;

    log(Kind::Info, "Estraggo lo zip di UUP dump…");
    ps(
        sys,
        &format!(
            "Expand-Archive -LiteralPath {} -DestinationPath {} -Force",
            ps_quote(&zip.to_string_lossy()),
            ps_quote(&dir.to_string_lossy())
        ),
    )
    .map_err(|e| format!("Zip non estratto: {e}"))?;
    let script = find_script(&dir).ok_or_else(|| {
        format!("Nello zip manca {SCRIPT}: scegli «Scarica e converti in ISO» su UUP dump")
    })?;
    if let Ok(text) = fs::read_to_string(&script) {
        let (patched, n) = netfix::patch_retries(&text);
        if n > 0 && fs::write(&script, patched).is_ok() {
            log(
                Kind::Info,
                &format!(
                    "Script di UUP dump: limitati i tentativi a {} (di suo ripete all'infinito)",
                    netfix::MAX_TRIES
                ),
            );
        }
    }
    let sdir = script.parent().map(Path::to_path_buf).unwrap_or(dir);

    log(
        Kind::Info,
        "Avvio lo script di UUP dump: si apre una finestra, a fine lavoro premi un tasto. Può richiedere molto tempo…",
    );
    // L'uscita dello script non è affidabile (finisce con una pausa): conta solo la ISO prodotta.
    let _ = ps(
        sys,
        &format!(
            "Start-Process -FilePath 'cmd.exe' -ArgumentList '/c','{SCRIPT}' -WorkingDirectory {} -Wait",
            ps_quote(&sdir.to_string_lossy())
        ),
    );
    let iso = find_iso(&sdir).ok_or(
        "Lo script non ha creato nessuna ISO: guarda i messaggi nella sua finestra (antivirus, spazio, rete)",
    )?;
    log(Kind::Ok, &format!("ISO creata: {}", iso.display()));
    Ok(iso)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clean::sys::fake::FakeSys;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("tduup_{}_{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn admin() -> FakeSys {
        let s = FakeSys {
            admin: true,
            ..FakeSys::default()
        };
        s.responses
            .borrow_mut()
            .push(("GetHostAddresses".into(), "92.223.124.62\r\n".into()));
        s
    }

    fn no_hosts(d: &Path) -> PathBuf {
        d.join("hosts_assente")
    }

    #[test]
    fn finds_script_and_biggest_iso() {
        let d = tmp("find");
        assert!(find_script(&d).is_none());
        fs::create_dir_all(d.join("sub")).unwrap();
        fs::write(d.join("sub").join(SCRIPT), b"x").unwrap();
        assert_eq!(find_script(&d), Some(d.join("sub").join(SCRIPT)));
        fs::write(d.join("a.ISO"), b"12").unwrap();
        fs::write(d.join("b.iso"), b"12345").unwrap();
        fs::write(d.join("c.txt"), b"1234567").unwrap();
        assert_eq!(find_iso(&d), Some(d.join("b.iso")));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn rejects_bad_zip_and_non_admin() {
        let d = tmp("bad");
        let e = run(
            &admin(),
            &d.join("no.zip"),
            &d,
            &no_hosts(&d),
            &mut |_, _| {},
        )
        .unwrap_err();
        assert!(e.contains("zip"));
        fs::write(d.join("a.zip"), b"x").unwrap();
        let e = run(
            &FakeSys::default(),
            &d.join("a.zip"),
            &d,
            &no_hosts(&d),
            &mut |_, _| {},
        )
        .unwrap_err();
        assert!(e.contains("amministratore"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn extracts_runs_script_from_plain_folder_and_returns_iso() {
        let d = tmp("ok");
        fs::write(d.join("uup.zip"), b"x").unwrap();
        let sys = admin();
        let dir = d.join("UUP");
        let d2 = dir.clone();
        sys.hooks.borrow_mut().push((
            "Expand-Archive".into(),
            Box::new(move |_| {
                let _ = fs::create_dir_all(&d2);
                let _ = fs::write(d2.join(SCRIPT), b"@echo off");
            }),
        ));
        let d3 = dir.clone();
        sys.hooks.borrow_mut().push((
            "Start-Process".into(),
            Box::new(move |_| {
                let _ = fs::write(d3.join("26200.iso"), b"iso");
            }),
        ));
        let iso = run(&sys, &d.join("uup.zip"), &d, &no_hosts(&d), &mut |_, _| {}).unwrap();
        assert_eq!(iso, dir.join("26200.iso"));
        let c = sys.commands.borrow().clone();
        assert!(c.iter().any(|x| x.contains("Expand-Archive")));
        assert!(c
            .iter()
            .any(|x| x.contains("Start-Process") && x.contains("-WorkingDirectory")));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn missing_script_or_iso_is_an_error() {
        let d = tmp("err");
        fs::write(d.join("uup.zip"), b"x").unwrap();
        let sys = admin();
        let e = run(&sys, &d.join("uup.zip"), &d, &no_hosts(&d), &mut |_, _| {}).unwrap_err();
        assert!(e.contains(SCRIPT));
        let d2 = d.join("UUP");
        sys.hooks.borrow_mut().push((
            "Expand-Archive".into(),
            Box::new(move |_| {
                let _ = fs::create_dir_all(&d2);
                let _ = fs::write(d2.join(SCRIPT), b"x");
            }),
        ));
        let e = run(&sys, &d.join("uup.zip"), &d, &no_hosts(&d), &mut |_, _| {}).unwrap_err();
        assert!(e.contains("nessuna ISO"));
        let _ = fs::remove_dir_all(&d);
    }
}
