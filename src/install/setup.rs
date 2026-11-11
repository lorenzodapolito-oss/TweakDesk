//! Script di configurazione di Windows (incorporati) e modo di lanciarli.

use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Bat,
    Reg,
    Exe,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Impact {
    Normal,
    /// Cambia molte cose del sistema o non si annulla facilmente: chiede conferma.
    High,
}

pub struct Script {
    pub group: &'static str,
    pub label: &'static str,
    pub desc: &'static str,
    pub file: &'static str,
    pub kind: Kind,
    pub impact: Impact,
    /// Contenuto incorporato; `None` per i programmi di terzi che non si ridistribuiscono.
    pub bytes: Option<&'static [u8]>,
}

macro_rules! embed {
    ($f:literal) => {
        Some(include_bytes!(concat!("../../scripts/", $f)) as &'static [u8])
    };
}

/// Gruppo degli script di ottimizzazione: stanno nella scheda «Debloat e ottimizzazione».
pub const OPTIMIZE_GROUP: &str = "Ottimizzazione di sistema";

/// Quale parte degli script mostra un pannello.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Set {
    /// Configurazione (lingua, cartelle, Start, taskbar, extra…).
    Config,
    /// Ottimizzazione (Ottimizza Windows, Blocca Windows Update).
    Optimize,
}

pub fn in_set(s: &Script, set: Set) -> bool {
    (s.group == OPTIMIZE_GROUP) == (set == Set::Optimize)
}

pub const SCRIPTS: &[Script] = &[
    Script {
        group: "Configurazione di sistema",
        label: "MegaConfig Windows",
        desc: "Risparmio energetico, privacy, rete privata, tema, taskbar a sinistra. Disattiva anche il controllo account (UAC).",
        file: "MegaConfigWindows.bat",
        kind: Kind::Bat,
        impact: Impact::High,
        bytes: embed!("MegaConfigWindows.bat"),
    },
    Script {
        group: "Configurazione di sistema",
        label: "Italiano + orario",
        desc: "Installa la lingua italiana, imposta il fuso orario e la sincronizzazione NTP.",
        file: "Italiano.bat",
        kind: Kind::Bat,
        impact: Impact::Normal,
        bytes: embed!("Italiano.bat"),
    },
    Script {
        group: OPTIMIZE_GROUP,
        label: "Ottimizza Windows",
        desc: "Toglie Copilot, telemetria, pubblicità, app inutili e OneDrive; disattiva servizi non essenziali.",
        file: "Ottimizza.bat",
        kind: Kind::Bat,
        impact: Impact::High,
        bytes: embed!("Ottimizza.bat"),
    },
    Script {
        group: "Configurazione di sistema",
        label: "Attivazione Windows",
        desc: "Controlla e ripristina l'attivazione di Windows con gli strumenti di sistema.",
        file: "Attivazione.bat",
        kind: Kind::Bat,
        impact: Impact::Normal,
        bytes: embed!("Attivazione.bat"),
    },
    Script {
        group: OPTIMIZE_GROUP,
        label: "Blocca Windows Update",
        desc: "Disattiva Windows Update in modo aggressivo: servizi, attività, permessi, file di sistema rinominati, firewall e hosts.",
        file: "updateKill.bat",
        kind: Kind::Bat,
        impact: Impact::High,
        bytes: embed!("updateKill.bat"),
    },
    Script {
        group: "Cartelle utente",
        label: "Sposta cartelle su D:",
        desc: "Sposta Desktop, Documenti, Immagini, Musica, Video e Download su D:. I duplicati su C: vengono eliminati.",
        file: "SpostaCartelleUtente.bat",
        kind: Kind::Bat,
        impact: Impact::High,
        bytes: embed!("SpostaCartelleUtente.bat"),
    },
    Script {
        group: "Menu Start",
        label: "Esporta menu Start",
        desc: "Salva la configurazione del menu Start in D:\\StartBackup.",
        file: "EsportaStart.bat",
        kind: Kind::Bat,
        impact: Impact::Normal,
        bytes: embed!("EsportaStart.bat"),
    },
    Script {
        group: "Menu Start",
        label: "Importa menu Start",
        desc: "Ripristina il menu Start da D:\\StartBackup, sovrascrivendo quello attuale.",
        file: "ImportaStart.bat",
        kind: Kind::Bat,
        impact: Impact::High,
        bytes: embed!("ImportaStart.bat"),
    },
    Script {
        group: "Barra delle applicazioni",
        label: "Esporta taskbar",
        desc: "Salva la configurazione della taskbar in D:\\TaskbarBackup.",
        file: "EsportaTaskbar.bat",
        kind: Kind::Bat,
        impact: Impact::Normal,
        bytes: embed!("EsportaTaskbar.bat"),
    },
    Script {
        group: "Barra delle applicazioni",
        label: "Importa taskbar",
        desc: "Ripristina la taskbar da D:\\TaskbarBackup, sovrascrivendo quella attuale.",
        file: "ImportaTaskbar.bat",
        kind: Kind::Bat,
        impact: Impact::High,
        bytes: embed!("ImportaTaskbar.bat"),
    },
    Script {
        group: "Extra",
        label: "Gallery in Esplora file",
        desc: "Applica il file .reg che abilita la Galleria in Esplora file.",
        file: "Gallery_Enabler_v2.reg",
        kind: Kind::Reg,
        impact: Impact::Normal,
        bytes: embed!("Gallery_Enabler_v2.reg"),
    },
    Script {
        group: "Extra",
        label: "Menu contestuale classico (W11ClassicMenu.exe)",
        desc: "Avvia lo strumento di terzi per il menu contestuale classico. Il file non è incluso: mettilo nella cartella «Script» accanto a TweakDesk.",
        file: "W11ClassicMenu.exe",
        kind: Kind::Exe,
        impact: Impact::Normal,
        bytes: None,
    },
];

