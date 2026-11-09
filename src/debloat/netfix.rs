//! Controlli di rete prima di scaricare Windows: file hosts, DNS e script di UUP dump.
//!
//! Gli errori «Failed to resolve the hostname» di aria2 nascono quasi sempre da righe nel file
//! hosts (blocchi dei programmi di privacy o IP messi a mano) o da un DNS del router che non
//! risponde. Qui li si individua e, dove è sicuro, li si corregge.

use crate::clean::sys::Sys;
use crate::debloat::apply::Kind;
use crate::debloat::target::ps_quote;
use regex::Regex;
use std::fs;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

/// Nome usato da UUP dump per scaricare i pacchetti.
pub const DOWNLOAD_HOST: &str = "tlu.dl.delivery.mp.microsoft.com";
/// Domini che nel file hosts rompono il download di Windows.
const BLOCKED: [&str; 6] = [
    "delivery.mp.microsoft.com",
    "delivery.microsoft.com",
    "windowsupdate.com",
    "download.microsoft.com",
    "update.microsoft.com",
    "uupdump.net",
];
/// Tentativi massimi che imponiamo allo script di UUP dump (di suo ripete all'infinito).
pub const MAX_TRIES: u32 = 6;

pub fn hosts_path() -> PathBuf {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
    PathBuf::from(root).join("System32/drivers/etc/hosts")
}

fn mentions_blocked(line: &str) -> bool {
    let l = line.trim();
    if l.is_empty() || l.starts_with('#') {
        return false;
    }
    let l = l.to_ascii_lowercase();
    BLOCKED.iter().any(|d| l.contains(d))
}

/// Righe attive del file hosts che toccano i domini di download.
pub fn problem_lines(text: &str) -> Vec<String> {
    text.lines()
        .filter(|l| mentions_blocked(l))
        .map(|l| l.trim().to_string())
        .collect()
}

/// Testo senza quelle righe (stesso tipo di a capo del file) e righe tolte.
pub fn clean_hosts(text: &str) -> (String, Vec<String>) {
    let crlf = text.contains("\r\n");
    let mut keep = Vec::new();
    let mut removed = Vec::new();
    for l in text.lines() {
        if mentions_blocked(l) {
            removed.push(l.trim().to_string());
        } else {
            keep.push(l);
        }
    }
    let nl = if crlf { "\r\n" } else { "\n" };
    let mut out = keep.join(nl);
    if !out.is_empty() {
        out.push_str(nl);
    }
    (out, removed)
}

/// Toglie dal file hosts le righe che bloccano il download, dopo una copia di sicurezza.
pub fn fix_hosts(path: &Path, log: &mut dyn FnMut(Kind, &str)) -> Result<usize, String> {
    let text = match fs::read(path) {
        Ok(b) => String::from_utf8_lossy(&b).to_string(),
        Err(_) => return Ok(0),
    };
    let (clean, removed) = clean_hosts(&text);
    if removed.is_empty() {
        return Ok(0);
    }
    let bak = path.with_file_name("hosts.tweakdesk.bak");
    if !bak.exists() {
        fs::write(&bak, &text).map_err(|e| format!("Copia di sicurezza non riuscita: {e}"))?;
    }
    fs::write(path, clean).map_err(|e| format!("{}: {e}", path.display()))?;
    for r in &removed {
        log(Kind::Warn, &format!("Tolta dal file hosts: {r}"));
    }
    log(
        Kind::Ok,
        &format!(
            "File hosts pulito ({} righe). Copia originale: {}",
            removed.len(),
            bak.display()
        ),
    );
    Ok(removed.len())
}

fn ps(sys: &dyn Sys, script: &str) -> Result<String, String> {
    sys.run(
        "powershell",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ],
    )
}

