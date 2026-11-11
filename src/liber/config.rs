//! Impostazioni (file `liberdesk.json` accanto al programma: tutto resta nella cartella portatile).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Config {
    /// Cartella delle app; vuota = `Apps` accanto al programma.
    pub apps_dir: String,
    /// Cartella degli installer non portabili; vuota = `Installers` accanto al programma.
    pub installers_dir: String,
    /// Controlla gli aggiornamenti all'avvio.
    pub auto_check: bool,
    /// Scarica e installa subito quelli trovati.
    pub auto_install: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            apps_dir: String::new(),
            installers_dir: String::new(),
            auto_check: true,
            auto_install: true,
        }
    }
}

impl Config {
    pub fn effective_dir(&self, base: &Path) -> PathBuf {
        let d = self.apps_dir.trim();
        if d.is_empty() {
            base.join("Apps")
        } else {
            PathBuf::from(d)
        }
    }
}

impl Config {
    pub fn effective_installers_dir(&self, base: &Path) -> PathBuf {
        let d = self.installers_dir.trim();
        if d.is_empty() {
            base.join("Installers")
        } else {
            PathBuf::from(d)
        }
    }
}

/// Cartella del programma.
pub fn base_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn load(path: &Path) -> Config {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, cfg: &Config) -> Result<(), String> {
    let text = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_round_trip() {
        let d = std::env::temp_dir().join(format!("ld_cfg_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("liberdesk.json");
        assert_eq!(load(&f), Config::default());
        let c = Config {
            apps_dir: "E:\\Apps".into(),
            installers_dir: "E:\\Inst".into(),
            auto_check: false,
            auto_install: false,
        };
        save(&f, &c).unwrap();
        assert_eq!(load(&f), c);
        std::fs::write(&f, "{ rotto").unwrap();
        assert_eq!(load(&f), Config::default());
        assert_eq!(Config::default().effective_dir(&d), d.join("Apps"));
        assert_eq!(c.effective_dir(&d), PathBuf::from("E:\\Apps"));
        assert_eq!(
            Config::default().effective_installers_dir(&d),
            d.join("Installers")
        );
        assert_eq!(c.effective_installers_dir(&d), PathBuf::from("E:\\Inst"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
