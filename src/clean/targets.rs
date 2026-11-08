//! Elenco delle cose che si possono pulire (come le caselle di CCleaner).

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Group {
    Windows,
    Browser,
    App,
    Avanzate,
}

impl Group {
    pub fn title(self) -> &'static str {
        match self {
            Group::Windows => "Windows",
            Group::Browser => "Browser",
            Group::App => "Applicazioni",
            Group::Avanzate => "Avanzate (per esperti)",
        }
    }
    pub const ALL: [Group; 4] = [Group::Windows, Group::Browser, Group::App, Group::Avanzate];
}

/// Azioni che non sono semplici cancellazioni di file.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    RecycleBin,
    FlushDns,
    ComponentCleanup,
    WindowsOld,
}

#[derive(Clone, Copy, Debug)]
pub enum Item {
    /// Percorso con variabili `%NOME%` e `*` nei nomi. Una cartella viene svuotata (resta la cartella),
    /// un file viene eliminato.
    Path(&'static str),
    Action(Action),
}

#[derive(Clone, Copy, Debug)]
pub struct Target {
    pub id: &'static str,
    pub group: Group,
    pub label: &'static str,
    pub hint: &'static str,
    pub default_on: bool,
    pub admin: bool,
    /// Servizi da fermare durante la pulizia e riavviare dopo.
    pub services: &'static [&'static str],
    pub items: &'static [Item],
}

const fn t(
    id: &'static str,
    group: Group,
    label: &'static str,
    hint: &'static str,
    default_on: bool,
    admin: bool,
    items: &'static [Item],
) -> Target {
    Target {
        id,
        group,
        label,
        hint,
        default_on,
        admin,
        services: &[],
        items,
    }
}

use Group::{App, Avanzate, Browser, Windows};
use Item::{Action as Act, Path as P};

macro_rules! chromium {
    ($root:literal, $leaf:literal) => {
        &[
            P(concat!($root, "\\", $leaf, "\\*\\Cache")),
            P(concat!($root, "\\", $leaf, "\\*\\Code Cache")),
            P(concat!($root, "\\", $leaf, "\\*\\GPUCache")),
            P(concat!(
                $root,
                "\\",
                $leaf,
                "\\*\\Service Worker\\CacheStorage"
            )),
            P(concat!($root, "\\", $leaf, "\\ShaderCache")),
            P(concat!($root, "\\", $leaf, "\\GrShaderCache")),
        ]
    };
}

macro_rules! chromium_cookies {
    ($root:literal, $leaf:literal) => {
        &[
            P(concat!($root, "\\", $leaf, "\\*\\Network\\Cookies")),
            P(concat!($root, "\\", $leaf, "\\*\\Network\\Cookies-journal")),
            P(concat!($root, "\\", $leaf, "\\*\\Cookies")),
        ]
    };
}

macro_rules! chromium_history {
    ($root:literal, $leaf:literal) => {
        &[
            P(concat!($root, "\\", $leaf, "\\*\\History")),
            P(concat!($root, "\\", $leaf, "\\*\\History-journal")),
            P(concat!($root, "\\", $leaf, "\\*\\Visited Links")),
            P(concat!($root, "\\", $leaf, "\\*\\Top Sites")),
        ]
    };
}