/// Indirizzi IP presenti nell'output (uno per riga).
pub fn parse_ips(out: &str) -> Vec<String> {
    out.lines()
        .map(str::trim)
        .filter(|l| l.parse::<IpAddr>().is_ok())
        .map(String::from)
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
pub enum Dns {
    /// Windows risolve il nome (come fa aria2 con `--async-dns=false`).
    Ok(Vec<String>),
    /// Windows no, ma un DNS pubblico sì: il guaio è il DNS del router o del provider.
    OnlyPublicWorks,
    /// Non risolve nessuno: rete, VPN o firewall.
    NothingWorks,
}

pub fn check_dns(sys: &dyn Sys, host: &str) -> Dns {
    let q = ps_quote(host);
    let system = ps(
        sys,
        &format!(
            "try {{ [System.Net.Dns]::GetHostAddresses({q}) | ForEach-Object {{ $_.IPAddressToString }} }} catch {{ exit 1 }}"
        ),
    )
    .map(|o| parse_ips(&o))
    .unwrap_or_default();
    if !system.is_empty() {
        return Dns::Ok(system);
    }
    let public = ps(
        sys,
        &format!(
            "Resolve-DnsName -Name {q} -Type A -DnsOnly -Server 1.1.1.1 -ErrorAction Stop | Where-Object {{ $_.IPAddress }} | ForEach-Object {{ $_.IPAddress }}"
        ),
    )
    .map(|o| parse_ips(&o))
    .unwrap_or_default();
    if public.is_empty() {
        Dns::NothingWorks
    } else {
        Dns::OnlyPublicWorks
    }
}

/// Imposta 1.1.1.1 e 8.8.8.8 sulle schede attive.
pub fn set_public_dns(sys: &dyn Sys) -> Result<(), String> {
    ps(
        sys,
        "Get-NetAdapter | Where-Object { $_.Status -eq 'Up' } | ForEach-Object { Set-DnsClientServerAddress -InterfaceIndex $_.ifIndex -ServerAddresses ('1.1.1.1','8.8.8.8') }; Clear-DnsClientCache",
    )
    .map(|_| ())
}

/// Rimette i DNS automatici (quelli del router) sulle schede attive.
pub fn reset_dns(sys: &dyn Sys) -> Result<(), String> {
    ps(
        sys,
        "Get-NetAdapter | Where-Object { $_.Status -eq 'Up' } | ForEach-Object { Set-DnsClientServerAddress -InterfaceIndex $_.ifIndex -ResetServerAddresses }; Clear-DnsClientCache",
    )
    .map(|_| ())
}

/// Tutti i controlli prima del download. `Err` = non conviene partire.
pub fn preflight(
    sys: &dyn Sys,
    hosts: &Path,
    log: &mut dyn FnMut(Kind, &str),
) -> Result<(), String> {
    fix_hosts(hosts, log)?;
    let _ = ps(sys, "Clear-DnsClientCache");
    match check_dns(sys, DOWNLOAD_HOST) {
        Dns::Ok(ips) => {
            log(
                Kind::Ok,
                &format!("Rete a posto: {DOWNLOAD_HOST} → {}", ips.join(", ")),
            );
            Ok(())
        }
        Dns::OnlyPublicWorks => Err(format!(
            "Il DNS del router o del provider non risolve {DOWNLOAD_HOST}, uno pubblico sì: usa «Imposta DNS pubblici» e riprova"
        )),
        Dns::NothingWorks => Err(format!(
            "{DOWNLOAD_HOST} non si risolve con nessun DNS: controlla la connessione, la VPN, il firewall o l'antivirus"
        )),
    }
}

/// Limita i tentativi dello script di UUP dump (di suo ripete senza fine se il download fallisce).
/// Restituisce il testo nuovo e quante righe ha cambiato.
pub fn patch_retries(script: &str) -> (String, usize) {
    let Ok(re) = Regex::new(
        r"(?i)^if %ERRORLEVEL% GTR 0 goto :(DOWNLOAD_APPS|DOWNLOAD_UUPS)( & exit /b 1)?\s*$",
    ) else {
        return (script.to_string(), 0);
    };
    let crlf = script.contains("\r\n");
    let mut out = Vec::new();
    let mut n = 0;
    for l in script.lines() {
        if let Some(c) = re.captures(l) {
            let label = c.get(1).map_or("", |m| m.as_str());
            out.push("set TDERR=%ERRORLEVEL%".to_string());
            out.push("if %TDERR% GTR 0 set /a TDN+=1".to_string());
            out.push(format!(
                "if %TDERR% GTR 0 if %TDN% LSS {MAX_TRIES} goto :{label}"
            ));
            out.push(
                "if %TDERR% GTR 0 echo Troppi tentativi: controlla DNS, file hosts, VPN e antivirus. & pause & exit /b 1"
                    .to_string(),
            );
            n += 1;
        } else {
            out.push(l.to_string());
        }
    }
    let nl = if crlf { "\r\n" } else { "\n" };
    let mut s = out.join(nl);
    s.push_str(nl);
    (s, n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clean::sys::fake::FakeSys;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("tdnet_{}_{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    const HOSTS: &str = "# Copyright\r\n127.0.0.1 localhost\r\n# 0.0.0.0 windowsupdate.com\r\n199.232.210.172 tlu.dl.delivery.mp.microsoft.com\r\n0.0.0.0 download.microsoft.com\r\n10.0.0.5 nas\r\n";

    #[test]
    fn finds_only_active_lines_for_download_domains() {
        let p = problem_lines(HOSTS);
        assert_eq!(p.len(), 2);
        assert!(p[0].contains("199.232.210.172"));
        assert!(problem_lines("127.0.0.1 localhost\n# x delivery.mp.microsoft.com\n").is_empty());
    }

    #[test]
    fn clean_keeps_everything_else_and_line_endings() {
        let (c, r) = clean_hosts(HOSTS);
        assert_eq!(r.len(), 2);
        assert!(c.contains("127.0.0.1 localhost\r\n"));
        assert!(c.contains("# 0.0.0.0 windowsupdate.com"));
        assert!(c.contains("10.0.0.5 nas\r\n"));
        assert!(!c.contains("199.232"));
        let (u, _) = clean_hosts("1.2.3.4 uupdump.net\nlocalhost\n");
        assert_eq!(u, "localhost\n");
    }

    #[test]
    fn fix_hosts_backs_up_once_and_is_idempotent() {
        let d = tmp("hosts");
        let h = d.join("hosts");
        fs::write(&h, HOSTS).unwrap();
        let mut log = Vec::new();
        assert_eq!(
            fix_hosts(&h, &mut |k, m| log.push((k, m.to_string()))).unwrap(),
            2
        );
        assert_eq!(
            fs::read_to_string(d.join("hosts.tweakdesk.bak")).unwrap(),
            HOSTS
        );
        assert!(!fs::read_to_string(&h).unwrap().contains("delivery"));
        assert!(log.iter().any(|(k, _)| *k == Kind::Warn));
        // Seconda volta: nulla da togliere, la copia resta quella originale.
        assert_eq!(fix_hosts(&h, &mut |_, _| {}).unwrap(), 0);
        assert_eq!(
            fs::read_to_string(d.join("hosts.tweakdesk.bak")).unwrap(),
            HOSTS
        );
        // File inesistente: nessun errore.
        assert_eq!(fix_hosts(&d.join("manca"), &mut |_, _| {}).unwrap(), 0);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn parses_ips_ignoring_noise() {
        let v = parse_ips("92.223.124.62\r\nerrore\r\n2a02:26f0:2d80:3::5f64:ab2c\r\n");
        assert_eq!(v, vec!["92.223.124.62", "2a02:26f0:2d80:3::5f64:ab2c"]);
    }

    #[test]
    fn dns_states() {
        let ok = FakeSys::default();
        ok.responses
            .borrow_mut()
            .push(("GetHostAddresses".into(), "92.223.124.62\r\n".into()));
        assert_eq!(
            check_dns(&ok, DOWNLOAD_HOST),
            Dns::Ok(vec!["92.223.124.62".into()])
        );

        let only = FakeSys::default();
        only.failing.borrow_mut().push("GetHostAddresses".into());
        only.responses
            .borrow_mut()
            .push(("Resolve-DnsName".into(), "92.223.124.62\r\n".into()));
        assert_eq!(check_dns(&only, DOWNLOAD_HOST), Dns::OnlyPublicWorks);

        let none = FakeSys::default();
        none.failing.borrow_mut().push("GetHostAddresses".into());
        none.failing.borrow_mut().push("Resolve-DnsName".into());
        assert_eq!(check_dns(&none, DOWNLOAD_HOST), Dns::NothingWorks);
    }

    #[test]
    fn preflight_fixes_hosts_then_reports_dns() {
        let d = tmp("pre");
        let h = d.join("hosts");
        fs::write(&h, HOSTS).unwrap();
        let sys = FakeSys::default();
        sys.responses
            .borrow_mut()
            .push(("GetHostAddresses".into(), "92.223.124.62\r\n".into()));
        preflight(&sys, &h, &mut |_, _| {}).unwrap();
        assert!(!fs::read_to_string(&h).unwrap().contains("199.232"));

        let bad = FakeSys::default();
        bad.failing.borrow_mut().push("GetHostAddresses".into());
        bad.responses
            .borrow_mut()
            .push(("Resolve-DnsName".into(), "1.2.3.4".into()));
        let e = preflight(&bad, &h, &mut |_, _| {}).unwrap_err();
        assert!(e.contains("Imposta DNS pubblici"));

        let none = FakeSys::default();
        none.failing.borrow_mut().push("GetHostAddresses".into());
        none.failing.borrow_mut().push("Resolve-DnsName".into());
        assert!(preflight(&none, &h, &mut |_, _| {})
            .unwrap_err()
            .contains("VPN"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn dns_buttons_run_the_right_commands() {
        let sys = FakeSys::default();
        set_public_dns(&sys).unwrap();
        reset_dns(&sys).unwrap();
        let c = sys.commands.borrow().clone();
        assert!(c
            .iter()
            .any(|x| x.contains("1.1.1.1") && x.contains("8.8.8.8")));
        assert!(c.iter().any(|x| x.contains("-ResetServerAddresses")));
    }

    const SCRIPT: &str = "echo a\r\nif %ERRORLEVEL% GTR 0 goto :DOWNLOAD_APPS\r\necho b\r\nif %ERRORLEVEL% GTR 0 goto :DOWNLOAD_UUPS & exit /b 1\r\nif %ERRORLEVEL% GTR 0 call :DOWNLOAD_ERROR & exit /b 1\r\n";

    #[test]
    fn retries_are_limited_in_the_uup_script() {
        let (s, n) = patch_retries(SCRIPT);
        assert_eq!(n, 2);
        assert!(s.contains("if %TDERR% GTR 0 if %TDN% LSS 6 goto :DOWNLOAD_APPS\r\n"));
        assert!(s.contains("if %TDERR% GTR 0 if %TDN% LSS 6 goto :DOWNLOAD_UUPS\r\n"));
        assert!(!s.contains("GTR 0 goto :DOWNLOAD"));
        // Le altre righe restano com'erano.
        assert!(s.contains("echo a\r\n") && s.contains("call :DOWNLOAD_ERROR & exit /b 1\r\n"));
        // Un secondo passaggio non cambia più nulla.
        assert_eq!(patch_retries(&s).1, 0);
        // Script sconosciuto: nessuna modifica.
        assert_eq!(patch_retries("echo ciao\n").1, 0);
    }
}
