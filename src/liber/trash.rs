//! Spostamento nel Cestino di Windows (recuperabile) di app e installer, con controlli di sicurezza.

use std::path::{Component, Path};

/// `target` sta davvero dentro `root` (non è `root` stesso, niente `..`)?
pub fn inside(root: &Path, target: &Path) -> bool {
    if target
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return false;
    }
    match target.strip_prefix(root) {
        Ok(rest) => rest.components().next().is_some(),
        Err(_) => false,
    }
}

fn quote(p: &Path) -> String {
    format!("'{}'", p.to_string_lossy().replace('\'', "''"))
}

/// Comando PowerShell che manda nel Cestino (con `SendToRecycleBin`, quindi recuperabile).
pub fn recycle_script(path: &Path, is_dir: bool) -> String {
    let (method, p) = if is_dir {
        ("DeleteDirectory", quote(path))
    } else {
        ("DeleteFile", quote(path))
    };
    format!(
        "Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.FileIO.FileSystem]::{method}({p},'OnlyErrorDialogs','SendToRecycleBin')"
    )
}

/// Manda `target` (cartella o file) nel Cestino, solo se sta dentro `root`.
pub fn to_recycle_bin(root: &Path, target: &Path) -> Result<(), String> {
    if !inside(root, target) {
        return Err(format!(
            "{} è fuori dalla cartella gestita: non lo tocco",
            target.display()
        ));
    }
    if !target.exists() {
        return Err(format!("{} non esiste più", target.display()));
    }
    run(&recycle_script(target, target.is_dir()))
}

#[cfg(windows)]
fn run(script: &str) -> Result<(), String> {
    let out = crate::clean::sys::quiet(std::process::Command::new("powershell").args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        script,
    ]))
    .output()
    .map_err(|e| format!("PowerShell non avviabile: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let t = String::from_utf8_lossy(&out.stderr);
        Err(t
            .lines()
            .next()
            .unwrap_or("operazione non riuscita")
            .trim()
            .to_string())
    }
}

#[cfg(not(windows))]
fn run(_script: &str) -> Result<(), String> {
    Err("Il Cestino è disponibile solo su Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn only_paths_strictly_inside_the_root_are_allowed() {
        let root = PathBuf::from("base").join("Apps");
        assert!(inside(&root, &root.join("Internet").join("Firefox")));
        assert!(inside(&root, &root.join("x")));
        assert!(!inside(&root, &root));
        assert!(!inside(
            &root,
            &PathBuf::from("base").join("Altro").join("x")
        ));
        assert!(!inside(&root, &root.join("..").join("Windows")));
        assert!(!inside(&root, Path::new("")));
    }

    #[test]
    fn script_uses_the_recycle_bin_and_escapes_quotes() {
        let s = recycle_script(Path::new("D:\\Apps\\L'app"), true);
        assert!(
            s.contains("DeleteDirectory('D:\\Apps\\L''app','OnlyErrorDialogs','SendToRecycleBin')")
        );
        let f = recycle_script(&PathBuf::from("D:\\i\\a.exe"), false);
        assert!(f.contains("DeleteFile('D:\\i\\a.exe'"));
        assert!(f.contains("SendToRecycleBin"));
    }

    #[test]
    fn refuses_outside_and_missing_targets() {
        let root = std::env::temp_dir().join(format!("ld_trash_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&root);
        assert!(to_recycle_bin(&root, &root).unwrap_err().contains("fuori"));
        assert!(to_recycle_bin(&root, &root.join("non_esiste"))
            .unwrap_err()
            .contains("non esiste"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
