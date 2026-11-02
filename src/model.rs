//! Tipi condivisi: valori di registro, voci di profilo e profilo.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum Value {
    Dword(u32),
    Sz(String),
}

impl Value {
    pub fn show(&self) -> String {
        match self {
            Value::Dword(n) => n.to_string(),
            Value::Sz(s) => format!("\"{s}\""),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Risk {
    Normal,
    Caution,
    Security,
}

impl Risk {
    pub fn label(self) -> &'static str {
        match self {
            Risk::Normal => "",
            Risk::Caution => "Attenzione",
            Risk::Security => "Riduce la sicurezza",
        }
    }
}

/// Valore che Windows usa di fabbrica.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Baseline {
    /// Il valore esiste di fabbrica (o, se manca, Windows si comporta come se valesse questo).
    Value(Value),
    /// Di fabbrica il valore non esiste.
    Absent,
    /// Non si sa (voci importate da un file .reg qualsiasi).
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub group: String,
    pub label: String,
    /// Chiave con prefisso `HKCU\` o `HKLM\`.
    pub key: String,
    /// Nome del valore (vuoto = valore predefinito della chiave).
    pub name: String,
    pub value: Value,
    pub baseline: Baseline,
    pub risk: Risk,
}

impl Entry {
    pub fn ident(&self) -> (String, String) {
        (self.key.to_lowercase(), self.name.to_lowercase())
    }

    pub fn needs_admin(&self) -> bool {
        self.key.to_uppercase().starts_with("HKLM\\")
    }
}

pub const PROFILE_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub version: u32,
    pub created: String,
    pub source: String,
    pub entries: Vec<Entry>,
}

impl Profile {
    pub fn new(entries: Vec<Entry>, source: &str, created: &str) -> Self {
        Self {
            version: PROFILE_VERSION,
            created: created.into(),
            source: source.into(),
            entries,
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let p: Profile =
            serde_json::from_str(&text).map_err(|e| format!("profilo non valido: {e}"))?;
        if p.version > PROFILE_VERSION {
            return Err(format!(
                "profilo di versione {} non supportata (serve un TweakDesk più recente)",
                p.version
            ));
        }
        if let Some(bad) = p.entries.iter().find(|e| !valid_key(&e.key)) {
            return Err(format!("chiave non ammessa nel profilo: {}", bad.key));
        }
        Ok(p)
    }
}

/// Solo HKCU e HKLM: un profilo non deve poter scrivere altrove.
pub fn valid_key(key: &str) -> bool {
    let up = key.to_uppercase();
    (up.starts_with("HKCU\\") || up.starts_with("HKLM\\")) && key.len() > 5 && !key.contains('\n')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(key: &str) -> Entry {
        Entry {
            group: "g".into(),
            label: "l".into(),
            key: key.into(),
            name: "N".into(),
            value: Value::Dword(1),
            baseline: Baseline::Absent,
            risk: Risk::Normal,
        }
    }

    #[test]
    fn profile_round_trip_and_validation() {
        let dir = std::env::temp_dir().join(format!("td_model_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("p.tweakdesk.json");
        let p = Profile::new(
            vec![entry("HKCU\\Software\\X"), entry("HKLM\\SOFTWARE\\Y")],
            "test",
            "oggi",
        );
        p.save(&path).unwrap();
        assert_eq!(Profile::load(&path).unwrap(), p);

        let bad = Profile::new(vec![entry("HKCR\\x")], "t", "o");
        bad.save(&path).unwrap();
        assert!(Profile::load(&path)
            .unwrap_err()
            .contains("chiave non ammessa"));

        std::fs::write(&path, "{rotto").unwrap();
        assert!(Profile::load(&path).unwrap_err().contains("non valido"));

        let mut future = Profile::new(vec![], "t", "o");
        future.version = 99;
        future.save(&path).unwrap();
        assert!(Profile::load(&path).unwrap_err().contains("versione"));
    }

    #[test]
    fn admin_and_ident() {
        assert!(entry("hklm\\software").needs_admin());
        assert!(!entry("HKCU\\software").needs_admin());
        assert_eq!(entry("HKCU\\A").ident(), ("hkcu\\a".into(), "n".into()));
        assert!(valid_key("HKCU\\a") && !valid_key("HKCU\\") && !valid_key("HKU\\a"));
    }

    #[test]
    fn value_display() {
        assert_eq!(Value::Dword(5).show(), "5");
        assert_eq!(Value::Sz("x".into()).show(), "\"x\"");
    }
}
