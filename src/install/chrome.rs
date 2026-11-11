//! Lettura delle estensioni installate in Chrome (tutti i profili).

use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct Ext {
    pub id: String,
    pub name: String,
    pub version: String,
    pub profile: String,
    /// Nome della cartella del profilo (`Default`, `Profile 1`…).
    pub profile_dir: String,
    /// Installata dal Chrome Web Store (ha l'indirizzo di aggiornamento di Google).
    pub store: bool,
    pub theme: bool,
}

/// Un id di estensione: 32 lettere da `a` a `p`.
pub fn valid_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| (b'a'..=b'p').contains(&b))
}

/// Cartella "User Data" di Chrome per il sistema in uso.
pub fn default_user_data() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join(r"Google\Chrome\User Data"))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(|p| PathBuf::from(p).join("Library/Application Support/Google/Chrome"))
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config/google-chrome"))
    }
}

fn read_json(path: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()
}

/// Confronto numerico di versioni `1.2.10_0` (il suffisso `_n` di Chrome si ignora).
fn version_key(v: &str) -> Vec<u64> {
    v.split('_')
        .next()
        .unwrap_or("")
        .split('.')
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

/// Risolve `__MSG_chiave__` con i file in `_locales`.
fn localized(dir: &Path, manifest: &Value, text: &str) -> String {
    let key = match text
        .strip_prefix("__MSG_")
        .and_then(|t| t.strip_suffix("__"))
    {
        Some(k) => k.to_lowercase(),
        None => return text.to_string(),
    };
    let default = manifest
        .get("default_locale")
        .and_then(Value::as_str)
        .unwrap_or("en");
    for loc in [default, "it", "en", "en_US", "en_GB"] {
        let file = dir.join("_locales").join(loc).join("messages.json");
        let Some(Value::Object(map)) = read_json(&file) else {
            continue;
        };
        let found = map
            .iter()
            .find(|(k, _)| k.to_lowercase() == key)
            .and_then(|(_, v)| v.get("message"))
            .and_then(Value::as_str);
        if let Some(m) = found {
            return m.to_string();
        }
    }
    text.to_string()
}

/// Un profilo di Chrome: cartella e nome da mostrare (nome, account e cartella).
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub dir: String,
    pub shown: String,
}

/// Una cartella di profilo valida: niente percorsi né caratteri speciali.
pub fn valid_profile_dir(d: &str) -> bool {
    !d.is_empty()
        && d.len() <= 64
        && d.chars()
            .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '_' | '-'))
}

fn label(info: &Value, d: &str) -> String {
    let p = info.get(d);
    let name = p.and_then(|p| p.get("name")).and_then(Value::as_str);
    let mail = p
        .and_then(|p| p.get("user_name"))
        .and_then(Value::as_str)
        .filter(|m| !m.is_empty());
    match (name, mail) {
        (Some(n), Some(m)) => format!("{n}, {m} ({d})"),
        (Some(n), None) => format!("{n} ({d})"),
        (None, Some(m)) => format!("{m} ({d})"),
        (None, None) => d.to_string(),
    }
}

