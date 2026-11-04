//! Generazione dei file .cmd per l'installazione silenziosa.

use crate::install::detect::Kind;
use std::path::Path;

fn crlf(lines: &[String]) -> String {
    let mut s = lines.join("\r\n");
    s.push_str("\r\n");
    s
}

/// Nome file sicuro per Windows.
pub fn safe_name(s: &str) -> String {
    let t: String = s
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let t = t.trim().trim_matches('.').to_string();
    if t.is_empty() {
        "programma".into()
    } else {
        t
    }
}

fn elevation() -> Vec<String> {
    vec![
        "net session >nul 2>&1".into(),
        "if %errorlevel% neq 0 (".into(),
        "    powershell -NoProfile -Command \"Start-Process -FilePath '%~f0' -Verb RunAs\"".into(),
        "    exit /b".into(),
        ")".into(),
    ]
}

/// Comando che lancia l'installer (percorso relativo alla cartella dello script).
pub fn install_command(file_name: &str, kind: Kind, args: &str) -> String {
    match kind {
        Kind::Msi => format!("msiexec /i \"%~dp0{file_name}\" {args}"),
        Kind::Msix => {
            format!("powershell -NoProfile -Command \"Add-AppxPackage -Path '%~dp0{file_name}'\"")
        }
        _ => format!("\"%~dp0{file_name}\" {args}"),
    }
}

/// Script per un singolo installer. Si auto-eleva e scrive il risultato in un log.
pub fn single(display: &str, file_name: &str, kind: Kind, args: &str) -> String {
    let mut l: Vec<String> = vec![
        "@echo off".into(),
        "chcp 65001 >nul".into(),
        "setlocal".into(),
    ];
    l.extend(elevation());
    l.push(format!("echo Installazione silenziosa: {display}"));
    l.push(install_command(file_name, kind, args));
    l.push("set \"ESITO=%errorlevel%\"".into());
    l.push(format!(
        "echo [%date% %time%] {display} - codice %ESITO%>>\"%~dp0installazioni.log\""
    ));
    l.push("if \"%ESITO%\"==\"0\" (echo Completato.) else if \"%ESITO%\"==\"3010\" (echo Completato, serve riavviare.) else (echo Errore %ESITO%)".into());
    l.push("exit /b %ESITO%".into());
    crlf(&l)
}

pub struct Job<'a> {
    pub display: &'a str,
    /// Percorso relativo alla cartella principale (es. `Git\Git-2.47.1.exe`).
    pub rel_path: &'a str,
    pub kind: Kind,
    pub args: &'a str,
}

/// Script che installa in sequenza tutti i programmi e riassume gli esiti.
pub fn install_all(jobs: &[Job<'_>]) -> String {
    let mut l: Vec<String> = vec![
        "@echo off".into(),
        "chcp 65001 >nul".into(),
        "setlocal enabledelayedexpansion".into(),
    ];
    l.extend(elevation());
    l.push("set OK=0".into());
    l.push("set KO=0".into());
    l.push(format!("echo Installazione di {} programmi...", jobs.len()));
    for j in jobs {
        l.push(String::new());
        l.push(format!("echo [%time%] {}", j.display));
        l.push(install_command(j.rel_path, j.kind, j.args));
        l.push("set \"E=!errorlevel!\"".into());
        l.push(format!(
            "echo [%date% %time%] {} - codice !E!>>\"%~dp0installazioni.log\"",
            j.display
        ));
        l.push("if \"!E!\"==\"0\" (set /a OK+=1) else if \"!E!\"==\"3010\" (set /a OK+=1) else (set /a KO+=1 & echo   ERRORE !E!)".into());
    }
    l.push(String::new());
    l.push("echo.".into());
    l.push("echo Riuscite: %OK%  Fallite: %KO%".into());
    l.push("pause".into());
    crlf(&l)
}

pub fn write(path: &Path, content: &str) -> Result<(), String> {
    std::fs::write(path, content).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_script_has_crlf_elevation_and_log() {
        let s = single("Git", "Git-2.47.1.exe", Kind::Inno, "/VERYSILENT");
        assert!(s.contains("\r\n") && !s.replace("\r\n", "").contains('\n'));
        assert!(s.contains("net session") && s.contains("RunAs"));
        assert!(s.contains("\"%~dp0Git-2.47.1.exe\" /VERYSILENT"));
        assert!(s.contains("installazioni.log"));
        assert!(s.ends_with("exit /b %ESITO%\r\n"));
    }

    #[test]
    fn msi_and_msix_use_their_own_launchers() {
        assert_eq!(
            install_command("a.msi", Kind::Msi, "/qn /norestart"),
            "msiexec /i \"%~dp0a.msi\" /qn /norestart"
        );
        assert!(install_command("a.msix", Kind::Msix, "").contains("Add-AppxPackage"));
    }

    #[test]
    fn install_all_lists_every_job_and_counts() {
        let jobs = [
            Job {
                display: "A",
                rel_path: "A\\a.exe",
                kind: Kind::Nsis,
                args: "/S",
            },
            Job {
                display: "B",
                rel_path: "B\\b.msi",
                kind: Kind::Msi,
                args: "/qn",
            },
        ];
        let s = install_all(&jobs);
        assert!(s.contains("\"%~dp0A\\a.exe\" /S"));
        assert!(s.contains("msiexec /i \"%~dp0B\\b.msi\" /qn"));
        assert!(s.contains("Installazione di 2 programmi"));
        assert!(s.contains("Riuscite: %OK%"));
    }

    #[test]
    fn safe_name_strips_forbidden_characters() {
        assert_eq!(safe_name("Notepad++"), "Notepad__");
        assert_eq!(safe_name("a/b:c"), "a_b_c");
        assert_eq!(safe_name("..."), "programma");
    }
}
