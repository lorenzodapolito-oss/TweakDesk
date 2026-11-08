//! Percorsi di lavoro «sicuri»: senza spazi, accenti o caratteri strani e abbastanza corti.
//! DISM e gli script `.cmd` (come quello di UUP dump) non gestiscono bene gli altri.

use crate::clean::sys::Sys;
use std::path::{Path, PathBuf};

/// Lunghezza massima della cartella di lavoro (Windows arriva a 260 con i percorsi interni).
pub const MAX_WORK_LEN: usize = 90;
/// Nome della cartella corta usata come ripiego, nella radice del disco.
pub const FALLBACK_DIR: &str = "TDwork";

/// Solo lettere e cifre ASCII e `\ / : _ - .`, nessuno spazio.
pub fn is_plain(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '\\' | '/' | ':' | '_' | '-' | '.'))
}

/// Lettera dell'unità se il percorso comincia con `X:`.
pub fn drive_of(s: &str) -> Option<char> {
    let mut it = s.chars();
    let d = it.next()?;
    (d.is_ascii_alphabetic() && it.next() == Some(':')).then(|| d.to_ascii_uppercase())
}

/// Tiene la cartella scelta se va bene, altrimenti usa `X:\TDwork` sullo stesso disco.
pub fn safe_root(base: &Path) -> PathBuf {
    let s = base.to_string_lossy();
    if is_plain(&s) && s.len() <= MAX_WORK_LEN - 20 {
        return base.to_path_buf();
    }
    match drive_of(&s) {
        Some(d) => PathBuf::from(format!("{d}:\\{FALLBACK_DIR}")),
        None => base.to_path_buf(),
    }
}

/// Errore chiaro se il percorso non è adatto.
pub fn check_work(p: &Path) -> Result<(), String> {
    let s = p.to_string_lossy();
    if !is_plain(&s) {
        return Err(format!(
            "Il percorso di lavoro «{s}» ha spazi, accenti o caratteri speciali: scegli una cartella come C:\\TDwork"
        ));
    }
    if s.len() > MAX_WORK_LEN {
        return Err(format!(
            "Il percorso di lavoro è troppo lungo ({} caratteri, massimo {MAX_WORK_LEN}): scegli una cartella più corta",
            s.len()
        ));
    }
    Ok(())
}

/// Nome file adatto a DISM: quello originale se semplice, altrimenti `pkg_<n>.<estensione>`.
pub fn plain_file_name(path: &Path, n: usize) -> String {
    let full = path.to_string_lossy();
    let name = full.rsplit(['\\', '/']).next().unwrap_or("");
    if is_plain(name) && !name.contains(':') {
        return name.to_string();
    }
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .filter(|e| is_plain(e))
        .unwrap_or_else(|| "bin".into());
    format!("pkg_{n:02}.{ext}")
}

/// Spazio libero (byte) sul disco del percorso; `None` se non si riesce a leggerlo.
pub fn free_bytes(sys: &dyn Sys, path: &Path) -> Option<u64> {
    let d = drive_of(&path.to_string_lossy())?;
    let out = sys
        .run(
            "powershell",
            &[
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                &format!("(Get-PSDrive -Name {d} -PSProvider FileSystem).Free"),
            ],
        )
        .ok()?;
    out.trim().parse().ok()
}

/// Errore se il disco ha meno spazio di `need` (se non si sa, lascia passare).
pub fn check_space(sys: &dyn Sys, path: &Path, need: u64) -> Result<(), String> {
    match free_bytes(sys, path) {
        Some(free) if free < need => Err(format!(
            "Spazio insufficiente su {}: servono circa {} GB, ce ne sono {}",
            path.display(),
            need.div_ceil(1_000_000_000),
            free / 1_000_000_000
        )),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clean::sys::fake::FakeSys;

    #[test]
    fn plain_detection() {
        assert!(is_plain("C:\\TDwork\\ISO-1.x"));
        assert!(!is_plain("C:\\Users\\Lorenzo D'Apolito\\x"));
        assert!(!is_plain("C:\\Perché"));
        assert!(!is_plain("C:\\a b"));
        assert!(!is_plain("C:\\a'b"));
        assert!(!is_plain(""));
    }

    #[test]
    fn safe_root_falls_back_to_short_folder_on_same_drive() {
        assert_eq!(
            safe_root(Path::new("D:\\Documenti e Più\\ISO")),
            PathBuf::from("D:\\TDwork")
        );
        assert_eq!(safe_root(Path::new("d:\\x y")), PathBuf::from("D:\\TDwork"));
        assert_eq!(safe_root(Path::new("C:\\TD")), PathBuf::from("C:\\TD"));
        let long = format!("C:\\{}", "a".repeat(80));
        assert_eq!(safe_root(Path::new(&long)), PathBuf::from("C:\\TDwork"));
    }

    #[test]
    fn check_work_messages() {
        assert!(check_work(Path::new("C:\\TDwork\\x")).is_ok());
        assert!(check_work(Path::new("C:\\a b"))
            .unwrap_err()
            .contains("spazi"));
        let long = format!("C:\\{}", "a".repeat(100));
        assert!(check_work(Path::new(&long)).unwrap_err().contains("lungo"));
    }

    #[test]
    fn file_names_are_made_plain() {
        assert_eq!(
            plain_file_name(Path::new("C:\\x\\windows11.0-kb1-x64.msu"), 1),
            "windows11.0-kb1-x64.msu"
        );
        assert_eq!(
            plain_file_name(Path::new("C:\\x\\Aggiornamento è.MSU"), 3),
            "pkg_03.msu"
        );
        assert_eq!(plain_file_name(Path::new("a b.cab"), 2), "pkg_02.cab");
    }

    #[test]
    fn space_check() {
        let sys = FakeSys::default();
        sys.responses
            .borrow_mut()
            .push(("Get-PSDrive".into(), "5000000000\r\n".into()));
        assert!(check_space(&sys, Path::new("C:\\TD"), 4_000_000_000).is_ok());
        let e = check_space(&sys, Path::new("C:\\TD"), 9_000_000_000).unwrap_err();
        assert!(e.contains("insufficiente"));
        // Non leggibile: si lascia passare.
        let vuoto = FakeSys::default();
        assert!(check_space(&vuoto, Path::new("C:\\TD"), u64::MAX).is_ok());
        assert!(check_space(&sys, Path::new("/tmp/x"), u64::MAX).is_ok());
    }
}
