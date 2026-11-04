//! Catalogo dei programmi noti: parole chiave, identificativo winget, opzioni silenziose.

pub struct Entry {
    pub name: &'static str,
    /// Parole chiave (con spazi = sequenza di parole nel nome del file).
    pub keywords: &'static [&'static str],
    pub winget_id: &'static str,
    /// Opzioni silenziose da usare al posto di quelle del tipo di installer.
    pub silent_args: Option<&'static str>,
}

const fn e(
    name: &'static str,
    keywords: &'static [&'static str],
    winget_id: &'static str,
    silent_args: Option<&'static str>,
) -> Entry {
    Entry {
        name,
        keywords,
        winget_id,
        silent_args,
    }
}

pub static CATALOG: &[Entry] = &[
    e(
        "Google Chrome",
        &["chrome", "googlechrome"],
        "Google.Chrome",
        None,
    ),
    e(
        "Mozilla Firefox",
        &["firefox"],
        "Mozilla.Firefox",
        Some("/S"),
    ),
    e(
        "Mozilla Thunderbird",
        &["thunderbird"],
        "Mozilla.Thunderbird",
        Some("/S"),
    ),
    e("Brave", &["brave"], "Brave.Brave", None),
    e("VLC media player", &["vlc"], "VideoLAN.VLC", None),
    e("7-Zip", &["7z", "7zip", "7 zip"], "7zip.7zip", None),
    e("WinRAR", &["winrar"], "RARLab.WinRAR", Some("/S")),
    e("Git", &["git"], "Git.Git", None),
    e(
        "Python",
        &["python"],
        "Python.Python.3.13",
        Some("/quiet InstallAllUsers=1 PrependPath=1"),
    ),
    e("Node.js", &["node", "nodejs"], "OpenJS.NodeJS.LTS", None),
    e(
        "Notepad++",
        &["notepad", "npp"],
        "Notepad++.Notepad++",
        None,
    ),
    e(
        "Visual Studio Code",
        &["vscode", "visual studio code"],
        "Microsoft.VisualStudioCode",
        None,
    ),
    e("PowerToys", &["powertoys"], "Microsoft.PowerToys", None),
    e("Discord", &["discord"], "Discord.Discord", Some("-s")),
    e("Spotify", &["spotify"], "Spotify.Spotify", Some("/silent")),
    e(
        "Telegram",
        &["telegram", "tsetup"],
        "Telegram.TelegramDesktop",
        None,
    ),
    e("Slack", &["slack"], "SlackTechnologies.Slack", None),
    e(
        "Zoom",
        &["zoom", "zoominstaller"],
        "Zoom.Zoom",
        Some("/silent"),
    ),
    e("OBS Studio", &["obs"], "OBSProject.OBSStudio", None),
    e("Audacity", &["audacity"], "Audacity.Audacity", None),
    e("GIMP", &["gimp"], "GIMP.GIMP.3", None),
    e("Inkscape", &["inkscape"], "Inkscape.Inkscape", None),
    e("Krita", &["krita"], "KDE.Krita", None),
    e("Blender", &["blender"], "BlenderFoundation.Blender", None),
    e("HandBrake", &["handbrake"], "HandBrake.HandBrake", None),
    e("VirtualBox", &["virtualbox"], "Oracle.VirtualBox", None),
    e(
        "Docker Desktop",
        &["docker"],
        "Docker.DockerDesktop",
        Some("install --quiet --accept-license"),
    ),
    e(
        "FileZilla",
        &["filezilla"],
        "TimKosse.FileZilla.Client",
        None,
    ),
    e("WinSCP", &["winscp"], "WinSCP.WinSCP", None),
    e("PuTTY", &["putty"], "PuTTY.PuTTY", None),
    e(
        "Wireshark",
        &["wireshark"],
        "WiresharkFoundation.Wireshark",
        None,
    ),
    e("TeamViewer", &["teamviewer"], "TeamViewer.TeamViewer", None),
    e(
        "AnyDesk",
        &["anydesk"],
        "AnyDesk.AnyDesk",
        Some("--install \"%ProgramFiles(x86)%\\AnyDesk\" --silent"),
    ),
    e(
        "CCleaner",
        &["ccleaner", "ccsetup"],
        "Piriform.CCleaner",
        Some("/S"),
    ),
    e(
        "Malwarebytes",
        &["malwarebytes", "mbsetup"],
        "Malwarebytes.Malwarebytes",
        Some("/VERYSILENT"),
    ),
    e(
        "LibreOffice",
        &["libreoffice"],
        "TheDocumentFoundation.LibreOffice",
        None,
    ),
    e(
        "qBittorrent",
        &["qbittorrent"],
        "qBittorrent.qBittorrent",
        None,
    ),
    e("Steam", &["steam", "steamsetup"], "Valve.Steam", Some("/S")),
    e("Foxit Reader", &["foxit"], "Foxit.FoxitReader", None),
    e(
        "Adobe Acrobat Reader",
        &["acrobat", "acroread"],
        "Adobe.Acrobat.Reader.64-bit",
        Some("/sAll /rs /msi EULA_ACCEPT=YES"),
    ),
    e("Everything", &["everything"], "voidtools.Everything", None),
];

