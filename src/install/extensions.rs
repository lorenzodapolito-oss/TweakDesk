//! Elenco portatile delle estensioni e modi per reinstallarle.

use crate::install::chrome::{valid_id, Ext};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const LIST_VERSION: u32 = 1;
/// Indirizzo di aggiornamento del Chrome Web Store.
pub const UPDATE_URL: &str = "https://clients2.google.com/service/update2/crx";
/// Chiave dei criteri di Chrome che installa estensioni in modo forzato.
pub const POLICY_KEY: &str =
    r"HKEY_LOCAL_MACHINE\SOFTWARE\Policies\Google\Chrome\ExtensionInstallForcelist";
/// I valori scritti da ExtDesk partono da qui, così non toccano quelli già presenti.
pub const POLICY_FIRST: u32 = 7001;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ListEntry {
    pub id: String,
    pub name: String,
    pub version: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExtList {
    pub version: u32,
    pub created: String,
    pub source: String,
    pub extensions: Vec<ListEntry>,
}

impl ExtList {
    /// Un'estensione per id (la stessa in più profili conta una volta).
    pub fn from_exts(exts: &[&Ext], source: &str, created: &str) -> Self {
        let mut seen = std::collections::BTreeSet::new();
        let extensions = exts
            .iter()
            .filter(|e| seen.insert(e.id.clone()))
            .map(|e| ListEntry {
                id: e.id.clone(),
                name: e.name.clone(),
                version: e.version.clone(),
            })
            .collect();
        Self {
            version: LIST_VERSION,
            created: created.into(),
            source: source.into(),
            extensions,
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut l: ExtList = serde_json::from_str(text.trim_start_matches('\u{feff}'))
            .map_err(|e| format!("elenco non valido: {e}"))?;
        if l.version > LIST_VERSION {
            return Err(format!("elenco di versione {} non supportata", l.version));
        }
        l.extensions.retain(|e| valid_id(&e.id));
        Ok(l)
    }
}

pub fn store_url(id: &str) -> String {
    format!("https://chromewebstore.google.com/detail/{id}")
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Pagina con un collegamento al Web Store per ogni estensione: utile da aprire su un altro PC.
pub fn html_page(list: &ExtList) -> String {
    let mut s = String::from("<!DOCTYPE html><html lang=\"it\"><head><meta charset=\"utf-8\"><title>Estensioni di Chrome</title></head><body><h1>Estensioni di Chrome</h1><ol>\n");
    for e in list.extensions.iter().filter(|e| valid_id(&e.id)) {
        s.push_str(&format!(
            "<li><a href=\"{}\">{}</a> <small>{}</small></li>\n",
            store_url(&e.id),
            html_escape(&e.name),
            html_escape(&e.version)
        ));
    }
    s.push_str("</ol></body></html>\n");
    s
}

fn utf16(text: &str) -> Vec<u8> {
    let mut out = vec![0xFF, 0xFE];
    for u in text.encode_utf16() {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out
}

fn forcelist_ids(ids: &[String]) -> Vec<&String> {
    let mut seen = std::collections::BTreeSet::new();
    ids.iter()
        .filter(|i| valid_id(i) && seen.insert(i.as_str()))
        .collect()
}

/// File .reg (UTF-16) che fa installare le estensioni a Chrome tramite criteri.
pub fn forcelist_reg(ids: &[String]) -> Vec<u8> {
    let mut s = format!("Windows Registry Editor Version 5.00\r\n\r\n[{POLICY_KEY}]\r\n");
    for (i, id) in forcelist_ids(ids).iter().enumerate() {
        s.push_str(&format!(
            "\"{}\"=\"{id};{UPDATE_URL}\"\r\n",
            POLICY_FIRST + i as u32
        ));
    }
    utf16(&s)
}

/// File .reg che toglie i valori scritti da [`forcelist_reg`] con gli stessi id.
pub fn forcelist_remove_reg(ids: &[String]) -> Vec<u8> {
    let mut s = format!("Windows Registry Editor Version 5.00\r\n\r\n[{POLICY_KEY}]\r\n");
    for i in 0..forcelist_ids(ids).len() {
        s.push_str(&format!("\"{}\"=-\r\n", POLICY_FIRST + i as u32));
    }
    utf16(&s)
}

/// Aperture del Web Store a gruppi, per non spalancare decine di schede insieme.
pub fn batches(ids: &[String], size: usize) -> Vec<Vec<String>> {
    forcelist_ids(ids)
        .chunks(size.max(1))
        .map(|c| c.iter().map(|s| store_url(s)).collect())
        .collect()
}

/// Apre gli indirizzi in Chrome, se serve nel profilo indicato (su Windows `start chrome`, altrove
/// il browser predefinito).
pub fn open_in_chrome(urls: &[String], profile_dir: Option<&str>) -> Result<(), String> {
    use std::process::Command;
    let profile_arg = match profile_dir {
        Some(d) if crate::install::chrome::valid_profile_dir(d) => {
            Some(format!("--profile-directory={d}"))
        }
        Some(_) => return Err("Nome del profilo non valido".into()),
        None => None,
    };
    #[cfg(windows)]
    {
        Command::new("cmd")
            .args(["/c", "start", "", "chrome"])
            .args(profile_arg.iter())
            .args(urls)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Chrome non avviabile: {e}"))
    }
    #[cfg(not(windows))]
    {
        let _ = profile_arg;
        for u in urls {
            Command::new("xdg-open")
                .arg(u)
                .spawn()
                .map_err(|e| format!("apertura non riuscita: {e}"))?;
        }
        Ok(())
    }
}

/// Importa un .reg con privilegi di amministratore (Windows chiede il permesso).
pub fn import_reg_as_admin(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        let arg = format!(
            "Start-Process -FilePath reg.exe -ArgumentList 'import','\"{}\"' -Verb RunAs -Wait",
            path.display()
        );
        crate::clean::sys::quiet(std::process::Command::new("powershell").args([
            "-NoProfile",
            "-Command",
            &arg,
        ]))
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("PowerShell non avviabile: {e}"))
    }
    #[cfg(not(windows))]
    {
        Err(format!(
            "importa a mano il file {} (solo Windows)",
            path.display()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn ext(id: &str, name: &str, profile: &str) -> Ext {
        Ext {
            id: id.into(),
            name: name.into(),
            version: "1.0".into(),
            profile: profile.into(),
            profile_dir: "Default".into(),
            store: true,
            theme: false,
        }
    }

    fn decode(b: &[u8]) -> String {
        let u: Vec<u16> = b[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&u)
    }

    #[test]
    fn list_dedups_saves_and_filters_bad_ids() {
        let a1 = ext(A, "Uno", "Default");
        let a2 = ext(A, "Uno", "Profile 1");
        let b = ext(B, "Due", "Default");
        let l = ExtList::from_exts(&[&a1, &a2, &b], "pc", "oggi");
        assert_eq!(l.extensions.len(), 2);

        let dir = std::env::temp_dir().join(format!("extdesk_list_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("e.extdesk.json");
        l.save(&p).unwrap();
        assert_eq!(ExtList::load(&p).unwrap(), l);

        let mut bad = l.clone();
        bad.extensions.push(ListEntry {
            id: "../../x".into(),
            name: "x".into(),
            version: "1".into(),
        });
        bad.save(&p).unwrap();
        assert_eq!(ExtList::load(&p).unwrap().extensions.len(), 2);

        std::fs::write(&p, "{rotto").unwrap();
        assert!(ExtList::load(&p).unwrap_err().contains("non valido"));
        let mut future = l;
        future.version = 99;
        future.save(&p).unwrap();
        assert!(ExtList::load(&p).unwrap_err().contains("versione"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn policy_files_use_own_numbering_and_match() {
        let ids = vec![
            A.to_string(),
            "non valido".into(),
            B.to_string(),
            A.to_string(),
        ];
        let add = decode(&forcelist_reg(&ids));
        assert!(add.starts_with("Windows Registry Editor Version 5.00\r\n"));
        assert!(add.contains(&format!("\"7001\"=\"{A};{UPDATE_URL}\"")));
        assert!(add.contains(&format!("\"7002\"=\"{B};{UPDATE_URL}\"")));
        assert!(!add.contains("7003") && !add.contains("non valido"));
        let del = decode(&forcelist_remove_reg(&ids));
        assert!(del.contains("\"7001\"=-") && del.contains("\"7002\"=-"));
        assert!(!del.contains("7003"));
    }

    #[test]
    fn batches_and_html_are_safe() {
        let ids: Vec<String> = [A, B].iter().map(|s| s.to_string()).collect();
        let b = batches(&ids, 1);
        assert_eq!(b.len(), 2);
        assert_eq!(b[0], vec![store_url(A)]);
        assert_eq!(
            batches(&ids, 0).len(),
            2,
            "dimensione zero non manda in crash"
        );
        assert!(batches(&[], 5).is_empty());

        let l = ExtList {
            version: 1,
            created: "x".into(),
            source: "y".into(),
            extensions: vec![ListEntry {
                id: A.into(),
                name: "<b>\"x\"&".into(),
                version: "1".into(),
            }],
        };
        let h = html_page(&l);
        assert!(h.contains("&lt;b&gt;&quot;x&quot;&amp;"));
        assert!(h.contains(&store_url(A)));
    }
}
