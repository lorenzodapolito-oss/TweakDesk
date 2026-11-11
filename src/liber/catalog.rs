//! Catalogo interno: solo le app comuni di cui sappiamo dove prendere l'ultima versione portabile.
//! Le altre app della cartella vengono gestite e avviate, ma non aggiornate.

pub enum Source {
    /// Ultima release di un repository GitHub; `asset` è un'espressione regolare sul nome del file.
    GitHub {
        repo: &'static str,
        asset: &'static str,
    },
}

pub struct Entry {
    pub name: &'static str,
    pub category: &'static str,
    /// Nomi con cui la cartella dell'app può chiamarsi (solo lettere e cifre minuscole).
    pub aliases: &'static [&'static str],
    /// Eseguibile principale, se non è quello col nome della cartella.
    pub exe: &'static str,
    /// File e cartelle che l'aggiornamento non deve mai toccare (oltre a Data, Settings, ecc.).
    pub keep: &'static [&'static str],
    pub source: Source,
}

macro_rules! gh {
    ($name:expr, $cat:expr, [$($al:expr),*], $exe:expr, [$($k:expr),*], $repo:expr, $asset:expr) => {
        Entry {
            name: $name,
            category: $cat,
            aliases: &[$($al),*],
            exe: $exe,
            keep: &[$($k),*],
            source: Source::GitHub { repo: $repo, asset: $asset },
        }
    };
}

pub static CATALOG: &[Entry] = &[
    gh!(
        "Notepad++",
        "Ufficio",
        ["notepad++", "notepadplusplus", "npp"],
        "notepad++.exe",
        [
            "config.xml",
            "session.xml",
            "shortcuts.xml",
            "stylers.xml",
            "contextMenu.xml",
            "plugins",
            "userDefineLangs",
            "themes",
            "backup",
            "doLocalConf.xml"
        ],
        "notepad-plus-plus/notepad-plus-plus",
        r"(?i)^npp\..*\.portable\.x64\.zip$"
    ),
    gh!(
        "KeePassXC",
        "Sicurezza",
        ["keepassxc"],
        "KeePassXC.exe",
        ["keepassxc.ini"],
        "keepassxreboot/keepassxc",
        r"(?i)^KeePassXC-[\d.]+-Win64\.zip$"
    ),
    gh!(
        "ShareX",
        "Immagini",
        ["sharex"],
        "ShareX.exe",
        ["ShareX"],
        "ShareX/ShareX",
        r"(?i)^ShareX-[\d.]+-portable\.zip$"
    ),
    gh!(
        "Rufus",
        "Sistema",
        ["rufus"],
        "",
        ["rufus.ini"],
        "pbatard/rufus",
        r"(?i)^rufus-[\d.]+\.exe$"
    ),
    gh!(
        "OBS Studio",
        "Multimedia",
        ["obs", "obsstudio"],
        "bin\\64bit\\obs64.exe",
        ["config", "basic"],
        "obsproject/obs-studio",
        r"(?i)^OBS-Studio-[\d.]+-Windows(-x64)?\.zip$"
    ),
    gh!(
        "yt-dlp",
        "Multimedia",
        ["ytdlp"],
        "yt-dlp.exe",
        ["yt-dlp.conf"],
        "yt-dlp/yt-dlp",
        r"^yt-dlp\.exe$"
    ),
    gh!(
        "Bulk Crap Uninstaller",
        "Sistema",
        ["bcuninstaller", "bulkcrapuninstaller"],
        "BCUninstaller.exe",
        [],
        "Klocman/Bulk-Crap-Uninstaller",
        r"(?i)^BCUninstaller_[\d.]+_portable\.zip$"
    ),
    gh!(
        "DB Browser for SQLite",
        "Sviluppo",
        ["dbbrowserforsqlite", "sqlitebrowser", "dbbrowser"],
        "DB Browser for SQLite.exe",
        [],
        "sqlitebrowser/sqlitebrowser",
        r"(?i)^DB\.Browser\.for\.SQLite-v[\d.]+-win64\.zip$"
    ),
    gh!(
        "Audacity",
        "Multimedia",
        ["audacity"],
        "Audacity.exe",
        ["Portable Settings"],
        "audacity/audacity",
        r"(?i)^audacity-win-[\d.]+-x86_64\.7z$"
    ),
];

