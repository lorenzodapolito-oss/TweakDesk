//! File di risposte `autounattend.xml` per la ISO: account locale, lingua, salto dei controlli, schermate iniziali.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// Salta account Microsoft e connessione obbligatoria.
    pub local_account: bool,
    pub account_name: String,
    pub account_password: String,
    /// Salta i controlli di TPM, Secure Boot, RAM e CPU durante l'installazione.
    pub bypass_checks: bool,
    pub language: String,
    pub keyboard: String,
    pub timezone: String,
    pub computer_name: String,
    pub skip_screens: bool,
    pub compact_os: bool,
    pub prevent_encryption: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            local_account: true,
            account_name: "Utente".into(),
            account_password: String::new(),
            bypass_checks: true,
            language: "it-IT".into(),
            keyboard: "0410:00000410".into(),
            timezone: "W. Europe Standard Time".into(),
            computer_name: String::new(),
            skip_screens: true,
            compact_os: false,
            prevent_encryption: true,
        }
    }
}

pub fn xml_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&apos;"),
            c if (c as u32) < 0x20 => {}
            c => o.push(c),
        }
    }
    o
}

fn ok_chars(s: &str, extra: &str) -> bool {
    s.chars().all(|c| c.is_alphanumeric() || extra.contains(c))
}

pub fn validate(o: &Options) -> Result<(), String> {
    if o.local_account {
        let n = o.account_name.trim();
        if n.is_empty() || n.chars().count() > 20 || !ok_chars(n, " ._-") {
            return Err("Nome dell'account non valido (massimo 20 caratteri: lettere, numeri, spazio . _ -)".into());
        }
        if o.account_password.chars().count() > 127 {
            return Err("Password troppo lunga".into());
        }
    }
    let c = o.computer_name.trim();
    if !c.is_empty() && (c.chars().count() > 15 || !ok_chars(c, "-")) {
        return Err(
            "Nome del PC non valido (massimo 15 caratteri: lettere, numeri, trattino)".into(),
        );
    }
    if o.language.is_empty() || !ok_chars(&o.language, "-") {
        return Err("Lingua non valida (esempio: it-IT)".into());
    }
    if !ok_chars(&o.keyboard, ":") {
        return Err("Tastiera non valida (esempio: 0410:00000410)".into());
    }
    if o.timezone.trim().is_empty() || !ok_chars(&o.timezone, " .-") {
        return Err("Fuso orario non valido".into());
    }
    Ok(())
}

fn run_sync(order: usize, desc: &str, cmd: &str) -> String {
    format!(
        "<RunSynchronousCommand wcm:action=\"add\"><Order>{order}</Order><Description>{}</Description><Path>{}</Path></RunSynchronousCommand>",
        xml_escape(desc),
        xml_escape(cmd)
    )
}

/// Comandi che disattivano i controlli di installazione.
pub fn lab_config_commands() -> [&'static str; 5] {
    [
        "reg add HKLM\\SYSTEM\\Setup\\LabConfig /v BypassTPMCheck /t REG_DWORD /d 1 /f",
        "reg add HKLM\\SYSTEM\\Setup\\LabConfig /v BypassSecureBootCheck /t REG_DWORD /d 1 /f",
        "reg add HKLM\\SYSTEM\\Setup\\LabConfig /v BypassRAMCheck /t REG_DWORD /d 1 /f",
        "reg add HKLM\\SYSTEM\\Setup\\LabConfig /v BypassCPUCheck /t REG_DWORD /d 1 /f",
        "reg add HKLM\\SYSTEM\\Setup\\LabConfig /v BypassStorageCheck /t REG_DWORD /d 1 /f",
    ]
}

