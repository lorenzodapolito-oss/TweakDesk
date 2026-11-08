//! Dove si applicano le modifiche: sul Windows in uso oppure su un'immagine montata (registro caricato offline).

use crate::clean::sys::Sys;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Target {
    /// Cartella dove è montata l'immagine (None = Windows in uso).
    pub mount: Option<PathBuf>,
    /// Cartella `Windows` (in uso, oppure dentro l'immagine).
    pub windows: PathBuf,
}

const HIVES: [(&str, &str); 3] = [
    ("zTD_SOFTWARE", "Windows\\System32\\config\\SOFTWARE"),
    ("zTD_SYSTEM", "Windows\\System32\\config\\SYSTEM"),
    ("zTD_NTUSER", "Users\\Default\\NTUSER.DAT"),
];

pub fn join_rel(base: &Path, rel: &str) -> PathBuf {
    let mut p = base.to_path_buf();
    for part in rel.split(['\\', '/']).filter(|x| !x.is_empty()) {
        p.push(part);
    }
    p
}

impl Target {
    pub fn online(windows: PathBuf) -> Self {
        Self {
            mount: None,
            windows,
        }
    }

    pub fn offline(mount: &Path) -> Self {
        Self {
            mount: Some(mount.to_path_buf()),
            windows: mount.join("Windows"),
        }
    }

    pub fn is_offline(&self) -> bool {
        self.mount.is_some()
    }

    /// Chiave di registro da usare davvero (nell'immagine i rami sono caricati sotto `HKLM\zTD_*`).
    pub fn map_key(&self, key: &str) -> Result<String, String> {
        if !self.is_offline() {
            return Ok(key.to_string());
        }
        let k = key.replace('/', "\\");
        let rest = |prefix: &str| -> Option<String> {
            k.get(..prefix.len())
                .filter(|p| p.eq_ignore_ascii_case(prefix))
                .map(|_| k[prefix.len()..].to_string())
        };
        if let Some(r) = rest("HKCU\\") {
            return Ok(format!("HKLM\\zTD_NTUSER\\{r}"));
        }
        if let Some(r) = rest("HKLM\\SOFTWARE\\") {
            return Ok(format!("HKLM\\zTD_SOFTWARE\\{r}"));
        }
        if let Some(r) = rest("HKLM\\SYSTEM\\CurrentControlSet\\") {
            return Ok(format!("HKLM\\zTD_SYSTEM\\ControlSet001\\{r}"));
        }
        if let Some(r) = rest("HKLM\\SYSTEM\\") {
            return Ok(format!("HKLM\\zTD_SYSTEM\\{r}"));
        }
        Err(format!("Chiave non supportata offline: {key}"))
    }

    /// Carica i rami di registro dell'immagine. Se uno fallisce scarica quelli già caricati.
    pub fn load_hives(&self, sys: &dyn Sys) -> Result<(), String> {
        let Some(m) = &self.mount else {
            return Ok(());
        };
        let mut loaded = Vec::new();
        for (name, rel) in HIVES {
            let file = join_rel(m, rel);
            let key = format!("HKLM\\{name}");
            match sys.run("reg", &["load", &key, &file.to_string_lossy()]) {
                Ok(_) => loaded.push(key),
                Err(e) => {
                    for k in loaded {
                        let _ = sys.run("reg", &["unload", &k]);
                    }
                    return Err(format!("Impossibile caricare {}: {e}", file.display()));
                }
            }
        }
        Ok(())
    }

    pub fn unload_hives(&self, sys: &dyn Sys) {
        if self.mount.is_none() {
            return;
        }
        for (name, _) in HIVES {
            let key = format!("HKLM\\{name}");
            for attempt in 0..5 {
                if sys.run("reg", &["unload", &key]).is_ok() {
                    break;
                }
                if attempt < 4 {
                    std::thread::sleep(std::time::Duration::from_millis(400));
                }
            }
        }
    }

    pub fn hive_files(&self) -> Vec<(&'static str, PathBuf)> {
        match &self.mount {
            Some(m) => HIVES.iter().map(|(n, r)| (*n, join_rel(m, r))).collect(),
            None => Vec::new(),
        }
    }
}

