//! Logica: estrazione delle impostazioni non di fabbrica, applicazione con
//! copia di sicurezza e verifica, annullamento.

use crate::model::{valid_key, Baseline, Entry, Risk, Value};
use crate::regfile::{self, Op, RegValue};
use crate::registry::Registry;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Valori letti dal registro: (chiave, nome) in minuscolo -> valore.
pub type Current = HashMap<(String, String), Value>;

fn read_keys(reg: &dyn Registry, keys: &[String]) -> Result<Current, String> {
    let mut seen: Vec<String> = Vec::new();
    let mut cur = Current::new();
    for key in keys {
        let low = key.to_lowercase();
        if seen.contains(&low) {
            continue;
        }
        seen.push(low.clone());
        for v in reg.read_key(key)? {
            if v.key.to_lowercase() == low {
                cur.insert((low.clone(), v.name.to_lowercase()), v.value);
            }
        }
    }
    Ok(cur)
}

pub fn read_current(reg: &dyn Registry, entries: &[&Entry]) -> Result<Current, String> {
    read_keys(
        reg,
        &entries.iter().map(|e| e.key.clone()).collect::<Vec<_>>(),
    )
}

/// Delle voci del catalogo, quelle che su questo PC sono diverse da Windows di fabbrica.
/// Il valore del profilo è quello attuale del registro.
pub fn extract(reg: &dyn Registry, catalog: &[Entry]) -> Result<Vec<Entry>, String> {
    let refs: Vec<&Entry> = catalog.iter().collect();
    let cur = read_current(reg, &refs)?;
    Ok(catalog
        .iter()
        .filter_map(|e| {
            let now = cur.get(&e.ident())?;
            let differs = match &e.baseline {
                Baseline::Value(b) => b != now,
                Baseline::Absent | Baseline::Unknown => true,
            };
            differs.then(|| Entry {
                value: now.clone(),
                ..e.clone()
            })
        })
        .collect())
}

/// Voci lette da un file .reg qualsiasi (solo HKCU/HKLM, solo DWORD e stringhe).
/// Restituisce le voci e quante ne sono state scartate.
pub fn from_reg(text: &str) -> (Vec<Entry>, usize) {
    let mut out: Vec<Entry> = Vec::new();
    let mut skipped = 0;
    for v in regfile::parse(text) {
        if !valid_key(&v.key) {
            skipped += 1;
            continue;
        }
        let label = if v.name.is_empty() {
            format!("{} (predefinito)", v.key)
        } else {
            format!("{} \\ {}", v.key, v.name)
        };
        let e = Entry {
            group: "Importate da .reg".into(),
            label,
            key: v.key,
            name: v.name,
            value: v.value,
            baseline: Baseline::Unknown,
            risk: Risk::Normal,
        };
        match out.iter_mut().find(|o| o.ident() == e.ident()) {
            Some(o) => *o = e,
            None => out.push(e),
        }
    }
    (out, skipped)
}

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Applied,
    /// Da applicare; contiene il valore attuale (se esiste).
    ToApply(Option<Value>),
}