/// Scrive lo script nella cartella `dir` e ne restituisce il percorso. Per i file non incorporati
/// cerca `dir\<file>` già presente.
pub fn prepare(s: &Script, dir: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(s.file);
    match s.bytes {
        Some(b) => std::fs::write(&path, b).map_err(|e| format!("{}: {e}", path.display()))?,
        None if path.is_file() => {}
        None => {
            return Err(format!(
                "{} non trovato: copialo in {}",
                s.file,
                dir.display()
            ))
        }
    }
    Ok(path)
}

/// Programma e argomenti (grezzi, già con le virgolette) per lanciare lo script in una finestra
/// visibile. Con `admin` falso passa da PowerShell, che chiede l'autorizzazione di
/// amministratore.
pub fn launch_plan(kind: Kind, path: &Path, admin: bool) -> (&'static str, String) {
    let p = path.display();
    if admin {
        match kind {
            Kind::Bat => (
                "cmd",
                format!("/d /c start \"\" /wait cmd /d /c \"\"{p}\"\""),
            ),
            Kind::Reg => ("regedit", format!("/s \"{p}\"")),
            Kind::Exe => ("cmd", format!("/d /c start \"\" /wait \"{p}\"")),
        }
    } else {
        let q = p.to_string().replace('\'', "''");
        let inner = match kind {
            Kind::Bat => {
                format!("Start-Process cmd -ArgumentList '/d','/c','\"\"{q}\"\"' -Verb RunAs -Wait")
            }
            Kind::Reg => {
                format!("Start-Process regedit -ArgumentList '/s','\"{q}\"' -Verb RunAs -Wait")
            }
            Kind::Exe => format!("Start-Process '{q}' -Verb RunAs -Wait"),
        };
        ("powershell", format!("-NoProfile -Command \"{inner}\""))
    }
}