pub const TARGETS: &[Target] = &[
    // --- Windows ---
    t("temp_user", Windows, "File temporanei dell'utente", "Cartella Temp dell'utente", true, false, &[P("%TEMP%")]),
    t("temp_win", Windows, "File temporanei di Windows", "C:\\Windows\\Temp", true, true, &[P("%SystemRoot%\\Temp")]),
    t("recycle", Windows, "Cestino", "Svuota il cestino di tutti i dischi", true, false, &[Act(Action::RecycleBin)]),
    t("thumbs", Windows, "Miniature e cache delle icone", "Si ricostruiscono da sole", true, false, &[
        P("%LOCALAPPDATA%\\Microsoft\\Windows\\Explorer\\thumbcache_*.db"),
        P("%LOCALAPPDATA%\\Microsoft\\Windows\\Explorer\\iconcache_*.db"),
        P("%LOCALAPPDATA%\\IconCache.db"),
    ]),
    t("recent", Windows, "Documenti recenti", "Collegamenti ai file aperti di recente", false, false, &[P("%APPDATA%\\Microsoft\\Windows\\Recent\\*.lnk")]),
    t("wer", Windows, "Rapporti di errore di Windows", "Segnalazioni di arresti anomali", true, false, &[
        P("%LOCALAPPDATA%\\Microsoft\\Windows\\WER"),
        P("%ProgramData%\\Microsoft\\Windows\\WER"),
    ]),
    t("dumps", Windows, "Dump di memoria", "File creati dopo un arresto anomalo", true, true, &[
        P("%LOCALAPPDATA%\\CrashDumps"),
        P("%SystemRoot%\\Minidump"),
        P("%SystemRoot%\\MEMORY.DMP"),
    ]),
    t("logs", Windows, "Registri di Windows", "File .log di CBS e installazioni", true, true, &[
        P("%SystemRoot%\\Logs\\CBS\\*.log"),
        P("%SystemRoot%\\Logs\\DISM\\*.log"),
        P("%SystemRoot%\\Panther\\*.log"),
    ]),
    Target {
        id: "wu_cache",
        group: Windows,
        label: "Download di Windows Update",
        hint: "Aggiornamenti già installati (ferma il servizio per qualche secondo)",
        default_on: true,
        admin: true,
        services: &["wuauserv", "bits"],
        items: &[P("%SystemRoot%\\SoftwareDistribution\\Download")],
    },
    Target {
        id: "delivery",
        group: Windows,
        label: "Ottimizzazione recapito",
        hint: "Cache dei download di Windows",
        default_on: true,
        admin: true,
        services: &["DoSvc"],
        items: &[P("%SystemRoot%\\ServiceProfiles\\NetworkService\\AppData\\Local\\Microsoft\\Windows\\DeliveryOptimization\\Cache")],
    },
    t("dx", Windows, "Cache degli shader (DirectX)", "Si ricrea al prossimo gioco", true, false, &[P("%LOCALAPPDATA%\\D3DSCache")]),
    t("dns", Windows, "Cache DNS", "Svuota la cache dei nomi di rete", true, false, &[Act(Action::FlushDns)]),
    // --- Browser ---
    t("chrome_cache", Browser, "Google Chrome: cache", "Chiudi Chrome per pulire tutto", true, false, chromium!("%LOCALAPPDATA%\\Google", "Chrome\\User Data")),
    t("chrome_cookies", Browser, "Google Chrome: cookie", "Dovrai riaccedere ai siti", false, false, chromium_cookies!("%LOCALAPPDATA%\\Google", "Chrome\\User Data")),
    t("chrome_history", Browser, "Google Chrome: cronologia", "Cronologia di navigazione", false, false, chromium_history!("%LOCALAPPDATA%\\Google", "Chrome\\User Data")),
    t("edge_cache", Browser, "Microsoft Edge: cache", "Chiudi Edge per pulire tutto", true, false, chromium!("%LOCALAPPDATA%\\Microsoft", "Edge\\User Data")),
    t("edge_cookies", Browser, "Microsoft Edge: cookie", "Dovrai riaccedere ai siti", false, false, chromium_cookies!("%LOCALAPPDATA%\\Microsoft", "Edge\\User Data")),
    t("edge_history", Browser, "Microsoft Edge: cronologia", "Cronologia di navigazione", false, false, chromium_history!("%LOCALAPPDATA%\\Microsoft", "Edge\\User Data")),
    t("brave_cache", Browser, "Brave: cache", "Chiudi Brave per pulire tutto", true, false, chromium!("%LOCALAPPDATA%\\BraveSoftware", "Brave-Browser\\User Data")),
    t("firefox_cache", Browser, "Firefox: cache", "Chiudi Firefox per pulire tutto", true, false, &[
        P("%LOCALAPPDATA%\\Mozilla\\Firefox\\Profiles\\*\\cache2"),
        P("%LOCALAPPDATA%\\Mozilla\\Firefox\\Profiles\\*\\startupCache"),
    ]),
    // --- Applicazioni ---
    t("discord", App, "Discord: cache", "", true, false, &[
        P("%APPDATA%\\discord\\Cache"),
        P("%APPDATA%\\discord\\Code Cache"),
        P("%APPDATA%\\discord\\GPUCache"),
    ]),
    t("spotify", App, "Spotify: cache", "", true, false, &[
        P("%LOCALAPPDATA%\\Spotify\\Storage"),
        P("%LOCALAPPDATA%\\Spotify\\Data"),
    ]),
    t("teams", App, "Microsoft Teams: cache", "", true, false, &[
        P("%APPDATA%\\Microsoft\\Teams\\Cache"),
        P("%APPDATA%\\Microsoft\\Teams\\Code Cache"),
        P("%APPDATA%\\Microsoft\\Teams\\GPUCache"),
    ]),
    t("office", App, "Microsoft Office: cache dei file", "", true, false, &[P("%LOCALAPPDATA%\\Microsoft\\Office\\16.0\\OfficeFileCache")]),
    t("vscode", App, "Visual Studio Code: cache", "", true, false, &[
        P("%APPDATA%\\Code\\Cache"),
        P("%APPDATA%\\Code\\CachedData"),
        P("%APPDATA%\\Code\\GPUCache"),
    ]),
    t("steam", App, "Steam: cache web", "", true, false, &[P("%LOCALAPPDATA%\\Steam\\htmlcache")]),
    t("nvidia", App, "NVIDIA: cache degli shader e installer", "", true, false, &[
        P("%LOCALAPPDATA%\\NVIDIA\\DXCache"),
        P("%LOCALAPPDATA%\\NVIDIA\\GLCache"),
        P("%ProgramData%\\NVIDIA Corporation\\Downloader"),
    ]),
    // --- Avanzate ---
    t("components", Avanzate, "Componenti di Windows superati (WinSxS)", "Lento (diversi minuti). Dopo non si possono disinstallare gli ultimi aggiornamenti", false, true, &[Act(Action::ComponentCleanup)]),
    t("windows_old", Avanzate, "Cartella Windows.old", "Il vecchio Windows dopo un aggiornamento di versione: non potrai tornare indietro", false, true, &[Act(Action::WindowsOld)]),
];

