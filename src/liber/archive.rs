//! Estrazione degli zip (con difese contro percorsi malevoli) e copia di cartelle.

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

const MAX_ENTRIES: usize = 200_000;
const MAX_TOTAL: u64 = 16 * 1024 * 1024 * 1024;
const MAX_DEPTH: usize = 40;

/// Estrae lo zip in `dest`. I percorsi che escono dalla cartella vengono rifiutati.
pub fn extract_zip(zip_path: &Path, dest: &Path) -> Result<usize, String> {
    let f = File::open(zip_path).map_err(|e| format!("{}: {e}", zip_path.display()))?;
    let mut zip = zip::ZipArchive::new(f).map_err(|e| format!("Zip non valido: {e}"))?;
    if zip.len() > MAX_ENTRIES {
        return Err("Lo zip ha troppi file".into());
    }
    fs::create_dir_all(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    let mut total = 0u64;
    let mut n = 0usize;
    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| format!("Zip non valido: {e}"))?;
        let Some(rel) = entry.enclosed_name() else {
            return Err(format!("Percorso non sicuro nello zip: {}", entry.name()));
        };
        let out = dest.join(rel);
        if entry.is_dir() {
            fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
            continue;
        }
        total = total.saturating_add(entry.size());
        if total > MAX_TOTAL {
            return Err("Lo zip è troppo grande una volta estratto".into());
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        let mut w = File::create(&out).map_err(|e| format!("{}: {e}", out.display()))?;
        io::copy(&mut entry, &mut w).map_err(|e| format!("{}: {e}", out.display()))?;
        n += 1;
    }
    Ok(n)
}

