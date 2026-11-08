//! Elenco delle voci (scritto da zero): cosa si può togliere o cambiare, con livello e avviso.

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Level {
    Sicuro,
    Standard,
    Estremo,
}

impl Level {
    pub fn title(self) -> &'static str {
        match self {
            Level::Sicuro => "Sicuro",
            Level::Standard => "Standard",
            Level::Estremo => "Estremo",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Group {
    Apps,
    Privacy,
    Interfaccia,
    Prestazioni,
    Servizi,
    Componenti,
    Pulizia,
}

impl Group {
    pub const ALL: [Group; 7] = [
        Group::Apps,
        Group::Privacy,
        Group::Interfaccia,
        Group::Prestazioni,
        Group::Servizi,
        Group::Componenti,
        Group::Pulizia,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Group::Apps => "App preinstallate",
            Group::Privacy => "Privacy e telemetria",
            Group::Interfaccia => "Interfaccia",
            Group::Prestazioni => "Prestazioni",
            Group::Servizi => "Servizi e attività pianificate",
            Group::Componenti => "Componenti di Windows",
            Group::Pulizia => "Pulizia dell'immagine",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
    Both,
    IsoOnly,
    OnlineOnly,
}

#[derive(Clone, Copy, Debug)]
pub enum RegKind {
    Dword,
    Sz,
}

#[derive(Clone, Copy, Debug)]
pub enum Act {
    /// App: prefisso del nome del pacchetto (per esempio `Microsoft.BingNews`).
    Appx(&'static str),
    /// Valore di registro. `key` comincia con HKCU\ o HKLM\. Nome vuoto = valore predefinito.
    Reg {
        key: &'static str,
        name: &'static str,
        kind: RegKind,
        data: &'static str,
    },
    /// Elimina un valore di registro.
    RegDel {
        key: &'static str,
        name: &'static str,
    },
    /// Servizio: tipo di avvio (2 automatico, 3 manuale, 4 disattivato).
    Svc { name: &'static str, start: u8 },
    /// Attività pianificata (percorso come in Utilità di pianificazione).
    Task(&'static str),
    /// Funzionalità facoltativa di Windows (DISM /Disable-Feature).
    Feature(&'static str),
    /// Funzionalità su richiesta: prefisso del nome (DISM /Remove-Capability).
    Capability(&'static str),
    /// File o cartella (relativo alla cartella di Windows installato, `*` ammesso nell'ultimo nome).
    Delete(&'static str),
    /// Comando (solo sul sistema installato).
    Cmd(&'static str, &'static [&'static str]),
    /// Pulizia dei componenti con DISM (/StartComponentCleanup /ResetBase).
    ResetBase,
}

#[derive(Clone, Copy, Debug)]
pub struct Item {
    pub id: &'static str,
    pub group: Group,
    pub level: Level,
    pub label: &'static str,
    pub warn: &'static str,
    pub scope: Scope,
    pub acts: &'static [Act],
}

use Act::{Appx, Capability, Cmd, Delete, Feature, Reg, RegDel, ResetBase, Svc, Task};
use Group::{Apps, Componenti, Interfaccia, Prestazioni, Privacy, Pulizia, Servizi};
use Level::{Estremo, Sicuro, Standard};
use RegKind::{Dword, Sz};
use Scope::{Both, IsoOnly, OnlineOnly};

const fn it(
    id: &'static str,
    group: Group,
    level: Level,
    label: &'static str,
    warn: &'static str,
    scope: Scope,
    acts: &'static [Act],
) -> Item {
    Item {
        id,
        group,
        level,
        label,
        warn,
        scope,
        acts,
    }
}

macro_rules! app {
    ($id:expr, $level:expr, $label:expr, $prefix:expr) => {
        it($id, Apps, $level, $label, "", Both, &[Appx($prefix)])
    };
}

const fn d(key: &'static str, name: &'static str, data: &'static str) -> Act {
    Reg {
        key,
        name,
        kind: Dword,
        data,
    }
}

const CDM: &str = "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\ContentDeliveryManager";
const ADV: &str = "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Advanced";
const POL_SYS: &str = "HKLM\\SOFTWARE\\Policies\\Microsoft\\Windows\\System";
const WD_POL: &str = "HKLM\\SOFTWARE\\Policies\\Microsoft\\Windows Defender";
const WU_AU: &str = "HKLM\\SOFTWARE\\Policies\\Microsoft\\Windows\\WindowsUpdate\\AU";
const PERS: &str = "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize";

pub const ITEMS: &[Item] = &[
    // ---------- App: Sicuro ----------
    app!("xbox", Sicuro, "Xbox e Game Bar", "Microsoft.GamingApp"),
    it("xbox_ov", Apps, Sicuro, "Xbox: sovrapposizioni e servizi dell'app", "", Both, &[
        Appx("Microsoft.XboxGamingOverlay"),
        Appx("Microsoft.XboxGameOverlay"),
        Appx("Microsoft.XboxIdentityProvider"),
        Appx("Microsoft.XboxSpeechToTextOverlay"),
        Appx("Microsoft.Xbox.TCUI"),
    ]),
    it("teams", Apps, Sicuro, "Microsoft Teams", "", Both, &[Appx("MicrosoftTeams"), Appx("MSTeams")]),
    app!("news", Sicuro, "Notizie", "Microsoft.BingNews"),
    app!("weather", Sicuro, "Meteo", "Microsoft.BingWeather"),
    app!("solitaire", Sicuro, "Solitario e giochi Microsoft", "Microsoft.MicrosoftSolitaireCollection"),
    app!("clipchamp", Sicuro, "Clipchamp", "Clipchamp.Clipchamp"),
    app!("feedback", Sicuro, "Hub di Feedback", "Microsoft.WindowsFeedbackHub"),
    app!("maps", Sicuro, "Mappe", "Microsoft.WindowsMaps"),
    app!("movies", Sicuro, "Film e TV", "Microsoft.ZuneVideo"),
    app!("groove", Sicuro, "Groove Musica / Media Player nuovo", "Microsoft.ZuneMusic"),
    it("phone", Apps, Sicuro, "Collegamento al telefono", "", Both, &[Appx("Microsoft.YourPhone"), Appx("MicrosoftWindows.CrossDevice")]),
    app!("mr", Sicuro, "Mixed Reality Portal", "Microsoft.MixedReality.Portal"),
    app!("pad", Sicuro, "Power Automate", "Microsoft.PowerAutomateDesktop"),
    app!("devhome", Sicuro, "Dev Home", "Microsoft.Windows.DevHome"),
    app!("linkedin", Sicuro, "LinkedIn", "7EE7776C.LinkedInforWindows"),
    app!("todo", Sicuro, "Microsoft To Do", "Microsoft.Todos"),
    it("promo", Apps, Sicuro, "App pubblicitarie (Spotify, Disney+, Prime Video, TikTok, Instagram)", "", Both, &[
        Appx("SpotifyAB.SpotifyMusic"),
        Appx("Disney.37853FC22B2CE"),
        Appx("AmazonVideo.PrimeVideo"),
        Appx("BytedancePte.Ltd.TikTok"),
        Appx("Facebook.InstagramBeta"),
        Appx("Facebook.Facebook"),
    ]),
    // ---------- App: Standard ----------
    app!("copilot", Standard, "Copilot", "Microsoft.Copilot"),
    app!("outlook", Standard, "Outlook (nuovo)", "Microsoft.OutlookForWindows"),
    app!("sticky", Standard, "Sticky Notes", "Microsoft.MicrosoftStickyNotes"),
    app!("recorder", Standard, "Registratore vocale", "Microsoft.WindowsSoundRecorder"),
    app!("cortana", Standard, "Cortana", "Microsoft.549981C3F5F10"),
    it("widgets", Apps, Standard, "Widget", "", Both, &[Appx("MicrosoftWindows.Client.WebExperience")]),
    app!("gethelp", Standard, "Richiedi assistenza", "Microsoft.GetHelp"),
    app!("tips", Standard, "Suggerimenti", "Microsoft.Getstarted"),
    app!("officehub", Standard, "Hub di Microsoft 365", "Microsoft.MicrosoftOfficeHub"),
    app!("people", Standard, "Contatti", "Microsoft.People"),
    app!("journal", Standard, "Journal", "Microsoft.MicrosoftJournal"),
    // ---------- App: Estremo ----------
    it("store", Apps, Estremo, "Microsoft Store", "Senza Store non potrai installare app da lì. Anche winget può smettere di funzionare.", Both, &[
        Appx("Microsoft.WindowsStore"),
        Appx("Microsoft.StorePurchaseApp"),
        Appx("Microsoft.Services.Store.Engagement"),
    ]),
    it("winget", Apps, Estremo, "Programma di installazione app (winget)", "Perdi il comando winget e l'installazione dei programmi da riga di comando.", Both, &[Appx("Microsoft.DesktopAppInstaller")]),
    app!("calc", Estremo, "Calcolatrice", "Microsoft.WindowsCalculator"),
    app!("photos", Estremo, "Foto", "Microsoft.Windows.Photos"),
    app!("camera", Estremo, "Fotocamera", "Microsoft.WindowsCamera"),
    app!("snip", Estremo, "Strumento di cattura", "Microsoft.ScreenSketch"),
    app!("paint", Estremo, "Paint", "Microsoft.Paint"),
    app!("notepad", Estremo, "Blocco note (app)", "Microsoft.WindowsNotepad"),
    app!("terminal", Estremo, "Terminale Windows", "Microsoft.WindowsTerminal"),
    it("sechealth", Apps, Estremo, "Sicurezza di Windows (interfaccia)", "Senza l'interfaccia non puoi più gestire Defender dalle Impostazioni.", Both, &[Appx("Microsoft.SecHealthUI")]),
    // ---------- Privacy: Sicuro ----------
    it("ad_id", Privacy, Sicuro, "ID pubblicitario spento", "", Both, &[d("HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\AdvertisingInfo", "Enabled", "0")]),
    it("suggest", Privacy, Sicuro, "Suggerimenti, consigli e app promosse nel menu Start", "", Both, &[
        d(CDM, "SubscribedContent-338388Enabled", "0"),
        d(CDM, "SubscribedContent-338389Enabled", "0"),
        d(CDM, "SubscribedContent-338387Enabled", "0"),
        d(CDM, "SubscribedContent-353694Enabled", "0"),
        d(CDM, "SubscribedContent-353696Enabled", "0"),
        d(CDM, "SubscribedContent-338393Enabled", "0"),
        d(CDM, "SubscribedContent-310093Enabled", "0"),
        d(CDM, "SystemPaneSuggestionsEnabled", "0"),
        d(CDM, "SilentInstalledAppsEnabled", "0"),
        d(CDM, "OemPreInstalledAppsEnabled", "0"),
        d(CDM, "PreInstalledAppsEnabled", "0"),
        d(CDM, "ContentDeliveryAllowed", "0"),
        d(CDM, "SoftLandingEnabled", "0"),
        d(CDM, "RotatingLockScreenOverlayEnabled", "0"),
    ]),
    it("bing", Privacy, Sicuro, "Ricerca web (Bing) nel menu Start spenta", "", Both, &[
        d("HKCU\\Software\\Policies\\Microsoft\\Windows\\Explorer", "DisableSearchBoxSuggestions", "1"),
        d("HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Search", "BingSearchEnabled", "0"),
    ]),
    it("telemetry", Privacy, Sicuro, "Telemetria al minimo", "", Both, &[
        d("HKLM\\SOFTWARE\\Policies\\Microsoft\\Windows\\DataCollection", "AllowTelemetry", "0"),
        d("HKLM\\SOFTWARE\\Policies\\Microsoft\\Windows\\DataCollection", "DoNotShowFeedbackNotifications", "1"),
    ]),
    it("wer", Privacy, Sicuro, "Segnalazione errori spenta", "", Both, &[d("HKLM\\SOFTWARE\\Microsoft\\Windows\\Windows Error Reporting", "Disabled", "1")]),
    // ---------- Privacy: Standard ----------
    it("activity", Privacy, Standard, "Cronologia attività spenta", "", Both, &[
        d(POL_SYS, "EnableActivityFeed", "0"),
        d(POL_SYS, "PublishUserActivities", "0"),
        d(POL_SYS, "UploadUserActivities", "0"),
    ]),
    it("recall", Privacy, Standard, "Recall (istantanee dell'attività) spento", "", Both, &[
        d("HKLM\\SOFTWARE\\Policies\\Microsoft\\Windows\\WindowsAI", "DisableAIDataAnalysis", "1"),
        Feature("Recall"),
    ]),
    it("inking", Privacy, Standard, "Raccolta di scrittura e digitazione spenta", "", Both, &[
        d("HKCU\\Software\\Microsoft\\InputPersonalization", "RestrictImplicitTextCollection", "1"),
        d("HKCU\\Software\\Microsoft\\InputPersonalization", "RestrictImplicitInkCollection", "1"),
    ]),
    it("spotlight", Privacy, Standard, "Windows Spotlight e consigli sulla schermata di blocco", "", Both, &[
        d("HKCU\\Software\\Policies\\Microsoft\\Windows\\CloudContent", "DisableWindowsSpotlightFeatures", "1"),
        d("HKCU\\Software\\Policies\\Microsoft\\Windows\\CloudContent", "DisableWindowsConsumerFeatures", "1"),
        d("HKLM\\SOFTWARE\\Policies\\Microsoft\\Windows\\CloudContent", "DisableWindowsConsumerFeatures", "1"),
    ]),
    it("edge_nag", Privacy, Standard, "Edge: niente prima esecuzione né consigli", "", Both, &[
        d("HKLM\\SOFTWARE\\Policies\\Microsoft\\Edge", "HideFirstRunExperience", "1"),
        d("HKLM\\SOFTWARE\\Policies\\Microsoft\\Edge", "ShowRecommendationsEnabled", "0"),
    ]),
    // ---------- Interfaccia ----------
    it("ctx", Interfaccia, Sicuro, "Menu contestuale classico (tasto destro completo)", "", Both, &[Reg {
        key: "HKCU\\Software\\Classes\\CLSID\\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}\\InprocServer32",
        name: "",
        kind: Sz,
        data: "",
    }]),
    it("ext", Interfaccia, Sicuro, "Estensioni dei file visibili", "", Both, &[d(ADV, "HideFileExt", "0")]),
    it("thispc", Interfaccia, Sicuro, "Esplora risorse si apre su «Questo PC»", "", Both, &[d(ADV, "LaunchTo", "1")]),
    it("task_chat", Interfaccia, Sicuro, "Barra: senza Chat e Widget", "", Both, &[
        d(ADV, "TaskbarMn", "0"),
        d(ADV, "TaskbarDa", "0"),
        d("HKLM\\SOFTWARE\\Policies\\Microsoft\\Dsh", "AllowNewsAndInterests", "0"),
    ]),
    it("task_left", Interfaccia, Standard, "Barra delle applicazioni a sinistra", "", Both, &[d(ADV, "TaskbarAl", "0")]),
    it("start_recs", Interfaccia, Standard, "Start senza consigli e file recenti", "", Both, &[
        d(ADV, "Start_TrackDocs", "0"),
        d(ADV, "Start_IrisRecommendations", "0"),
        d("HKLM\\SOFTWARE\\Policies\\Microsoft\\Windows\\Explorer", "HideRecommendedSection", "1"),
    ]),
    it("dark", Interfaccia, Standard, "Tema scuro", "", Both, &[d(PERS, "AppsUseLightTheme", "0"), d(PERS, "SystemUsesLightTheme", "0")]),
    it("hidden", Interfaccia, Standard, "File nascosti visibili", "", Both, &[d(ADV, "Hidden", "1")]),
    // ---------- Prestazioni ----------
    it("fx", Prestazioni, Sicuro, "Effetti visivi ridotti", "", Both, &[
        d("HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\VisualEffects", "VisualFXSetting", "2"),
        d(PERS, "EnableTransparency", "0"),
        Reg { key: "HKCU\\Control Panel\\Desktop", name: "MenuShowDelay", kind: Sz, data: "0" },
    ]),
    it("startdelay", Prestazioni, Sicuro, "Nessun ritardo di avvio dei programmi", "", Both, &[d("HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Serialize", "StartupDelayInMSec", "0")]),
    it("power", Prestazioni, Sicuro, "Piano energetico «Prestazioni elevate»", "", OnlineOnly, &[Cmd("powercfg", &["/setactive", "SCHEME_MIN"])]),
    it("gamemode", Prestazioni, Standard, "Modalità gioco attiva", "", Both, &[
        d("HKCU\\Software\\Microsoft\\GameBar", "AutoGameModeEnabled", "1"),
        d("HKCU\\Software\\Microsoft\\GameBar", "AllowAutoGameMode", "1"),
    ]),
    it("hibernate", Prestazioni, Standard, "Ibernazione e avvio rapido spenti", "Libera lo spazio di hiberfil.sys. Il PC si avvia un po' più lentamente da spento.", Both, &[
        d("HKLM\\SYSTEM\\CurrentControlSet\\Control\\Power", "HibernateEnabled", "0"),
        d("HKLM\\SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Power", "HiberbootEnabled", "0"),
    ]),
    // ---------- Servizi: Standard ----------
    it("svc_telemetry", Servizi, Standard, "Servizi di telemetria (DiagTrack, dmwappushservice)", "", Both, &[
        Svc { name: "DiagTrack", start: 4 },
        Svc { name: "dmwappushservice", start: 4 },
    ]),
    it("svc_wer", Servizi, Standard, "Servizio di segnalazione errori", "", Both, &[Svc { name: "WerSvc", start: 4 }]),
    it("svc_xbox", Servizi, Standard, "Servizi Xbox", "", Both, &[
        Svc { name: "XblAuthManager", start: 4 },
        Svc { name: "XblGameSave", start: 4 },
        Svc { name: "XboxGipSvc", start: 4 },
        Svc { name: "XboxNetApiSvc", start: 4 },
    ]),
    it("svc_misc", Servizi, Standard, "Fax, mappe scaricate, demo per i negozi", "", Both, &[
        Svc { name: "Fax", start: 4 },
        Svc { name: "MapsBroker", start: 4 },
        Svc { name: "RetailDemo", start: 4 },
    ]),
    it("tasks_telemetry", Servizi, Standard, "Attività pianificate di telemetria e compatibilità", "", Both, &[
        Task("\\Microsoft\\Windows\\Application Experience\\Microsoft Compatibility Appraiser"),
        Task("\\Microsoft\\Windows\\Application Experience\\ProgramDataUpdater"),
        Task("\\Microsoft\\Windows\\Customer Experience Improvement Program\\Consolidator"),
        Task("\\Microsoft\\Windows\\Customer Experience Improvement Program\\UsbCeip"),
        Task("\\Microsoft\\Windows\\Feedback\\Siuf\\DmClient"),
        Task("\\Microsoft\\Windows\\Feedback\\Siuf\\DmClientOnScenarioDownload"),
        Task("\\Microsoft\\Windows\\Windows Error Reporting\\QueueReporting"),
    ]),
    // ---------- Servizi: Estremo ----------
    it("svc_update", Servizi, Estremo, "Windows Update disattivato", "Dopo questo il sistema non si potrà più aggiornare.", Both, &[
        Svc { name: "wuauserv", start: 4 },
        Svc { name: "UsoSvc", start: 4 },
        Svc { name: "WaaSMedicSvc", start: 4 },
        Svc { name: "DoSvc", start: 4 },
        d(WU_AU, "NoAutoUpdate", "1"),
    ]),
    it("defender", Servizi, Estremo, "Microsoft Defender disattivato", "Resti senza antivirus integrato: serve un'alternativa. Con la «Protezione antimanomissione» attiva può non funzionare sul sistema installato.", Both, &[
        d(WD_POL, "DisableAntiSpyware", "1"),
        d("HKLM\\SOFTWARE\\Policies\\Microsoft\\Windows Defender\\Real-Time Protection", "DisableRealtimeMonitoring", "1"),
        Svc { name: "WinDefend", start: 4 },
        Svc { name: "WdNisSvc", start: 4 },
        Svc { name: "Sense", start: 4 },
        Svc { name: "WdFilter", start: 4 },
        Svc { name: "WdNisDrv", start: 4 },
        Svc { name: "WdBoot", start: 4 },
        Svc { name: "SecurityHealthService", start: 4 },
    ]),
    it("smartscreen", Servizi, Estremo, "SmartScreen spento", "Perdi l'avviso su file e siti pericolosi.", Both, &[d(POL_SYS, "EnableSmartScreen", "0")]),
    it("svc_store", Servizi, Estremo, "Servizi dello Store", "", Both, &[
        Svc { name: "InstallService", start: 4 },
        Svc { name: "WSService", start: 4 },
        Svc { name: "LicenseManager", start: 4 },
    ]),
    // ---------- Componenti: Standard ----------
    it("onedrive", Componenti, Standard, "OneDrive", "", Both, &[
        RegDel { key: "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run", name: "OneDriveSetup" },
        Delete("System32\\OneDriveSetup.exe"),
        Delete("SysWOW64\\OneDriveSetup.exe"),
    ]),
    it("ie", Componenti, Standard, "Internet Explorer", "", Both, &[Capability("Browser.InternetExplorer"), Feature("Internet-Explorer-Optional-amd64")]),
    it("wmp", Componenti, Standard, "Windows Media Player (classico)", "", Both, &[Capability("Media.WindowsMediaPlayer"), Feature("WindowsMediaPlayer")]),
    it("quickassist", Componenti, Standard, "Assistenza rapida", "", Both, &[Capability("App.Support.QuickAssist")]),
    it("steps", Componenti, Standard, "Registratore azioni, WordPad, PowerShell ISE", "", Both, &[
        Capability("App.StepsRecorder"),
        Capability("Microsoft.Windows.WordPad"),
        Capability("Microsoft.Windows.PowerShell.ISE"),
    ]),
    it("ps2", Componenti, Standard, "PowerShell 2.0 (vecchio e insicuro)", "", Both, &[Feature("MicrosoftWindowsPowerShellV2Root"), Feature("MicrosoftWindowsPowerShellV2")]),
    // ---------- Componenti: Estremo ----------
    it("edge", Componenti, Estremo, "Microsoft Edge", "Il browser non c'è più: installa prima un altro browser. Alcune pagine di Windows e della Guida usano Edge. WebView2 resta.", IsoOnly, &[
        Delete("..\\Program Files (x86)\\Microsoft\\Edge"),
        Delete("..\\Program Files (x86)\\Microsoft\\EdgeCore"),
        Delete("..\\Program Files (x86)\\Microsoft\\EdgeUpdate"),
    ]),
    it("hyperv", Componenti, Estremo, "Hyper-V, Sandbox e Piattaforma macchina virtuale", "Niente macchine virtuali Hyper-V, Sandbox né WSL2.", Both, &[
        Feature("Microsoft-Hyper-V-All"),
        Feature("Containers-DisposableClientVM"),
        Feature("VirtualMachinePlatform"),
    ]),
    it("wsl", Componenti, Estremo, "Sottosistema Windows per Linux", "", Both, &[Feature("Microsoft-Windows-Subsystem-Linux")]),
    it("legacy_net", Componenti, Estremo, "SMB1, Telnet, TFTP, Work Folders, XPS", "", Both, &[
        Feature("SMB1Protocol"),
        Feature("TelnetClient"),
        Feature("TFTP"),
        Feature("WorkFolders-Client"),
        Feature("Printing-XPSServices-Features"),
    ]),
    it("fax", Componenti, Estremo, "Fax e scanner di Windows", "", Both, &[Feature("FaxServicesClientPackage")]),
    it("winre", Componenti, Estremo, "Ambiente di ripristino (WinRE)", "Senza WinRE non funzionano «Ripristina il PC» e la riparazione all'avvio.", IsoOnly, &[Delete("System32\\Recovery\\Winre.wim")]),
    it("lang_extra", Componenti, Estremo, "Riconoscimento vocale, scrittura a mano, OCR e sintesi vocale", "", Both, &[
        Capability("Language.Handwriting"),
        Capability("Language.OCR"),
        Capability("Language.Speech"),
        Capability("Language.TextToSpeech"),
    ]),
    it("hello", Componenti, Estremo, "Windows Hello (viso e impronte)", "", Both, &[Capability("Hello.Face"), Capability("Hello.Face.Migration")]),
    // ---------- Pulizia ----------
    it("clean_files", Pulizia, Standard, "File inutili (temporanei, log, Panther, download di aggiornamento)", "", Both, &[
        Delete("Logs\\*"),
        Delete("Temp\\*"),
        Delete("Panther\\*"),
        Delete("SoftwareDistribution\\Download\\*"),
        Delete("System32\\LogFiles\\*"),
        Delete("Prefetch\\*"),
    ]),
    it("clean_backup", Pulizia, Standard, "Cartella di backup di WinSxS", "Non si potranno più riparare certi componenti.", Both, &[Delete("WinSxS\\Backup\\*")]),
    it("resetbase", Pulizia, Standard, "Componenti superati (DISM ResetBase)", "Gli aggiornamenti già integrati non si possono più disinstallare. Può durare diversi minuti.", IsoOnly, &[ResetBase]),
    it("net_light", Pulizia, Standard, "Cache di .NET (immagini native)", "", Both, &[Delete("assembly\\NativeImages_*")]),
    it("wu_complete", Pulizia, Estremo, "File di Windows Update (pulizia completa)", "Dopo questo il sistema non si potrà più aggiornare.", Both, &[
        Delete("SoftwareDistribution\\*"),
        Delete("servicing\\LCU"),
        Delete("Logs\\CBS\\*"),
    ]),
    it("winsxs_deep", Pulizia, Estremo, "WinSxS: cache e cartelle temporanee (oltre DISM)", "Dopo questo il sistema non si potrà più aggiornare né riparare con DISM.", Both, &[
        Delete("WinSxS\\ManifestCache\\*"),
        Delete("WinSxS\\Temp\\*"),
        Delete("WinSxS\\InstallTemp\\*"),
        Delete("WinSxS\\Backup\\*"),
    ]),
];

pub fn items_for(scope_iso: bool) -> impl Iterator<Item = &'static Item> {
    ITEMS.iter().filter(move |i| match i.scope {
        Scope::Both => true,
        Scope::IsoOnly => scope_iso,
        Scope::OnlineOnly => !scope_iso,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_unique_and_complete() {
        let mut seen = HashSet::new();
        for i in ITEMS {
            assert!(seen.insert(i.id), "id doppio {}", i.id);
            assert!(!i.acts.is_empty(), "{}", i.id);
            assert!(!i.label.is_empty());
        }
    }

    #[test]
    fn risky_items_have_a_warning() {
        for id in [
            "store",
            "winget",
            "svc_update",
            "defender",
            "wu_complete",
            "winsxs_deep",
            "edge",
            "winre",
            "resetbase",
        ] {
            let i = ITEMS.iter().find(|i| i.id == id).unwrap();
            assert!(!i.warn.is_empty(), "{id}");
        }
        for id in ["svc_update", "wu_complete", "winsxs_deep"] {
            let i = ITEMS.iter().find(|i| i.id == id).unwrap();
            assert!(i.warn.contains("aggiorn"), "{id}");
        }
    }

    #[test]
    fn every_level_and_group_present() {
        for l in [Sicuro, Standard, Estremo] {
            assert!(ITEMS.iter().any(|i| i.level == l));
        }
        for g in Group::ALL {
            assert!(ITEMS.iter().any(|i| i.group == g));
        }
    }

    #[test]
    fn net_framework_is_not_removable() {
        for i in ITEMS {
            for a in i.acts {
                let s = format!("{a:?}").to_lowercase();
                assert!(
                    !s.contains("netfx") && !s.contains("microsoft.net\\"),
                    "{}",
                    i.id
                );
            }
        }
    }

    #[test]
    fn delete_paths_are_safe() {
        for i in ITEMS {
            for a in i.acts {
                if let Act::Delete(p) = a {
                    assert!(!p.starts_with('\\') && !p.contains(':'), "{p}");
                    let ups = p.matches("..").count();
                    assert!(
                        ups == 0 || p.starts_with("..\\Program Files (x86)\\Microsoft\\"),
                        "{p}"
                    );
                    assert!(p.len() >= 6, "{p}");
                }
            }
        }
    }

    #[test]
    fn scope_filters() {
        assert!(items_for(true).any(|i| i.id == "edge"));
        assert!(!items_for(false).any(|i| i.id == "edge"));
        assert!(items_for(false).any(|i| i.id == "power"));
        assert!(!items_for(true).any(|i| i.id == "power"));
    }
}