#[cfg(test)]
pub fn find(id: &str) -> Option<&'static Target> {
    TARGETS.iter().find(|t| t.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_are_unique_and_items_present() {
        let mut seen = HashSet::new();
        for t in TARGETS {
            assert!(seen.insert(t.id), "id doppio {}", t.id);
            assert!(!t.items.is_empty(), "{} senza voci", t.id);
            assert!(!t.label.is_empty());
        }
    }

    #[test]
    fn paths_are_safe_patterns() {
        for t in TARGETS {
            for it in t.items {
                if let Item::Path(p) = it {
                    assert!(!p.contains(".."), "{p}");
                    assert!(p.starts_with('%'), "{p} deve partire da una variabile");
                    // Mai un'intera radice: dopo la variabile ci deve essere almeno un nome.
                    assert!(
                        p.matches('\\').count() >= 1 || p.eq_ignore_ascii_case("%TEMP%"),
                        "{p}"
                    );
                }
            }
        }
    }

    #[test]
    fn dangerous_items_are_off_by_default() {
        for id in [
            "components",
            "windows_old",
            "chrome_cookies",
            "chrome_history",
            "edge_cookies",
            "edge_history",
            "recent",
        ] {
            assert!(find(id).is_some_and(|t| !t.default_on), "{id}");
        }
    }
}