/// Percorso sicuro sotto la cartella Windows: niente `..` salvo `..\Program Files (x86)\Microsoft\...`.
pub fn safe_join(windows: &Path, rel: &str) -> Option<PathBuf> {
    let rel = rel.replace('/', "\\");
    let mut base = windows.to_path_buf();
    let mut parts: Vec<&str> = rel.split('\\').filter(|p| !p.is_empty()).collect();
    if parts.first() == Some(&"..") {
        if parts.get(1) != Some(&"Program Files (x86)") {
            return None;
        }
        base = windows.parent()?.to_path_buf();
        parts.remove(0);
    }
    if parts.is_empty()
        || parts
            .iter()
            .any(|p| *p == ".." || *p == "." || p.contains(':'))
    {
        return None;
    }
    for p in parts {
        base.push(p);
    }
    Some(base)
}

pub fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clean::sys::fake::FakeSys;

    #[test]
    fn online_keys_are_unchanged() {
        let t = Target::online(PathBuf::from("C:\\Windows"));
        assert_eq!(t.map_key("HKCU\\Software\\X").unwrap(), "HKCU\\Software\\X");
    }

    #[test]
    fn offline_keys_are_remapped() {
        let t = Target::offline(Path::new("D:\\mnt"));
        assert_eq!(
            t.map_key("HKCU\\Software\\X").unwrap(),
            "HKLM\\zTD_NTUSER\\Software\\X"
        );
        assert_eq!(
            t.map_key("HKLM\\SOFTWARE\\Policies\\Y").unwrap(),
            "HKLM\\zTD_SOFTWARE\\Policies\\Y"
        );
        assert_eq!(
            t.map_key("HKLM\\SYSTEM\\CurrentControlSet\\Services\\Z")
                .unwrap(),
            "HKLM\\zTD_SYSTEM\\ControlSet001\\Services\\Z"
        );
        assert_eq!(
            t.map_key("hklm\\software\\a").unwrap(),
            "HKLM\\zTD_SOFTWARE\\a"
        );
        assert!(t.map_key("HKCR\\x").is_err());
        assert!(t.map_key("HKLM\\HARDWARE\\x").is_err());
    }

    #[test]
    fn hives_load_and_unload_in_order() {
        let t = Target::offline(Path::new("D:\\mnt"));
        let sys = FakeSys::default();
        t.load_hives(&sys).unwrap();
        t.unload_hives(&sys);
        let c = sys.commands.borrow().clone();
        assert_eq!(c.len(), 6);
        assert!(c[0].starts_with("reg load HKLM\\zTD_SOFTWARE "));
        assert!(c[2].contains("NTUSER.DAT"));
        assert!(c[3].starts_with("reg unload HKLM\\zTD_SOFTWARE"));
    }

    #[test]
    fn failed_load_unloads_the_others() {
        let t = Target::offline(Path::new("D:\\mnt"));
        let sys = FakeSys::default();
        sys.failing.borrow_mut().push("zTD_SYSTEM".into());
        assert!(t.load_hives(&sys).is_err());
        let c = sys.commands.borrow().clone();
        assert!(c.iter().any(|x| x == "reg unload HKLM\\zTD_SOFTWARE"));
    }

    #[test]
    fn online_needs_no_hives() {
        let sys = FakeSys::default();
        let t = Target::online(PathBuf::from("C:\\Windows"));
        t.load_hives(&sys).unwrap();
        t.unload_hives(&sys);
        assert!(sys.commands.borrow().is_empty());
    }

    #[test]
    fn safe_join_rules() {
        let w = Path::new("/m/Windows");
        assert_eq!(
            safe_join(w, "Logs\\*").unwrap(),
            Path::new("/m/Windows/Logs/*")
        );
        assert!(safe_join(w, "..\\Windows2").is_none());
        assert!(safe_join(w, "..\\Users\\x").is_none());
        assert_eq!(
            safe_join(w, "..\\Program Files (x86)\\Microsoft\\Edge").unwrap(),
            Path::new("/m/Program Files (x86)/Microsoft/Edge")
        );
        assert!(safe_join(w, "A\\..\\..").is_none());
        assert!(safe_join(w, "C:\\x").is_none());
        assert!(safe_join(w, "").is_none());
    }

    #[test]
    fn quoting() {
        assert_eq!(ps_quote("a'b"), "'a''b'");
    }
}
