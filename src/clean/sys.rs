//! Accesso al sistema (reg.exe, schtasks, comandi) dietro un trait, così la logica si prova senza Windows.

use regex::Regex;
use std::process::{Command, Stdio};

/// Un valore letto con `reg query`: nome, tipo (REG_SZ, REG_BINARY...) e dati.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegItem {
    pub name: String,
    pub kind: String,
    pub data: String,
}

pub trait Sys {
    /// Valori diretti della chiave (nessuna sottochiave). Chiave inesistente = elenco vuoto.
    fn reg_query(&self, key: &str) -> Vec<RegItem>;
    fn reg_set_binary(&self, key: &str, name: &str, hex: &str) -> Result<(), String>;
    fn reg_delete_value(&self, key: &str, name: &str) -> Result<(), String>;
    fn reg_export(&self, key: &str, file: &std::path::Path) -> Result<(), String>;
    /// Esegue un programma e restituisce l'output (errore se il codice di uscita non è 0).
    fn run(&self, program: &str, args: &[&str]) -> Result<String, String>;
    fn is_admin(&self) -> bool;
}

/// Legge l'output di `reg query <chiave>` (solo le righe dei valori, separate da 4 spazi).
pub fn parse_reg_query(text: &str) -> Vec<RegItem> {
    let Ok(re) = Regex::new(r"^\s{4}(.*?)\s{4}(REG_[A-Z_]+)(?:\s{4}(.*))?$") else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|l| {
            let c = re.captures(l.trim_end())?;
            Some(RegItem {
                name: c.get(1)?.as_str().to_string(),
                kind: c.get(2)?.as_str().to_string(),
                data: c.get(3).map_or(String::new(), |m| m.as_str().to_string()),
            })
        })
        .collect()
}

pub struct RealSys;

fn exec(program: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{program}: {e}"))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if out.status.success() {
        Ok(text)
    } else {
        Err(text.trim().to_string())
    }
}

impl Sys for RealSys {
    fn reg_query(&self, key: &str) -> Vec<RegItem> {
        exec("reg", &["query", key])
            .map(|t| parse_reg_query(&t))
            .unwrap_or_default()
    }

    fn reg_set_binary(&self, key: &str, name: &str, hex: &str) -> Result<(), String> {
        exec(
            "reg",
            &["add", key, "/v", name, "/t", "REG_BINARY", "/d", hex, "/f"],
        )
        .map(|_| ())
    }

    fn reg_delete_value(&self, key: &str, name: &str) -> Result<(), String> {
        exec("reg", &["delete", key, "/v", name, "/f"]).map(|_| ())
    }

    fn reg_export(&self, key: &str, file: &std::path::Path) -> Result<(), String> {
        exec("reg", &["export", key, &file.to_string_lossy(), "/y"]).map(|_| ())
    }

    fn run(&self, program: &str, args: &[&str]) -> Result<String, String> {
        exec(program, args)
    }

    fn is_admin(&self) -> bool {
        exec("net", &["session"]).is_ok()
    }
}

#[cfg(test)]
pub mod fake {
    use super::*;
    use std::cell::RefCell;

    /// Sistema finto: registro in memoria e registro dei comandi eseguiti.
    #[derive(Default)]
    pub struct FakeSys {
        /// (chiave, voce)
        pub reg: RefCell<Vec<(String, RegItem)>>,
        pub commands: RefCell<Vec<String>>,
        /// Risposta di `schtasks /query /xml`.
        pub tasks_xml: RefCell<String>,
        pub admin: bool,
        pub fail_run: RefCell<bool>,
        /// (testo contenuto nel comando, risposta)
        pub responses: RefCell<Vec<(String, String)>>,
        /// Comandi che devono fallire (testo contenuto).
        pub failing: RefCell<Vec<String>>,
    }

    impl FakeSys {
        pub fn add(&self, key: &str, name: &str, kind: &str, data: &str) {
            self.reg.borrow_mut().push((
                key.to_string(),
                RegItem {
                    name: name.into(),
                    kind: kind.into(),
                    data: data.into(),
                },
            ));
        }
        pub fn get(&self, key: &str, name: &str) -> Option<String> {
            self.reg
                .borrow()
                .iter()
                .find(|(k, i)| k.eq_ignore_ascii_case(key) && i.name == name)
                .map(|(_, i)| i.data.clone())
        }
    }

    impl Sys for FakeSys {
        fn reg_query(&self, key: &str) -> Vec<RegItem> {
            self.reg
                .borrow()
                .iter()
                .filter(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, i)| i.clone())
                .collect()
        }
        fn reg_set_binary(&self, key: &str, name: &str, hex: &str) -> Result<(), String> {
            self.reg
                .borrow_mut()
                .retain(|(k, i)| !(k.eq_ignore_ascii_case(key) && i.name == name));
            self.add(key, name, "REG_BINARY", hex);
            Ok(())
        }
        fn reg_delete_value(&self, key: &str, name: &str) -> Result<(), String> {
            self.reg
                .borrow_mut()
                .retain(|(k, i)| !(k.eq_ignore_ascii_case(key) && i.name == name));
            Ok(())
        }
        fn reg_export(&self, key: &str, file: &std::path::Path) -> Result<(), String> {
            std::fs::write(file, format!("backup di {key}")).map_err(|e| e.to_string())
        }
        fn run(&self, program: &str, args: &[&str]) -> Result<String, String> {
            self.commands
                .borrow_mut()
                .push(format!("{program} {}", args.join(" ")));
            let line = format!("{program} {}", args.join(" "));
            if *self.fail_run.borrow() || self.failing.borrow().iter().any(|f| line.contains(f)) {
                return Err("errore simulato".into());
            }
            if let Some((_, r)) = self
                .responses
                .borrow()
                .iter()
                .find(|(k, _)| line.contains(k))
            {
                return Ok(r.clone());
            }
            if program == "schtasks" && args.contains(&"/xml") && !args.contains(&"/tn") {
                return Ok(self.tasks_xml.borrow().clone());
            }
            Ok(String::new())
        }
        fn is_admin(&self) -> bool {
            self.admin
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_reg_query_output() {
        let t = "\r\nHKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Run\r\n    OneDrive    REG_SZ    \"C:\\Users\\Lo\\OneDrive.exe\" /background\r\n    My App    REG_EXPAND_SZ    %ProgramFiles%\\a.exe\r\n    Vuoto    REG_SZ    \r\n    Bin    REG_BINARY    030000000000000000000000\r\n";
        let v = parse_reg_query(t);
        assert_eq!(v.len(), 4);
        assert_eq!(v[0].name, "OneDrive");
        assert_eq!(v[0].data, "\"C:\\Users\\Lo\\OneDrive.exe\" /background");
        assert_eq!(v[1].name, "My App");
        assert_eq!(v[1].kind, "REG_EXPAND_SZ");
        assert_eq!(v[2].data, "");
        assert_eq!(v[3].kind, "REG_BINARY");
        assert_eq!(v[3].data, "030000000000000000000000");
    }

    #[test]
    fn ignores_garbage() {
        assert!(parse_reg_query("").is_empty());
        assert!(parse_reg_query("ERRORE: chiave non trovata\r\n").is_empty());
    }
}