pub fn status(e: &Entry, cur: &Current) -> Status {
    match cur.get(&e.ident()) {
        Some(v) if *v == e.value => Status::Applied,
        other => Status::ToApply(other.cloned()),
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UndoItem {
    pub key: String,
    pub name: String,
    /// Valore prima della modifica; `None` = non esisteva.
    pub before: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UndoFile {
    pub version: u32,
    pub created: String,
    pub items: Vec<UndoItem>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    pub done: usize,
    pub already: usize,
    /// (voce, motivo)
    pub failed: Vec<(String, String)>,
    pub backup_dir: Option<PathBuf>,
}

fn is_hklm(key: &str) -> bool {
    key.to_uppercase().starts_with("HKLM\\")
}

/// Importa le operazioni, una volta per ramo (HKCU e HKLM), poi segna gli errori.
fn import_by_hive(
    reg: &dyn Registry,
    ops: &[(Op, String)],
    failed: &mut Vec<(String, String)>,
) -> Vec<bool> {
    let mut ok = vec![false; ops.len()];
    for hklm in [false, true] {
        let idx: Vec<usize> = (0..ops.len())
            .filter(|&i| is_hklm(&op_key(&ops[i].0)) == hklm)
            .collect();
        if idx.is_empty() {
            continue;
        }
        let batch: Vec<Op> = idx.iter().map(|&i| ops[i].0.clone()).collect();
        match reg.import(&regfile::build(&batch)) {
            Ok(()) => idx.iter().for_each(|&i| ok[i] = true),
            Err(e) => idx
                .iter()
                .for_each(|&i| failed.push((ops[i].1.clone(), e.clone()))),
        }
    }
    ok
}

fn op_key(o: &Op) -> String {
    match o {
        Op::Set(v) => v.key.clone(),
        Op::Delete { key, .. } => key.clone(),
    }
}

const ADMIN_MSG: &str = "serve l'esecuzione come amministratore";

/// Applica le voci: salva prima i valori attuali (annullabili), importa, verifica.
pub fn apply(
    reg: &dyn Registry,
    entries: &[&Entry],
    backup_root: &Path,
    stamp: &str,
) -> Result<Report, String> {
    let cur = read_current(reg, entries)?;
    let mut rep = Report::default();
    let mut todo: Vec<&Entry> = Vec::new();
    for e in entries {
        if status(e, &cur) == Status::Applied {
            rep.already += 1;
        } else if is_hklm(&e.key) && !reg.is_admin() {
            rep.failed.push((e.label.clone(), ADMIN_MSG.into()));
        } else {
            todo.push(e);
        }
    }
    if todo.is_empty() {
        return Ok(rep);
    }

    let dir = backup_root.join(stamp);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let undo = UndoFile {
        version: 1,
        created: stamp.into(),
        items: todo
            .iter()
            .map(|e| UndoItem {
                key: e.key.clone(),
                name: e.name.clone(),
                before: cur.get(&e.ident()).cloned(),
            })
            .collect(),
    };
    let json = serde_json::to_string_pretty(&undo).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("annulla.json"), json)
        .map_err(|e| format!("copia di sicurezza: {e}"))?;
    let mut keys: Vec<String> = Vec::new();
    for e in &todo {
        if !keys.iter().any(|k| k.eq_ignore_ascii_case(&e.key)) {
            keys.push(e.key.clone());
        }
    }
    for (i, k) in keys.iter().enumerate() {
        let _ = reg.backup_key(k, &dir.join(format!("chiave_{}.reg", i + 1)));
    }
    rep.backup_dir = Some(dir);

    let ops: Vec<(Op, String)> = todo
        .iter()
        .map(|e| {
            (
                Op::Set(RegValue {
                    key: e.key.clone(),
                    name: e.name.clone(),
                    value: e.value.clone(),
                }),
                e.label.clone(),
            )
        })
        .collect();
    let ok = import_by_hive(reg, &ops, &mut rep.failed);

    let applied: Vec<&Entry> = todo
        .iter()
        .zip(&ok)
        .filter(|(_, &o)| o)
        .map(|(e, _)| *e)
        .collect();
    let after = read_current(reg, &applied)?;
    for e in applied {
        if after.get(&e.ident()) == Some(&e.value) {
            rep.done += 1;
        } else {
            rep.failed
                .push((e.label.clone(), "scritta ma non verificata".into()));
        }
    }
    Ok(rep)
}

pub fn load_undo(path: &Path) -> Result<UndoFile, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let u: UndoFile =
        serde_json::from_str(&text).map_err(|e| format!("file di annullamento non valido: {e}"))?;
    if let Some(bad) = u.items.iter().find(|i| !valid_key(&i.key)) {
        return Err(format!("chiave non ammessa: {}", bad.key));
    }
    Ok(u)
}

/// Rimette i valori com'erano prima di `apply` (e cancella quelli che non esistevano).
pub fn undo(reg: &dyn Registry, path: &Path) -> Result<Report, String> {
    let file = load_undo(path)?;
    let mut rep = Report::default();
    let mut ops: Vec<(Op, String)> = Vec::new();
    for it in &file.items {
        let label = if it.name.is_empty() {
            format!("{} (predefinito)", it.key)
        } else {
            format!("{} \\ {}", it.key, it.name)
        };
        if is_hklm(&it.key) && !reg.is_admin() {
            rep.failed.push((label, ADMIN_MSG.into()));
            continue;
        }
        let op = match &it.before {
            Some(v) => Op::Set(RegValue {
                key: it.key.clone(),
                name: it.name.clone(),
                value: v.clone(),
            }),
            None => Op::Delete {
                key: it.key.clone(),
                name: it.name.clone(),
            },
        };
        ops.push((op, label));
    }
    let ok = import_by_hive(reg, &ops, &mut rep.failed);
    let keys: Vec<String> = ops.iter().map(|(o, _)| op_key(o)).collect();
    let after = read_keys(reg, &keys)?;
    for ((op, label), &imported) in ops.iter().zip(&ok) {
        if !imported {
            continue;
        }
        let (key, name, want) = match op {
            Op::Set(v) => (&v.key, &v.name, Some(&v.value)),
            Op::Delete { key, name } => (key, name, None),
        };
        if after.get(&(key.to_lowercase(), name.to_lowercase())) == want {
            rep.done += 1;
        } else {
            rep.failed.push((label.clone(), "non ripristinata".into()));
        }
    }
    Ok(rep)
}

/// Cartelle di copia di sicurezza esistenti (la più recente per ultima).
pub fn list_backups(root: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(root)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.join("annulla.json").is_file())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::catalog;
    use crate::registry::fake::FakeRegistry;

    const ADV: &str = "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Advanced";
    const MM: &str = "HKLM\\SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Memory Management";

    fn entry(key: &str, name: &str, value: Value, baseline: Baseline) -> Entry {
        Entry {
            group: "g".into(),
            label: name.to_string(),
            key: key.into(),
            name: name.into(),
            value,
            baseline,
            risk: Risk::Normal,
        }
    }

    fn dir(n: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("td_eng_{n}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn extract_keeps_only_non_factory_values_and_uses_current_value() {
        let r = FakeRegistry::new(true);
        r.set(ADV, "Hidden", Value::Dword(1)); // diverso da 2 di fabbrica
        r.set(ADV, "HideFileExt", Value::Dword(1)); // uguale al valore di fabbrica
        r.set(ADV, "LaunchTo", Value::Dword(7)); // diverso e non quello del catalogo
        r.set(MM, "FeatureSettingsOverride", Value::Dword(3)); // baseline assente
        let got = extract(&r, &catalog()).unwrap();
        let names: Vec<&str> = got.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"Hidden") && names.contains(&"FeatureSettingsOverride"));
        assert!(!names.contains(&"HideFileExt") && !names.contains(&"UseCompactMode"));
        assert_eq!(
            got.iter().find(|e| e.name == "LaunchTo").unwrap().value,
            Value::Dword(7)
        );
    }

    #[test]
    fn extract_on_a_clean_registry_is_empty() {
        assert!(extract(&FakeRegistry::new(true), &catalog())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn from_reg_filters_hives_and_dedupes() {
        let text = "Windows Registry Editor Version 5.00\r\n[HKEY_CURRENT_USER\\A]\r\n\"x\"=dword:00000001\r\n\"x\"=dword:00000002\r\n@=\"d\"\r\n[HKEY_CLASSES_ROOT\\B]\r\n\"y\"=dword:00000001\r\n";
        let (v, skipped) = from_reg(text);
        assert_eq!(v.len(), 2);
        assert_eq!(skipped, 1);
        assert_eq!(v[0].value, Value::Dword(2));
        assert!(v[1].label.contains("predefinito"));
    }

    #[test]
    fn apply_writes_backs_up_verifies_and_is_idempotent() {
        let r = FakeRegistry::new(true);
        r.set(ADV, "Hidden", Value::Dword(2));
        let e1 = entry(ADV, "Hidden", Value::Dword(1), Baseline::Unknown);
        let e2 = entry(
            MM,
            "DisablePagingExecutive",
            Value::Dword(1),
            Baseline::Unknown,
        );
        let root = dir("apply");
        let rep = apply(&r, &[&e1, &e2], &root, "s1").unwrap();
        assert_eq!((rep.done, rep.already, rep.failed.len()), (2, 0, 0));
        assert_eq!(r.get(ADV, "Hidden"), Some(Value::Dword(1)));
        assert_eq!(r.get(MM, "DisablePagingExecutive"), Some(Value::Dword(1)));
        assert_eq!(r.imports.borrow().len(), 2, "un import per ramo");
        let bd = rep.backup_dir.unwrap();
        assert!(bd.join("annulla.json").is_file() && bd.join("chiave_1.reg").is_file());

        let again = apply(&r, &[&e1, &e2], &root, "s2").unwrap();
        assert_eq!((again.done, again.already), (0, 2));
        assert!(again.backup_dir.is_none());
        assert_eq!(list_backups(&root).len(), 1);
    }

    #[test]
    fn hklm_without_admin_is_reported_and_hkcu_still_applied() {
        let r = FakeRegistry::new(false);
        let e1 = entry(ADV, "Hidden", Value::Dword(1), Baseline::Unknown);
        let e2 = entry(
            MM,
            "DisablePagingExecutive",
            Value::Dword(1),
            Baseline::Unknown,
        );
        let rep = apply(&r, &[&e1, &e2], &dir("noadmin"), "s").unwrap();
        assert_eq!(rep.done, 1);
        assert_eq!(rep.failed.len(), 1);
        assert!(rep.failed[0].1.contains("amministratore"));
        assert_eq!(r.get(MM, "DisablePagingExecutive"), None);
    }

    #[test]
    fn unverified_write_is_reported() {
        let r = FakeRegistry::new(true);
        r.ignore_writes.set(true);
        let e = entry(ADV, "Hidden", Value::Dword(1), Baseline::Unknown);
        let rep = apply(&r, &[&e], &dir("verify"), "s").unwrap();
        assert_eq!(rep.done, 0);
        assert!(rep.failed[0].1.contains("non verificata"));
    }

    #[test]
    fn undo_restores_old_values_and_removes_new_ones() {
        let r = FakeRegistry::new(true);
        r.set(ADV, "Hidden", Value::Dword(2));
        let e1 = entry(ADV, "Hidden", Value::Dword(1), Baseline::Unknown);
        let e2 = entry(ADV, "Nuova", Value::Sz("x".into()), Baseline::Unknown);
        let e3 = entry(
            "HKCU\\Software\\Classes\\CLSID\\{x}\\InprocServer32",
            "",
            Value::Sz(String::new()),
            Baseline::Absent,
        );
        let root = dir("undo");
        let rep = apply(&r, &[&e1, &e2, &e3], &root, "s").unwrap();
        assert_eq!(rep.done, 3);
        let back = undo(&r, &rep.backup_dir.unwrap().join("annulla.json")).unwrap();
        assert_eq!((back.done, back.failed.len()), (3, 0));
        assert_eq!(r.get(ADV, "Hidden"), Some(Value::Dword(2)));
        assert_eq!(r.get(ADV, "Nuova"), None);
        assert_eq!(
            r.get("HKCU\\Software\\Classes\\CLSID\\{x}\\InprocServer32", ""),
            None
        );
    }

    #[test]
    fn undo_rejects_bad_files() {
        let d = dir("badundo");
        let p = d.join("annulla.json");
        std::fs::write(&p, "{x").unwrap();
        assert!(load_undo(&p).unwrap_err().contains("non valido"));
        let bad = UndoFile {
            version: 1,
            created: "s".into(),
            items: vec![UndoItem {
                key: "HKCR\\x".into(),
                name: "n".into(),
                before: None,
            }],
        };
        std::fs::write(&p, serde_json::to_string(&bad).unwrap()).unwrap();
        assert!(load_undo(&p).unwrap_err().contains("non ammessa"));
    }

    #[test]
    fn status_reports_applied_or_current_value() {
        let r = FakeRegistry::new(true);
        r.set(ADV, "Hidden", Value::Dword(2));
        let e = entry(ADV, "Hidden", Value::Dword(1), Baseline::Unknown);
        let cur = read_current(&r, &[&e]).unwrap();
        assert_eq!(status(&e, &cur), Status::ToApply(Some(Value::Dword(2))));
        r.set(ADV, "Hidden", Value::Dword(1));
        let cur = read_current(&r, &[&e]).unwrap();
        assert_eq!(status(&e, &cur), Status::Applied);
    }
}