/// Profili di Chrome; con `only_with_extensions` solo quelli che hanno estensioni.
pub fn profiles(user_data: &Path, only_with_extensions: bool) -> Vec<Profile> {
    let info = read_json(&user_data.join("Local State"))
        .and_then(|v| v.pointer("/profile/info_cache").cloned())
        .unwrap_or(Value::Null);
    let mut dirs: Vec<String> = std::fs::read_dir(user_data)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter(|e| {
                    if only_with_extensions {
                        e.path().join("Extensions").is_dir()
                    } else {
                        e.path().join("Preferences").is_file()
                    }
                })
                .filter_map(|e| e.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default();
    dirs.sort();
    dirs.into_iter()
        .map(|d| Profile {
            shown: label(&info, &d),
            dir: d,
        })
        .collect()
}

fn read_extension(profile: &str, profile_dir: &str, id: &str, folder: &Path) -> Option<Ext> {
    let mut versions: Vec<String> = std::fs::read_dir(folder)
        .ok()?
        .filter_map(Result::ok)
        .filter(|e| e.path().join("manifest.json").is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    versions.sort_by_key(|v| version_key(v));
    let latest = versions.pop()?;
    let dir = folder.join(&latest);
    let manifest = read_json(&dir.join("manifest.json"))?;
    let raw = manifest.get("name").and_then(Value::as_str).unwrap_or(id);
    let name = localized(&dir, &manifest, raw);
    let version = manifest
        .get("version")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| latest.split('_').next().unwrap_or("").to_string());
    let store = manifest
        .get("update_url")
        .and_then(Value::as_str)
        .is_some_and(|u| u.contains("google.com") || u.contains("googleapis.com"));
    Some(Ext {
        id: id.to_string(),
        name,
        version,
        profile: profile.to_string(),
        profile_dir: profile_dir.to_string(),
        store,
        theme: manifest.get("theme").is_some(),
    })
}

/// Tutte le estensioni di tutti i profili sotto `user_data`, ordinate per nome.
pub fn scan(user_data: &Path) -> Vec<Ext> {
    let mut out = Vec::new();
    for Profile { dir, shown } in profiles(user_data, true) {
        let ext_dir = user_data.join(&dir).join("Extensions");
        let Ok(rd) = std::fs::read_dir(&ext_dir) else {
            continue;
        };
        for e in rd.filter_map(Result::ok) {
            let Ok(id) = e.file_name().into_string() else {
                continue;
            };
            if !valid_id(&id) {
                continue;
            }
            if let Some(x) = read_extension(&shown, &dir, &id, &e.path()) {
                out.push(x);
            }
        }
    }
    out.sort_by_key(|x| (x.name.to_lowercase(), x.profile.clone()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const ID_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("extdesk_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn put(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn id_validation() {
        assert!(valid_id(ID_A));
        assert!(!valid_id("short"));
        assert!(!valid_id(&"q".repeat(32)));
        assert!(!valid_id(&"A".repeat(32)));
    }

    #[test]
    fn reads_profiles_versions_and_localized_names() {
        let ud = tmp("scan");
        put(
            &ud.join("Local State"),
            r#"{"profile":{"info_cache":{"Default":{"name":"Lorenzo"}}}}"#,
        );
        // A: due versioni, nome localizzato, dal Web Store.
        put(
            &ud.join(format!("Default/Extensions/{ID_A}/1.9.0_0/manifest.json")),
            r#"{"name":"Vecchia","version":"1.9.0"}"#,
        );
        let v2 = ud.join(format!("Default/Extensions/{ID_A}/1.10.0_0"));
        put(
            &v2.join("manifest.json"),
            "\u{feff}{\"name\":\"__MSG_appName__\",\"version\":\"1.10.0\",\"default_locale\":\"en\",\"update_url\":\"https://clients2.google.com/service/update2/crx\"}",
        );
        put(
            &v2.join("_locales/en/messages.json"),
            r#"{"APPNAME":{"message":"Blocca pubblicità"}}"#,
        );
        // B: nel secondo profilo, tema, senza update_url.
        put(
            &ud.join(format!("Profile 1/Extensions/{ID_B}/2.0_0/manifest.json")),
            r#"{"name":"Tema blu","version":"2.0","theme":{}}"#,
        );
        // Spazzatura ignorata.
        put(&ud.join("Default/Extensions/Temp/x/manifest.json"), "{}");
        put(
            &ud.join(format!("Default/Extensions/{ID_B}/1.0_0/manifest.json")),
            "{rotto",
        );

        let list = scan(&ud);
        assert_eq!(list.len(), 2, "{list:?}");
        let a = list.iter().find(|e| e.id == ID_A).unwrap();
        assert_eq!(a.name, "Blocca pubblicità");
        assert_eq!(a.version, "1.10.0");
        assert_eq!(a.profile, "Lorenzo (Default)");
        assert_eq!(a.profile_dir, "Default");
        assert!(a.store && !a.theme);
        let b = list.iter().find(|e| e.id == ID_B).unwrap();
        assert_eq!(b.profile, "Profile 1");
        assert!(!b.store && b.theme);
        let _ = std::fs::remove_dir_all(&ud);
    }

    #[test]
    fn missing_folder_gives_nothing() {
        assert!(scan(Path::new("/percorso/che/non/esiste")).is_empty());
    }

    #[test]
    fn unresolved_message_keeps_the_placeholder() {
        let d = tmp("msg");
        let m: Value = serde_json::from_str(r#"{"default_locale":"en"}"#).unwrap();
        assert_eq!(localized(&d, &m, "__MSG_nope__"), "__MSG_nope__");
        assert_eq!(localized(&d, &m, "Semplice"), "Semplice");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn profiles_show_account_and_validate_dirs() {
        let ud = tmp("prof");
        put(
            &ud.join("Local State"),
            r#"{"profile":{"info_cache":{"Default":{"name":"Lorenzo","user_name":"a@b.it"},"Profile 1":{"name":"Lavoro","user_name":""}}}}"#,
        );
        put(&ud.join("Default/Preferences"), "{}");
        put(&ud.join("Profile 1/Preferences"), "{}");
        put(&ud.join("Profile 2/Preferences"), "{}");
        put(&ud.join("Cache/x"), "x");
        let all = profiles(&ud, false);
        let shown: Vec<_> = all.iter().map(|p| p.shown.as_str()).collect();
        assert_eq!(
            shown,
            vec![
                "Lorenzo, a@b.it (Default)",
                "Lavoro (Profile 1)",
                "Profile 2"
            ]
        );
        assert!(profiles(&ud, true).is_empty());
        assert!(valid_profile_dir("Profile 1") && valid_profile_dir("Default"));
        assert!(!valid_profile_dir("") && !valid_profile_dir("../x") && !valid_profile_dir("a\"b"));
        let _ = std::fs::remove_dir_all(&ud);
    }
}