pub fn generate(o: &Options) -> Result<String, String> {
    validate(o)?;
    let lang = xml_escape(&o.language);
    let kb = xml_escape(&o.keyboard);
    let tz = xml_escape(&o.timezone);
    let mut x = String::new();
    x.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    x.push_str("<unattend xmlns=\"urn:schemas-microsoft-com:unattend\" xmlns:wcm=\"http://schemas.microsoft.com/WMIConfig/2002/State\">\n");

    // --- windowsPE ---
    x.push_str("  <settings pass=\"windowsPE\">\n");
    x.push_str(&format!(
        "    <component name=\"Microsoft-Windows-International-Core-WinPE\" processorArchitecture=\"amd64\" publicKeyToken=\"31bf3856ad364e35\" language=\"neutral\" versionScope=\"nonSxS\">\n      <SetupUILanguage><UILanguage>{lang}</UILanguage></SetupUILanguage>\n      <InputLocale>{kb}</InputLocale><SystemLocale>{lang}</SystemLocale><UILanguage>{lang}</UILanguage><UserLocale>{lang}</UserLocale>\n    </component>\n"
    ));
    x.push_str("    <component name=\"Microsoft-Windows-Setup\" processorArchitecture=\"amd64\" publicKeyToken=\"31bf3856ad364e35\" language=\"neutral\" versionScope=\"nonSxS\">\n");
    if o.bypass_checks {
        x.push_str("      <RunSynchronous>");
        for (i, c) in lab_config_commands().iter().enumerate() {
            x.push_str(&run_sync(i + 1, "Salta i controlli di installazione", c));
        }
        x.push_str("</RunSynchronous>\n");
    }
    x.push_str("      <UserData><AcceptEula>true</AcceptEula></UserData>\n    </component>\n  </settings>\n");

    // --- specialize ---
    x.push_str("  <settings pass=\"specialize\">\n");
    x.push_str("    <component name=\"Microsoft-Windows-Shell-Setup\" processorArchitecture=\"amd64\" publicKeyToken=\"31bf3856ad364e35\" language=\"neutral\" versionScope=\"nonSxS\">\n");
    if !o.computer_name.trim().is_empty() {
        x.push_str(&format!(
            "      <ComputerName>{}</ComputerName>\n",
            xml_escape(o.computer_name.trim())
        ));
    }
    x.push_str(&format!(
        "      <TimeZone>{tz}</TimeZone>\n    </component>\n"
    ));
    if o.local_account || o.prevent_encryption {
        x.push_str("    <component name=\"Microsoft-Windows-Deployment\" processorArchitecture=\"amd64\" publicKeyToken=\"31bf3856ad364e35\" language=\"neutral\" versionScope=\"nonSxS\">\n      <RunSynchronous>");
        let mut n = 1;
        if o.local_account {
            x.push_str(&run_sync(n, "Account locale", "reg add HKLM\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\OOBE /v BypassNRO /t REG_DWORD /d 1 /f"));
            n += 1;
        }
        if o.prevent_encryption {
            x.push_str(&run_sync(n, "Niente cifratura automatica", "reg add HKLM\\SYSTEM\\CurrentControlSet\\Control\\BitLocker /v PreventDeviceEncryption /t REG_DWORD /d 1 /f"));
        }
        x.push_str("</RunSynchronous>\n    </component>\n");
    }
    x.push_str("  </settings>\n");

    // --- oobeSystem ---
    x.push_str("  <settings pass=\"oobeSystem\">\n");
    x.push_str(&format!(
        "    <component name=\"Microsoft-Windows-International-Core\" processorArchitecture=\"amd64\" publicKeyToken=\"31bf3856ad364e35\" language=\"neutral\" versionScope=\"nonSxS\">\n      <InputLocale>{kb}</InputLocale><SystemLocale>{lang}</SystemLocale><UILanguage>{lang}</UILanguage><UserLocale>{lang}</UserLocale>\n    </component>\n"
    ));
    x.push_str("    <component name=\"Microsoft-Windows-Shell-Setup\" processorArchitecture=\"amd64\" publicKeyToken=\"31bf3856ad364e35\" language=\"neutral\" versionScope=\"nonSxS\">\n");
    if o.local_account {
        let name = xml_escape(o.account_name.trim());
        x.push_str("      <UserAccounts><LocalAccounts><LocalAccount wcm:action=\"add\">\n");
        x.push_str(&format!("        <Name>{name}</Name><DisplayName>{name}</DisplayName><Group>Administrators</Group>\n"));
        if o.account_password.is_empty() {
            x.push_str("        <Password><Value></Value><PlainText>true</PlainText></Password>\n");
        } else {
            x.push_str(&format!(
                "        <Password><Value>{}</Value><PlainText>true</PlainText></Password>\n",
                xml_escape(&o.account_password)
            ));
        }
        x.push_str("      </LocalAccount></LocalAccounts></UserAccounts>\n");
    }
    x.push_str("      <OOBE>\n");
    if o.skip_screens {
        x.push_str("        <HideEULAPage>true</HideEULAPage><HideWirelessSetupInOOBE>true</HideWirelessSetupInOOBE><ProtectYourPC>3</ProtectYourPC>\n");
    }
    if o.local_account {
        x.push_str("        <HideOnlineAccountScreens>true</HideOnlineAccountScreens>\n");
    }
    x.push_str("      </OOBE>\n");
    if o.compact_os {
        x.push_str("      <FirstLogonCommands><SynchronousCommand wcm:action=\"add\"><Order>1</Order><Description>Compact OS</Description><CommandLine>compact.exe /CompactOS:always</CommandLine></SynchronousCommand></FirstLogonCommands>\n");
    }
    x.push_str("    </component>\n  </settings>\n</unattend>\n");
    Ok(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_valid_and_contains_key_parts() {
        let x = generate(&Options::default()).unwrap();
        assert!(x.contains("<UILanguage>it-IT</UILanguage>"));
        assert!(x.contains("BypassTPMCheck"));
        assert!(x.contains("BypassNRO"));
        assert!(x.contains("<Name>Utente</Name>"));
        assert!(x.contains("HideOnlineAccountScreens"));
        assert!(x.contains("W. Europe Standard Time"));
        assert!(!x.contains("CompactOS"));
        assert_eq!(x.matches("<settings ").count(), 3);
        assert_eq!(x.matches("</settings>").count(), 3);
    }

    #[test]
    fn options_switch_parts_off() {
        let o = Options {
            local_account: false,
            bypass_checks: false,
            skip_screens: false,
            prevent_encryption: false,
            compact_os: true,
            ..Options::default()
        };
        let x = generate(&o).unwrap();
        assert!(!x.contains("BypassTPMCheck"));
        assert!(!x.contains("LocalAccount"));
        assert!(!x.contains("HideEULAPage"));
        assert!(!x.contains("PreventDeviceEncryption"));
        assert!(x.contains("compact.exe /CompactOS:always"));
    }

    #[test]
    fn password_is_escaped() {
        let o = Options {
            account_password: "a<b>&\"c'".into(),
            ..Options::default()
        };
        let x = generate(&o).unwrap();
        assert!(x.contains("<Value>a&lt;b&gt;&amp;&quot;c&apos;</Value>"));
        assert!(!x.contains("a<b>"));
    }

    #[test]
    fn invalid_values_are_rejected() {
        let bad = |f: fn(&mut Options)| {
            let mut o = Options::default();
            f(&mut o);
            assert!(generate(&o).is_err());
        };
        bad(|o| o.account_name = "".into());
        bad(|o| o.account_name = "a<b".into());
        bad(|o| o.account_name = "x".repeat(21));
        bad(|o| o.computer_name = "nome con spazi".into());
        bad(|o| o.computer_name = "x".repeat(16));
        bad(|o| o.language = "it IT; calc".into());
        bad(|o| o.keyboard = "0410;x".into());
        bad(|o| o.timezone = "".into());
    }

    #[test]
    fn computer_name_included_when_set() {
        let o = Options {
            computer_name: "PC-LORENZO".into(),
            ..Options::default()
        };
        assert!(generate(&o)
            .unwrap()
            .contains("<ComputerName>PC-LORENZO</ComputerName>"));
    }

    #[test]
    fn escape_drops_control_characters() {
        assert_eq!(xml_escape("a\u{1}b"), "ab");
    }
}
