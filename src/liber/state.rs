//! Stato di ogni app: file `liberdesk.json` nella sua cartella.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct State {
    /// Versione installata da LiberDesk (o letta dall'eseguibile).
    pub version: Option<String>,
    /// Ultimo controllo (testo leggibile).
    pub checked: String,
    /// Categoria scelta dall'utente (vince su quella del catalogo).
    pub category: Option<String>,
    /// Pacchetto winget scelto dall'utente per controllare l'ultima versione.
    pub winget_id: Option<String>,
}

pub const FILE: &str = "liberdesk.json";

pub fn load(dir: &Path) -> State {
    std::fs::read_to_string(dir.join(FILE))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save(dir: &Path, st: &State) -> Result<(), String> {
    let text = serde_json::to_string_pretty(st).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(FILE), text).map_err(|e| format!("{}: {e}", dir.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_garbage() {
        let d = std::env::temp_dir().join(format!("ld_st_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        assert_eq!(load(&d), State::default());
        let s = State {
            version: Some("1.2".into()),
            checked: "ieri".into(),
            category: Some("Giochi".into()),
            winget_id: Some("A.B".into()),
        };
        save(&d, &s).unwrap();
        assert_eq!(load(&d), s);
        std::fs::write(d.join(FILE), "[[").unwrap();
        assert_eq!(load(&d), State::default());
        let _ = std::fs::remove_dir_all(&d);
    }
}