/// Estrae un archivio `.7z` in `dest`, rifiutando i percorsi che escono dalla cartella.
pub fn extract_7z(path: &Path, dest: &Path) -> Result<usize, String> {
    fs::create_dir_all(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    let mut n = 0usize;
    let mut total = 0u64;
    let root = dest.to_path_buf();
    sevenz_rust::decompress_file_with_extract_fn(path, dest, |entry, reader, out| {
        let name = entry.name();
        if name.is_empty() {
            return Ok(true);
        }
        let bad = name.starts_with(['/', '\\'])
            || name.contains(':')
            || name.split(['/', '\\']).any(|p| p == "..")
            || !out.starts_with(&root);
        if bad {
            return Err(sevenz_rust::Error::other(format!(
                "Percorso non sicuro nell'archivio: {name}"
            )));
        }
        if n >= MAX_ENTRIES {
            return Err(sevenz_rust::Error::other("L'archivio ha troppi file"));
        }
        total = total.saturating_add(entry.size());
        if total > MAX_TOTAL {
            return Err(sevenz_rust::Error::other(
                "L'archivio è troppo grande una volta estratto",
            ));
        }
        if !entry.is_directory() {
            n += 1;
        }
        sevenz_rust::default_entry_extract_fn(entry, reader, out)
    })
    .map_err(|e| format!("Archivio .7z non valido: {e}"))?;
    Ok(n)
}

/// Se lo zip contiene una sola cartella e nient'altro, la radice è quella cartella.
pub fn content_root(dir: &Path) -> PathBuf {
    let mut cur = dir.to_path_buf();
    for _ in 0..3 {
        let Ok(rd) = fs::read_dir(&cur) else {
            break;
        };
        let items: Vec<_> = rd.flatten().collect();
        if items.len() == 1 && items[0].file_type().is_ok_and(|t| t.is_dir()) {
            cur = items[0].path();
        } else {
            break;
        }
    }
    cur
}

/// Copia ricorsiva; i collegamenti simbolici si saltano. `skip` decide quali percorsi
/// relativi (dalla radice) non copiare.
pub fn copy_tree(from: &Path, to: &Path, skip: &dyn Fn(&Path) -> bool) -> Result<usize, String> {
    fn go(
        root: &Path,
        cur: &Path,
        to: &Path,
        skip: &dyn Fn(&Path) -> bool,
        depth: usize,
        n: &mut usize,
    ) -> Result<(), String> {
        if depth > MAX_DEPTH {
            return Err("Cartelle troppo annidate".into());
        }
        let rd = fs::read_dir(cur).map_err(|e| format!("{}: {e}", cur.display()))?;
        for e in rd.flatten() {
            let p = e.path();
            let rel = p.strip_prefix(root).map_err(|e| e.to_string())?;
            if skip(rel) {
                continue;
            }
            let ft = e.file_type().map_err(|e| e.to_string())?;
            let dest = to.join(rel);
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                fs::create_dir_all(&dest).map_err(|e| format!("{}: {e}", dest.display()))?;
                go(root, &p, to, skip, depth + 1, n)?;
            } else if ft.is_file() {
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
                }
                fs::copy(&p, &dest).map_err(|e| format!("{}: {e}", p.display()))?;
                *n += 1;
            }
        }
        Ok(())
    }
    fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    let mut n = 0;
    go(from, from, to, skip, 0, &mut n)?;
    Ok(n)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ld_arc_{}_{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    pub fn make_zip(path: &Path, files: &[(&str, &[u8])]) {
        let f = File::create(path).unwrap();
        let mut z = zip::ZipWriter::new(f);
        let o = zip::write::SimpleFileOptions::default();
        for (name, data) in files {
            z.start_file(*name, o).unwrap();
            z.write_all(data).unwrap();
        }
        z.finish().unwrap();
    }

    #[test]
    fn extracts_and_finds_root() {
        let d = tmp("x");
        make_zip(
            &d.join("a.zip"),
            &[("App/app.exe", b"1"), ("App/dati/x.txt", b"2")],
        );
        assert_eq!(extract_zip(&d.join("a.zip"), &d.join("out")).unwrap(), 2);
        let root = content_root(&d.join("out"));
        assert!(root.ends_with("App"));
        assert!(root.join("dati/x.txt").is_file());
        // Con due elementi in cima la radice resta la cartella stessa.
        make_zip(&d.join("b.zip"), &[("a.exe", b"1"), ("b.dll", b"2")]);
        extract_zip(&d.join("b.zip"), &d.join("out2")).unwrap();
        assert_eq!(content_root(&d.join("out2")), d.join("out2"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn extracts_7z_archives() {
        let d = tmp("sz");
        fs::create_dir_all(d.join("src/sub")).unwrap();
        fs::write(d.join("src/a.exe"), b"1").unwrap();
        fs::write(d.join("src/sub/b.txt"), b"2").unwrap();
        sevenz_rust::compress_to_path(d.join("src"), d.join("a.7z")).unwrap();
        assert_eq!(extract_7z(&d.join("a.7z"), &d.join("out")).unwrap(), 2);
        let root = content_root(&d.join("out"));
        assert!(root.join("a.exe").is_file() || root.join("src/a.exe").is_file());
        fs::write(d.join("rotto.7z"), b"non e 7z").unwrap();
        assert!(extract_7z(&d.join("rotto.7z"), &d.join("o2")).is_err());
        assert!(extract_7z(&d.join("manca.7z"), &d.join("o3")).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn path_traversal_is_rejected() {
        let d = tmp("zs");
        make_zip(&d.join("evil.zip"), &[("../fuori.txt", b"x")]);
        let e = extract_zip(&d.join("evil.zip"), &d.join("out")).unwrap_err();
        assert!(e.contains("non sicuro"));
        assert!(!d.join("fuori.txt").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn broken_zip_is_an_error() {
        let d = tmp("bad");
        fs::write(d.join("a.zip"), b"non e uno zip").unwrap();
        assert!(extract_zip(&d.join("a.zip"), &d.join("o")).is_err());
        assert!(extract_zip(&d.join("manca.zip"), &d.join("o")).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn copy_tree_respects_skip() {
        let d = tmp("cp");
        fs::create_dir_all(d.join("a/sub")).unwrap();
        fs::write(d.join("a/uno.txt"), b"1").unwrap();
        fs::write(d.join("a/sub/due.txt"), b"2").unwrap();
        fs::write(d.join("a/salta.txt"), b"3").unwrap();
        let n = copy_tree(&d.join("a"), &d.join("b"), &|r| r == Path::new("salta.txt")).unwrap();
        assert_eq!(n, 2);
        assert!(d.join("b/sub/due.txt").is_file());
        assert!(!d.join("b/salta.txt").exists());
        let _ = fs::remove_dir_all(&d);
    }
}
