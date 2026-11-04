//! Impostazioni persistenti (`installdesk.json` accanto al programma).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const CONFIG_FILE: &str = "installdesk.json";
pub const DEFAULT_FOLDER_NAME: &str = "Installer";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// Cartella predefinita dove vengono scaricati gli installer.
    pub download_dir: String,
    /// Opzioni silenziose personalizzate, per nome programma.
    pub silent_overrides: BTreeMap<String, String>,
    /// Programmi (nomi del catalogo) da tenere aggiornati.
    pub tracked: Vec<String>,
}

impl AppConfig {
    /// Cartella effettiva: quella scelta, oppure `<cartella programma>\Installer`.
    pub fn effective_dir(&self, app_dir: &Path) -> PathBuf {
        let t = self.download_dir.trim();
        if t.is_empty() {
            app_dir.join(DEFAULT_FOLDER_NAME)
        } else {
            PathBuf::from(t)
        }
    }
}

pub fn app_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn load(path: &Path) -> AppConfig {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, cfg: &AppConfig) -> Result<(), String> {
    let json = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("idesk_cfg_{name}_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&d);
        d.join(CONFIG_FILE)
    }

    #[test]
    fn default_folder_is_next_to_the_program() {
        let cfg = AppConfig::default();
        assert_eq!(
            cfg.effective_dir(Path::new("D:\\Tools")),
            Path::new("D:\\Tools").join("Installer")
        );
        let own = AppConfig {
            download_dir: " E:\\Setup ".into(),
            ..AppConfig::default()
        };
        assert_eq!(
            own.effective_dir(Path::new("x")),
            PathBuf::from("E:\\Setup")
        );
    }

    #[test]
    fn round_trip_and_corrupt_file() {
        let p = tmp("rt");
        let mut cfg = AppConfig {
            download_dir: "E:\\S".into(),
            ..AppConfig::default()
        };
        cfg.silent_overrides.insert("Git".into(), "/SILENT".into());
        cfg.tracked.push("Git".into());
        save(&p, &cfg).unwrap();
        assert_eq!(load(&p), cfg);
        std::fs::write(&p, "{rotto").unwrap();
        assert_eq!(load(&p), AppConfig::default());
        assert_eq!(
            load(&p.with_file_name("assente.json")),
            AppConfig::default()
        );
    }
}
