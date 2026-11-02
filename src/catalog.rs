//! Catalogo delle impostazioni note, con il valore di fabbrica di Windows 11.

use crate::model::{Baseline, Entry, Risk, Value};

struct Cat {
    v: Vec<Entry>,
    group: &'static str,
    risk: Risk,
}

impl Cat {
    fn push(&mut self, label: &str, key: &str, name: &str, value: Value, baseline: Baseline) {
        self.v.push(Entry {
            group: self.group.into(),
            label: label.into(),
            key: key.into(),
            name: name.into(),
            value,
            baseline,
            risk: self.risk,
        });
    }

    /// `base`: valore di fabbrica; `None` = il valore di fabbrica non esiste.
    fn dw(&mut self, label: &str, key: &str, name: &str, new: u32, base: Option<u32>) {
        let b = base.map_or(Baseline::Absent, |n| Baseline::Value(Value::Dword(n)));
        self.push(label, key, name, Value::Dword(new), b);
    }

    fn sz(&mut self, label: &str, key: &str, name: &str, new: &str, base: Option<&str>) {
        let b = base.map_or(Baseline::Absent, |s| Baseline::Value(Value::Sz(s.into())));
        self.push(label, key, name, Value::Sz(new.into()), b);
    }

    fn group(&mut self, group: &'static str, risk: Risk) {
        self.group = group;
        self.risk = risk;
    }
}

const ADV: &str = "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Advanced";
const CV: &str = "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion";
const POL: &str = "HKLM\\SOFTWARE\\Policies\\Microsoft";
const CCS: &str = "HKLM\\SYSTEM\\CurrentControlSet\\Control";