fn tokens(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

/// Le parole lunghe (5+ lettere) possono essere l'inizio di una parola del nome
/// (`chromesetup`); quelle corte devono coincidere per intero (`git` non scatta su `digital`).
fn token_matches(file_token: &str, kw: &str) -> bool {
    if kw.len() >= 5 {
        file_token.starts_with(kw)
    } else {
        // `7z2408`: la parola corta può essere seguita solo da cifre.
        file_token
            .strip_prefix(kw)
            .is_some_and(|rest| rest.chars().all(|c| c.is_ascii_digit()))
    }
}

pub fn find(file_name: &str) -> Option<&'static Entry> {
    let stem = file_name.rsplit_once('.').map_or(file_name, |(s, _)| s);
    let ft = tokens(stem);
    CATALOG.iter().find(|entry| {
        entry.keywords.iter().any(|kw| {
            let kt = tokens(kw);
            if kt.is_empty() || kt.len() > ft.len() {
                return false;
            }
            ft.windows(kt.len())
                .any(|w| w.iter().zip(&kt).all(|(f, k)| token_matches(f, k)))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(f: &str) -> Option<&'static str> {
        find(f).map(|e| e.name)
    }

    #[test]
    fn matches_whole_words_not_substrings() {
        assert_eq!(name("Git-2.47.1-64-bit.exe"), Some("Git"));
        assert_eq!(name("digital-editions.exe"), None);
        assert_eq!(name("jobs-setup.exe"), None);
        assert_eq!(name("amdocs.msi"), None);
        assert_eq!(name("OBS-Studio-31.0.0.exe"), Some("OBS Studio"));
    }

    #[test]
    fn long_keywords_match_word_prefixes() {
        assert_eq!(name("ChromeSetup.exe"), Some("Google Chrome"));
        assert_eq!(name("DiscordSetup.exe"), Some("Discord"));
        assert_eq!(name("FirefoxSetup133.exe"), Some("Mozilla Firefox"));
    }

    #[test]
    fn multi_word_keywords_need_the_sequence() {
        assert_eq!(
            name("Visual-Studio-Code-1.95.exe"),
            Some("Visual Studio Code")
        );
        assert_eq!(name("7z2408-x64.exe"), Some("7-Zip"));
        assert_eq!(name("7-zip-24.08.msi"), Some("7-Zip"));
        assert_eq!(name("studio-visual.exe"), None);
    }

    #[test]
    fn catalog_ids_are_unique_and_non_empty() {
        let mut ids: Vec<&str> = CATALOG.iter().map(|e| e.winget_id).collect();
        assert!(ids.iter().all(|i| i.contains('.')));
        ids.sort_unstable();
        let n = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), n);
    }
}