/// Solo lettere e cifre minuscole: per confrontare i nomi delle cartelle.
pub fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || *c == '+')
        .flat_map(char::to_lowercase)
        .collect()
}

/// Voce del catalogo per il nome di una cartella: uguale a un alias, oppure alias seguito dalla
/// versione (`Rufus 2.11`, `KeePassXC-2.7.11-Win64`, `qbittorrent_4.6.3_x64`).
pub fn find_for_folder(folder: &str) -> Option<&'static Entry> {
    let n = normalize(folder);
    if n.is_empty() {
        return None;
    }
    CATALOG.iter().find(|e| {
        e.aliases.iter().any(|a| {
            n == *a
                || n.strip_prefix(a)
                    .is_some_and(|rest| rest.chars().next().is_some_and(|c| c.is_ascii_digit()))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    #[test]
    fn every_pattern_compiles_and_ids_are_unique() {
        let mut names = std::collections::HashSet::new();
        for e in CATALOG {
            assert!(names.insert(e.name), "nome doppio {}", e.name);
            assert!(!e.aliases.is_empty());
            let Source::GitHub { repo, asset } = &e.source;
            assert!(repo.contains('/'));
            assert!(Regex::new(asset).is_ok(), "{}", e.name);
            for a in e.aliases {
                assert_eq!(*a, normalize(a));
            }
        }
    }

    #[test]
    fn folder_names_are_matched() {
        assert_eq!(
            find_for_folder("Notepad++").map(|e| e.name),
            Some("Notepad++")
        );
        assert!(find_for_folder("Notepad2").is_none());
        assert!(find_for_folder("Notepad").is_none());
        assert_eq!(
            find_for_folder("KeePassXC-2.7.11-Win64").map(|e| e.name),
            Some("KeePassXC")
        );
        assert_eq!(find_for_folder("Rufus 2.11").map(|e| e.name), Some("Rufus"));
        assert!(find_for_folder("OBSPortable").is_none());
        assert_eq!(
            find_for_folder("KeePassXC 2").map(|e| e.name),
            Some("KeePassXC")
        );
        assert_eq!(
            find_for_folder("DB Browser for SQLite").map(|e| e.name),
            Some("DB Browser for SQLite")
        );
        assert_eq!(find_for_folder("Rufus4").map(|e| e.name), Some("Rufus"));
        assert!(find_for_folder("Sconosciuta").is_none());
        assert!(find_for_folder("rufusxyz").is_none());
        assert!(find_for_folder("").is_none());
    }

    #[test]
    fn asset_patterns_match_real_looking_names() {
        let m = |folder: &str, name: &str| {
            find_for_folder(folder)
                .map(|e| {
                    let Source::GitHub { asset, .. } = &e.source;
                    Regex::new(asset).map(|r| r.is_match(name)).unwrap_or(false)
                })
                .unwrap_or(false)
        };
        assert!(m("notepad++", "npp.8.7.1.portable.x64.zip"));
        assert!(!m("notepad++", "npp.8.7.1.Installer.x64.exe"));
        assert!(m("keepassxc", "KeePassXC-2.7.9-Win64.zip"));
        assert!(!m("keepassxc", "KeePassXC-2.7.9-Win64.msi"));
        assert!(m("sharex", "ShareX-16.1.0-portable.zip"));
        assert!(m("rufus", "rufus-4.5.exe"));
        assert!(!m("rufus", "rufus-4.5p.exe"));
        assert!(m("ytdlp", "yt-dlp.exe"));
        assert!(m("audacity", "audacity-win-4.0.2-x86_64.7z"));
        assert!(!m("audacity", "audacity-win-4.0.2-arm64.7z"));
        assert!(!m("audacity", "audacity-win-4.0.2-x86_64.msi"));
    }
}
