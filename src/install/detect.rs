//! Riconoscimento del tipo di installer e relative opzioni silenziose.

use std::fs::File;
use std::io::Read;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Msi,
    Msix,
    Inno,
    Nsis,
    InstallShield,
    Burn,
    Unknown,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Msi => "MSI",
            Kind::Msix => "MSIX/APPX",
            Kind::Inno => "Inno Setup",
            Kind::Nsis => "NSIS",
            Kind::InstallShield => "InstallShield",
            Kind::Burn => "WiX Burn",
            Kind::Unknown => "Sconosciuto",
        }
    }

    /// Opzioni silenziose predefinite (per MSI/MSIX sono gestite a parte).
    pub fn default_args(self) -> &'static str {
        match self {
            Kind::Inno => "/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP-",
            Kind::Nsis => "/S",
            Kind::InstallShield => "/s /v\"/qn /norestart\"",
            Kind::Burn => "/quiet /norestart",
            Kind::Msi => "/qn /norestart",
            Kind::Msix | Kind::Unknown => "/S",
        }
    }

    /// Per i tipi sconosciuti le opzioni sono solo un tentativo.
    pub fn is_reliable(self) -> bool {
        self != Kind::Unknown
    }
}

const SIGNATURES: &[(&[u8], Kind)] = &[
    (b"Inno Setup", Kind::Inno),
    (b"Nullsoft", Kind::Nsis),
    (b"InstallShield", Kind::InstallShield),
    (b".wixburn", Kind::Burn),
    (b"WixBurn", Kind::Burn),
];

/// Cerca le firme leggendo il flusso a blocchi (senza caricare tutto il file),
/// tenendo in coda i byte finali per le firme a cavallo di due blocchi.
pub fn scan_signatures<R: Read>(mut r: R, limit: u64) -> Option<Kind> {
    let keep = SIGNATURES.iter().map(|(s, _)| s.len()).max().unwrap_or(0);
    let mut buf = vec![0u8; 256 * 1024];
    let mut carry: Vec<u8> = Vec::new();
    let mut read_total = 0u64;
    while read_total < limit {
        let n = r.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        read_total += n as u64;
        let mut window = std::mem::take(&mut carry);
        window.extend_from_slice(&buf[..n]);
        for (sig, kind) in SIGNATURES {
            if window.windows(sig.len()).any(|w| w == *sig) {
                return Some(*kind);
            }
        }
        let start = window.len().saturating_sub(keep);
        carry = window[start..].to_vec();
    }
    None
}

pub fn detect(path: &Path) -> Kind {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "msi" => return Kind::Msi,
        "msix" | "appx" | "msixbundle" | "appxbundle" => return Kind::Msix,
        _ => {}
    }
    match File::open(path) {
        Ok(f) => scan_signatures(f, 64 * 1024 * 1024).unwrap_or(Kind::Unknown),
        Err(_) => Kind::Unknown,
    }
}

/// Il file inizia come un eseguibile (MZ) o come un MSI (OLE2) o come uno zip (msix)?
pub fn looks_like_installer(path: &Path) -> bool {
    let mut head = [0u8; 8];
    let ok = File::open(path)
        .and_then(|mut f| f.read_exact(&mut head))
        .is_ok();
    ok && (head.starts_with(b"MZ")
        || head.starts_with(&[0xD0, 0xCF, 0x11, 0xE0])
        || head.starts_with(b"PK"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn finds_signatures_inside_the_stream() {
        let mut data = vec![0u8; 1000];
        data.extend_from_slice(b"...Nullsoft Install System...");
        assert_eq!(
            scan_signatures(Cursor::new(data), u64::MAX),
            Some(Kind::Nsis)
        );
        assert_eq!(
            scan_signatures(Cursor::new(vec![1u8; 5000]), u64::MAX),
            None
        );
    }

    #[test]
    fn finds_a_signature_split_across_two_blocks() {
        let block = 256 * 1024;
        let mut data = vec![0u8; block - 4];
        data.extend_from_slice(b"Inno Setup");
        data.extend_from_slice(&[0u8; 100]);
        assert_eq!(
            scan_signatures(Cursor::new(data), u64::MAX),
            Some(Kind::Inno)
        );
    }

    #[test]
    fn respects_the_read_limit() {
        let mut data = vec![0u8; 600 * 1024];
        data.extend_from_slice(b"Inno Setup");
        assert_eq!(scan_signatures(Cursor::new(data), 100 * 1024), None);
    }

    #[test]
    fn extension_decides_msi_and_msix() {
        assert_eq!(detect(Path::new("x/app.MSI")), Kind::Msi);
        assert_eq!(detect(Path::new("x/app.msixbundle")), Kind::Msix);
        assert_eq!(detect(Path::new("x/non_esiste.exe")), Kind::Unknown);
    }

    #[test]
    fn header_check() {
        let dir = std::env::temp_dir().join(format!("idesk_det_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let good = dir.join("a.exe");
        let bad = dir.join("b.exe");
        std::fs::write(&good, b"MZ\x90\x00rest-of-file").unwrap();
        std::fs::write(&bad, b"<html>404 not found</html>").unwrap();
        assert!(looks_like_installer(&good));
        assert!(!looks_like_installer(&bad));
        assert!(!looks_like_installer(&dir.join("missing.exe")));
    }

    #[test]
    fn real_file_detection() {
        let dir = std::env::temp_dir().join(format!("idesk_det2_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let f = dir.join("setup.exe");
        std::fs::write(&f, b"MZ....Inno Setup Setup Data....").unwrap();
        assert_eq!(detect(&f), Kind::Inno);
        assert!(Kind::Inno.is_reliable() && !Kind::Unknown.is_reliable());
    }
}