/// True se il programma gira con i privilegi di amministratore (solo Windows).
pub fn is_admin() -> bool {
    #[cfg(windows)]
    {
        use std::process::{Command, Stdio};
        Command::new("net")
            .arg("session")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Lancia lo script senza aspettarne la fine.
pub fn launch(kind: Kind, path: &Path, admin: bool) -> Result<(), String> {
    let (prog, args) = launch_plan(kind, path, admin);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new(prog)
            .raw_arg(args)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("{prog}: {e}"))
    }
    #[cfg(not(windows))]
    {
        Err(format!("solo Windows (comando: {prog} {args})"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_consistent() {
        let mut files = std::collections::BTreeSet::new();
        for s in SCRIPTS {
            assert!(files.insert(s.file), "duplicato {}", s.file);
            assert!(!s.file.contains(['/', '\\', ':']), "nome file sicuro");
            assert_eq!(
                s.kind,
                match s.file.rsplit('.').next() {
                    Some("bat") => Kind::Bat,
                    Some("reg") => Kind::Reg,
                    _ => Kind::Exe,
                }
            );
            if let Some(b) = s.bytes {
                assert!(b.len() > 20, "{} vuoto", s.file);
            }
        }
        assert_eq!(SCRIPTS.iter().filter(|s| s.bytes.is_none()).count(), 1);
    }

    #[test]
    fn dangerous_scripts_ask_for_confirmation() {
        for f in [
            "updateKill.bat",
            "SpostaCartelleUtente.bat",
            "MegaConfigWindows.bat",
            "Ottimizza.bat",
        ] {
            let s = SCRIPTS.iter().find(|s| s.file == f).unwrap();
            assert_eq!(s.impact, Impact::High, "{f}");
        }
    }

    #[test]
    fn prepare_writes_exact_bytes_and_reports_missing_third_party() {
        let dir = std::env::temp_dir().join(format!("idesk_setup_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let s = SCRIPTS.iter().find(|s| s.file == "Italiano.bat").unwrap();
        let p = prepare(s, &dir).unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), s.bytes.unwrap());
        let ext = SCRIPTS.iter().find(|s| s.bytes.is_none()).unwrap();
        assert!(prepare(ext, &dir).unwrap_err().contains("non trovato"));
        std::fs::write(dir.join(ext.file), b"MZ").unwrap();
        assert!(prepare(ext, &dir).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn launch_plans_quote_paths_with_spaces_and_elevate_when_needed() {
        let p = Path::new(r"C:\Users\Lo Re\Script\a.bat");
        let (prog, args) = launch_plan(Kind::Bat, p, true);
        assert_eq!(prog, "cmd");
        assert!(args.contains(r#"cmd /d /c ""C:\Users\Lo Re\Script\a.bat"""#));
        let (prog, args) = launch_plan(Kind::Bat, p, false);
        assert_eq!(prog, "powershell");
        assert!(args.contains("-Verb RunAs") && args.contains("Lo Re"));
        let (prog, args) = launch_plan(Kind::Reg, Path::new("x.reg"), true);
        assert_eq!((prog, args.as_str()), ("regedit", "/s \"x.reg\""));
        let (_, args) = launch_plan(Kind::Exe, Path::new("it's.exe"), false);
        assert!(
            args.contains("it''s.exe"),
            "apice raddoppiato per PowerShell"
        );
    }

    #[test]
    fn optimization_scripts_are_split_from_configuration() {
        let opt: Vec<_> = SCRIPTS
            .iter()
            .filter(|s| in_set(s, Set::Optimize))
            .map(|s| s.label)
            .collect();
        assert_eq!(opt, vec!["Ottimizza Windows", "Blocca Windows Update"]);
        let cfg = SCRIPTS.iter().filter(|s| in_set(s, Set::Config)).count();
        assert_eq!(cfg + opt.len(), SCRIPTS.len());
        assert!(SCRIPTS
            .iter()
            .all(|s| in_set(s, Set::Config) != in_set(s, Set::Optimize)));
    }
}