pub fn catalog() -> Vec<Entry> {
    let mut c = Cat {
        v: Vec::new(),
        group: "",
        risk: Risk::Normal,
    };

    c.group("Explorer e aspetto", Risk::Normal);
    c.dw("Mostra file e cartelle nascosti", ADV, "Hidden", 1, Some(2));
    c.dw(
        "Mostra le estensioni dei file",
        ADV,
        "HideFileExt",
        0,
        Some(1),
    );
    c.dw(
        "Esplora risorse si apre su Questo PC",
        ADV,
        "LaunchTo",
        1,
        Some(3),
    );
    c.dw(
        "Modalità compatta in Esplora risorse",
        ADV,
        "UseCompactMode",
        1,
        Some(0),
    );
    c.dw("Icone della barra a sinistra", ADV, "TaskbarAl", 0, Some(1));
    c.dw(
        "Nascondi il pulsante Visualizzazione attività",
        ADV,
        "ShowTaskViewButton",
        0,
        Some(1),
    );
    c.dw("Nascondi Chat nella barra", ADV, "TaskbarMn", 0, Some(1));
    c.dw(
        "Secondi nell'orologio",
        ADV,
        "ShowSecondsInSystemClock",
        1,
        Some(0),
    );
    c.dw(
        "Animazioni della barra disattivate",
        ADV,
        "TaskbarAnimations",
        0,
        Some(1),
    );
    c.dw(
        "Snap Assist disattivato",
        ADV,
        "EnableSnapAssistFlyout",
        0,
        Some(1),
    );
    c.dw("Barra Snap disattivata", ADV, "EnableSnapBar", 0, Some(1));
    c.dw(
        "Anteprima desktop (Peek) disattivata",
        ADV,
        "DisablePreviewDesktop",
        1,
        Some(0),
    );
    c.dw(
        "Niente notifiche dei provider di sincronizzazione",
        ADV,
        "ShowSyncProviderNotifications",
        0,
        Some(1),
    );
    c.dw(
        "Non tenere traccia dei programmi avviati",
        ADV,
        "Start_TrackProgs",
        0,
        Some(1),
    );
    c.dw(
        "Casella di ricerca nascosta nella barra",
        &format!("{CV}\\Search"),
        "SearchboxTaskbarMode",
        0,
        Some(2),
    );
    c.dw(
        "Windows Peek (DWM) disattivato",
        "HKCU\\Software\\Microsoft\\Windows\\DWM",
        "EnableAeroPeek",
        0,
        Some(1),
    );
    c.dw(
        "Tema scuro per il sistema",
        &format!("{CV}\\Themes\\Personalize"),
        "SystemUsesLightTheme",
        0,
        Some(1),
    );
    c.sz(
        "Menu contestuale classico (stile Windows 10)",
        "HKCU\\Software\\Classes\\CLSID\\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}\\InprocServer32",
        "",
        "",
        None,
    );
    let gb = "HKCU\\Software\\Microsoft\\GameBar";
    c.dw(
        "Game Bar: nessun pannello all'avvio",
        gb,
        "ShowStartupPanel",
        0,
        Some(1),
    );
    c.dw(
        "Game Bar: non aprire con il pulsante del controller",
        gb,
        "UseNexusForGameBarEnabled",
        0,
        Some(1),
    );
    c.dw(
        "Game Bar: scorciatoia del controller disattivata",
        gb,
        "GamepadNexusChordEnabled",
        0,
        Some(1),
    );
    c.dw(
        "Modalità gioco: niente notifiche",
        gb,
        "ShowGameModeNotifications",
        0,
        Some(1),
    );

    c.group("Privacy e suggerimenti", Risk::Normal);
    c.dw(
        "ID pubblicità disattivato",
        &format!("{CV}\\AdvertisingInfo"),
        "Enabled",
        0,
        Some(1),
    );
    c.dw(
        "Esperienze personalizzate dai dati diagnostici disattivate",
        &format!("{CV}\\Privacy"),
        "TailoredExperiencesWithDiagnosticDataEnabled",
        0,
        Some(1),
    );
    let cdm = format!("{CV}\\ContentDeliveryManager");
    for (name, label) in [
        (
            "FeatureManagementEnabled",
            "Gestione funzionalità dei contenuti",
        ),
        ("PreInstalledAppsEnabled", "App preinstallate"),
        ("RotatingLockScreenEnabled", "Schermata di blocco Spotlight"),
        (
            "RotatingLockScreenOverlayEnabled",
            "Suggerimenti sulla schermata di blocco",
        ),
        (
            "SilentInstalledAppsEnabled",
            "Installazione silenziosa di app suggerite",
        ),
        ("SoftLandingEnabled", "Suggerimenti di benvenuto"),
        (
            "SystemPaneSuggestionsEnabled",
            "Suggerimenti nel menu Start",
        ),
    ] {
        c.dw(&format!("Disattiva: {label}"), &cdm, name, 0, Some(1));
    }
    for id in [
        "338388", "338389", "338393", "353694", "353696", "310093", "314563", "353698",
    ] {
        c.dw(
            &format!("Disattiva contenuti suggeriti {id}"),
            &cdm,
            &format!("SubscribedContent-{id}Enabled"),
            0,
            Some(1),
        );
    }
    c.dw(
        "Notifiche sulla schermata di blocco disattivate",
        &format!("{CV}\\PushNotifications"),
        "LockScreenToastEnabled",
        0,
        Some(1),
    );
    c.dw(
        "Avviso app all'avvio disattivato",
        &format!("{CV}\\Notifications\\Settings\\Windows.SystemToast.StartupApp"),
        "Enabled",
        0,
        Some(1),
    );
    let ip = "HKCU\\Software\\Microsoft\\InputPersonalization";
    c.dw(
        "Raccolta dati di inchiostro limitata",
        ip,
        "RestrictImplicitInkCollection",
        1,
        Some(0),
    );
    c.dw(
        "Raccolta dati di scrittura limitata",
        ip,
        "RestrictImplicitTextCollection",
        1,
        Some(0),
    );
    c.dw(
        "Non raccogliere i contatti",
        &format!("{ip}\\TrainedDataStore"),
        "HarvestContacts",
        0,
        Some(1),
    );
    c.dw(
        "Pannello di input non precaricato",
        "HKCU\\Software\\Microsoft\\input",
        "IsInputAppPreloadEnabled",
        0,
        Some(1),
    );
    c.dw(
        "Suggerimenti di input (TIPC) disattivati",
        "HKCU\\Software\\Microsoft\\input\\TIPC",
        "Enabled",
        0,
        Some(1),
    );
    let up = "HKCU\\Control Panel\\International\\User Profile";
    c.dw(
        "Correzione automatica disattivata",
        up,
        "ShowAutoCorrection",
        0,
        Some(1),
    );
    c.dw(
        "Previsione del testo disattivata",
        up,
        "ShowTextPrediction",
        0,
        Some(1),
    );

    c.group("Reattività e tastiera", Risk::Normal);
    let d = "HKCU\\Control Panel\\Desktop";
    c.sz("Menu senza ritardo", d, "MenuShowDelay", "0", Some("400"));
    c.sz(
        "Chiudi subito le app bloccate",
        d,
        "AutoEndTasks",
        "1",
        Some("0"),
    );
    c.sz(
        "Timeout app bloccata: 2,5 secondi",
        d,
        "HungAppTimeout",
        "2500",
        Some("5000"),
    );
    c.sz(
        "Attesa chiusura app: 2,5 secondi",
        d,
        "WaitToKillAppTimeout",
        "2500",
        Some("5000"),
    );
    c.sz(
        "Timeout hook di basso livello: 1 secondo",
        d,
        "LowLevelHooksTimeout",
        "1000",
        None,
    );
    c.sz(
        "Ritardo di ripetizione tastiera minimo",
        "HKCU\\Control Panel\\Keyboard",
        "KeyboardDelay",
        "0",
        Some("1"),
    );
    c.dw(
        "Tasto Stamp senza Cattura schermo",
        "HKCU\\Control Panel\\Keyboard",
        "PrintScreenKeyForSnippingEnabled",
        0,
        Some(1),
    );

    c.group("Criteri e telemetria", Risk::Normal);
    c.dw(
        "Notizie e interessi disattivati",
        &format!("{POL}\\Dsh"),
        "AllowNewsAndInterests",
        0,
        None,
    );
    c.dw(
        "Edge: niente precaricamento",
        &format!("{POL}\\MicrosoftEdge\\Main"),
        "AllowPrelaunch",
        0,
        None,
    );
    c.dw(
        "Strumento rimozione malware non offerto da Windows Update",
        &format!("{POL}\\MRT"),
        "DontOfferThroughWUAU",
        1,
        None,
    );
    c.dw(
        "Programma miglioramento esperienza (CEIP) disattivato",
        &format!("{POL}\\SQMClient\\Windows"),
        "CEIPEnable",
        0,
        None,
    );
    c.dw(
        "Funzionalità consumer di Windows disattivate",
        &format!("{POL}\\Windows\\CloudContent"),
        "DisableWindowsConsumerFeatures",
        1,
        None,
    );
    c.dw(
        "Telemetria al minimo",
        &format!("{POL}\\Windows\\DataCollection"),
        "AllowTelemetry",
        0,
        None,
    );
    c.dw(
        "Ottimizzazione recapito: solo download diretto",
        &format!("{POL}\\Windows\\DeliveryOptimization"),
        "DODownloadMode",
        0,
        None,
    );
    c.dw(
        "Non cercare nello Store con «Apri con»",
        &format!("{POL}\\Windows\\Explorer"),
        "NoUseStoreOpenWith",
        1,
        None,
    );
    c.dw(
        "Sfondo senza effetto acrilico all'accesso",
        &format!("{POL}\\Windows\\System"),
        "DisableAcrylicBackgroundOnLogon",
        1,
        None,
    );
    c.dw(
        "Segnalazione errori di Windows disattivata",
        &format!("{POL}\\Windows\\Windows Error Reporting"),
        "Disabled",
        1,
        None,
    );
    c.dw(
        "Spazio di lavoro Windows Ink disattivato",
        &format!("{POL}\\WindowsInkWorkspace"),
        "AllowWindowsInkWorkspace",
        0,
        None,
    );
    let pe = format!("{CV}\\Policies\\Explorer");
    for (name, label) in [
        (
            "NoDriveTypeAutoRun",
            "Esecuzione automatica dei dischi disattivata",
        ),
        ("HideSCAMeetNow", "Nascondi «Riunione in corso»"),
        (
            "NoLowDiskSpaceChecks",
            "Nessun avviso di spazio su disco insufficiente",
        ),
        (
            "LinkResolveIgnoreLinkInfo",
            "Collegamenti: ignora le informazioni di collegamento",
        ),
        (
            "NoResolveSearch",
            "Collegamenti: non cercare i file mancanti",
        ),
        (
            "NoResolveTrack",
            "Collegamenti: non tracciare i file spostati",
        ),
        ("NoInternetOpenWith", "«Apri con»: non cercare su Internet"),
    ] {
        c.dw(label, &pe, name, 1, None);
    }
    c.dw(
        "Edge (utente): niente precaricamento",
        "HKCU\\Software\\Policies\\Microsoft\\MicrosoftEdge\\Main",
        "AllowPrelaunch",
        0,
        None,
    );
    c.dw(
        "Pianificazione pacchetti: nessun limite riservato",
        "HKCU\\Software\\Policies\\Microsoft\\Windows\\Psched",
        "NonBestEfforLimit",
        0,
        None,
    );
    c.dw(
        "Segnalazione errori (utente): log disattivato",
        "HKCU\\Software\\Policies\\Microsoft\\Windows\\Windows Error Reporting",
        "LoggingDisabled",
        1,
        None,
    );

    c.group("Energia e disco", Risk::Caution);
    c.dw(
        "Avvio rapido disattivato",
        &format!("{CCS}\\Session Manager\\Power"),
        "HiberbootEnabled",
        0,
        Some(1),
    );
    c.dw(
        "Nomi 8.3 NTFS disattivati",
        &format!("{CCS}\\FileSystem"),
        "NtfsDisable8dot3NameCreation",
        1,
        Some(2),
    );
    c.dw(
        "Kernel sempre in memoria (DisablePagingExecutive)",
        &format!("{CCS}\\Session Manager\\Memory Management"),
        "DisablePagingExecutive",
        1,
        Some(0),
    );
    c.dw(
        "Nessun dump di memoria dopo un errore grave",
        &format!("{CCS}\\CrashControl"),
        "CrashDumpEnabled",
        0,
        Some(7),
    );

    c.group("Aggiornamenti Windows", Risk::Security);
    let wu = format!("{POL}\\Windows\\WindowsUpdate");
    c.dw(
        "Blocca la versione di Windows",
        &wu,
        "TargetReleaseVersion",
        1,
        None,
    );
    c.sz(
        "Prodotto bloccato",
        &wu,
        "ProductVersion",
        "Windows 11",
        None,
    );
    c.sz(
        "Versione bloccata",
        &wu,
        "TargetReleaseVersionInfo",
        "24H2",
        None,
    );
    c.dw(
        "Nega l'accesso a Windows Update",
        &wu,
        "DisableWindowsUpdateAccess",
        1,
        None,
    );
    c.dw(
        "Aggiornamenti automatici disattivati",
        &format!("{wu}\\AU"),
        "NoAutoUpdate",
        1,
        None,
    );

    c.group("Sicurezza ridotta", Risk::Security);
    c.dw(
        "SmartScreen disattivato",
        &format!("{POL}\\Windows\\System"),
        "EnableSmartScreen",
        0,
        None,
    );
    c.dw(
        "Controllo firme Authenticode disattivato",
        &format!("{POL}\\Windows\\safer\\codeidentifiers"),
        "authenticodeenabled",
        0,
        None,
    );
    c.dw(
        "Notifiche di Sicurezza di Windows disattivate",
        &format!("{POL}\\Windows Defender Security Center\\Notifications"),
        "DisableNotifications",
        1,
        None,
    );
    let ap = "HKCU\\Software\\Microsoft\\Windows Defender Security Center\\Account protection";
    c.dw(
        "Protezione account: notifiche disattivate",
        ap,
        "DisableNotifications",
        1,
        None,
    );
    c.dw(
        "Protezione account: notifiche Windows Hello disattivate",
        ap,
        "DisableWindowsHelloNotifications",
        1,
        None,
    );
    c.dw(
        "Protezione account: notifiche Blocco dinamico disattivate",
        ap,
        "DisableDynamiclockNotifications",
        1,
        None,
    );
    c.dw(
        "File scaricati senza contrassegno «dal web»",
        &format!("{CV}\\Policies\\Attachments"),
        "SaveZoneInformation",
        1,
        None,
    );
    let mm = format!("{CCS}\\Session Manager\\Memory Management");
    c.dw(
        "Mitigazioni Spectre/Meltdown disattivate (override)",
        &mm,
        "FeatureSettingsOverride",
        3,
        None,
    );
    c.dw(
        "Mitigazioni Spectre/Meltdown disattivate (maschera)",
        &mm,
        "FeatureSettingsOverrideMask",
        3,
        None,
    );

    c.v
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn identities_are_unique_and_keys_valid() {
        let cat = catalog();
        assert!(cat.len() > 90);
        let mut seen = HashSet::new();
        for e in &cat {
            assert!(crate::model::valid_key(&e.key), "{}", e.key);
            assert!(seen.insert(e.ident()), "duplicato: {} {}", e.key, e.name);
            assert!(!e.label.is_empty() && !e.group.is_empty());
        }
    }

    #[test]
    fn new_value_differs_from_factory_value() {
        for e in catalog() {
            if let Baseline::Value(b) = &e.baseline {
                assert_ne!(b, &e.value, "{} {}", e.key, e.name);
                assert_eq!(
                    std::mem::discriminant(b),
                    std::mem::discriminant(&e.value),
                    "{}",
                    e.label
                );
            }
        }
    }

    #[test]
    fn risky_groups_are_flagged() {
        let cat = catalog();
        let sec = |g: &str| {
            cat.iter()
                .filter(|e| e.group == g)
                .all(|e| e.risk == Risk::Security)
        };
        assert!(sec("Aggiornamenti Windows") && sec("Sicurezza ridotta"));
        assert!(cat
            .iter()
            .any(|e| e.group == "Energia e disco" && e.risk == Risk::Caution));
        assert!(cat
            .iter()
            .filter(|e| e.group == "Explorer e aspetto")
            .all(|e| e.risk == Risk::Normal));
    }
}
