//! Accesso al registro tramite reg.exe (indipendente dalla lingua di Windows)
//! e una versione in memoria per i test.

use crate::regfile::{self, RegValue};
use std::path::Path;
use std::process::{Command, Stdio};

pub trait Registry {
    /// Tutti i valori (DWORD e stringa) sotto la chiave, sottochiavi comprese.
    /// Una chiave inesistente dà un elenco vuoto.
    fn read_key(&self, key: &str) -> Result<Vec<RegValue>, String>;
    /// Importa il testo di un file .reg.
    fn import(&self, reg_text: &str) -> Result<(), String>;
    /// Esporta la chiave in un file .reg. `false` se la chiave non esiste.
    fn backup_key(&self, key: &str, path: &Path) -> Result<bool, String>;
    fn is_admin(&self) -> bool;
}

pub struct RegExe;

fn tmp_file(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    std::env::temp_dir().join(format!(
        "tweakdesk_{}_{}_{tag}.reg",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ))
}

fn reg(args: &[&str]) -> Result<std::process::Output, String> {
    Command::new("reg")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("reg.exe non disponibile (serve Windows): {e}"))
}

impl Registry for RegExe {
    fn read_key(&self, key: &str) -> Result<Vec<RegValue>, String> {
        let f = tmp_file("r");
        let f_str = f.to_string_lossy().to_string();
        let out = reg(&["export", key, &f_str, "/y"])?;
        if !out.status.success() {
            let _ = std::fs::remove_file(&f);
            return Ok(Vec::new());
        }
        let bytes = std::fs::read(&f).map_err(|e| format!("{}: {e}", f.display()));
        let _ = std::fs::remove_file(&f);
        Ok(regfile::parse(&regfile::decode(&bytes?)))
    }

    fn import(&self, reg_text: &str) -> Result<(), String> {
        let f = tmp_file("i");
        std::fs::write(&f, regfile::encode(reg_text))
            .map_err(|e| format!("{}: {e}", f.display()))?;
        let out = reg(&["import", &f.to_string_lossy()]);
        let _ = std::fs::remove_file(&f);
        let out = out?;
        if out.status.success() {
            Ok(())
        } else {
            let msg = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            Err(msg.trim().to_string())
        }
    }

    fn backup_key(&self, key: &str, path: &Path) -> Result<bool, String> {
        Ok(reg(&["export", key, &path.to_string_lossy(), "/y"])?
            .status
            .success())
    }

    fn is_admin(&self) -> bool {
        Command::new("net")
            .arg("session")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

#[cfg(test)]
pub mod fake {
    use super::*;
    use crate::model::Value;
    use crate::regfile::Op;
    use std::cell::{Cell, RefCell};

    /// Registro in memoria.
    pub struct FakeRegistry {
        pub values: RefCell<Vec<RegValue>>,
        pub admin: bool,
        /// Simula un registro che ignora le scritture (per provare la verifica).
        pub ignore_writes: Cell<bool>,
        pub imports: RefCell<Vec<String>>,
    }

    impl FakeRegistry {
        pub fn new(admin: bool) -> Self {
            Self {
                values: RefCell::new(Vec::new()),
                admin,
                ignore_writes: Cell::new(false),
                imports: RefCell::new(Vec::new()),
            }
        }

        pub fn set(&self, key: &str, name: &str, value: Value) {
            self.apply_op(Op::Set(RegValue {
                key: key.into(),
                name: name.into(),
                value,
            }));
        }

        pub fn get(&self, key: &str, name: &str) -> Option<Value> {
            self.values
                .borrow()
                .iter()
                .find(|v| v.key.eq_ignore_ascii_case(key) && v.name.eq_ignore_ascii_case(name))
                .map(|v| v.value.clone())
        }

        fn apply_op(&self, op: Op) {
            let mut vals = self.values.borrow_mut();
            match op {
                Op::Set(n) => {
                    vals.retain(|v| {
                        !(v.key.eq_ignore_ascii_case(&n.key)
                            && v.name.eq_ignore_ascii_case(&n.name))
                    });
                    vals.push(n);
                }
                Op::Delete { key, name } => {
                    vals.retain(|v| {
                        !(v.key.eq_ignore_ascii_case(&key) && v.name.eq_ignore_ascii_case(&name))
                    });
                }
            }
        }
    }

    fn under(key: &str, root: &str) -> bool {
        let (k, r) = (key.to_lowercase(), root.to_lowercase());
        k == r || k.starts_with(&format!("{r}\\"))
    }

    impl Registry for FakeRegistry {
        fn read_key(&self, key: &str) -> Result<Vec<RegValue>, String> {
            Ok(self
                .values
                .borrow()
                .iter()
                .filter(|v| under(&v.key, key))
                .cloned()
                .collect())
        }

        fn import(&self, reg_text: &str) -> Result<(), String> {
            let ops = regfile::parse_ops(reg_text);
            if ops.iter().any(|o| {
                let k = match o {
                    Op::Set(v) => &v.key,
                    Op::Delete { key, .. } => key,
                };
                k.to_uppercase().starts_with("HKLM\\")
            }) && !self.admin
            {
                return Err("Accesso negato".into());
            }
            self.imports.borrow_mut().push(reg_text.to_string());
            if !self.ignore_writes.get() {
                for o in ops {
                    self.apply_op(o);
                }
            }
            Ok(())
        }

        fn backup_key(&self, key: &str, path: &Path) -> Result<bool, String> {
            let vals = self.read_key(key)?;
            if vals.is_empty() {
                return Ok(false);
            }
            let ops: Vec<Op> = vals.into_iter().map(Op::Set).collect();
            std::fs::write(path, regfile::encode(&regfile::build(&ops)))
                .map_err(|e| e.to_string())?;
            Ok(true)
        }

        fn is_admin(&self) -> bool {
            self.admin
        }
    }

    #[test]
    fn fake_registry_behaves_like_the_real_one() {
        let r = FakeRegistry::new(false);
        r.set("HKCU\\A\\B", "x", Value::Dword(1));
        r.set("HKCU\\AB", "y", Value::Dword(2));
        assert_eq!(r.read_key("HKCU\\A").unwrap().len(), 1);
        assert!(r.read_key("HKCU\\Nope").unwrap().is_empty());
        assert!(r
            .import(&regfile::build(&[Op::Set(RegValue {
                key: "HKLM\\Z".into(),
                name: "n".into(),
                value: Value::Dword(1)
            })]))
            .is_err());
        r.import(
            "Windows Registry Editor Version 5.00\r\n\r\n[HKEY_CURRENT_USER\\A\\B]\r\n\"x\"=-\r\n",
        )
        .unwrap();
        assert_eq!(r.get("HKCU\\A\\B", "x"), None);
    }
}
