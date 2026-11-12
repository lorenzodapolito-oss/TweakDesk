//! Icone proprie dei programmi (quelle dentro il file .exe) al posto della lettera.
//!
//! Su Windows le icone si estraggono con un piccolo script PowerShell nascosto e si salvano in
//! una cache come pixel grezzi (8 byte di intestazione: larghezza e altezza, poi RGBA). Il
//! programma le legge da lì: la prossima volta non serve più PowerShell. Fuori da Windows (o se
//! l'estrazione non riesce) resta la lettera.

use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};

/// Quante icone si chiedono a PowerShell in una volta sola.
const BATCH: usize = 40;
/// Lato massimo accettato per un'icona (i file della cache sono dati non fidati).
const MAX_SIDE: usize = 256;

/// Pixel RGBA di un'icona.
type Raw = (usize, usize, Vec<u8>);
type Done = Vec<(PathBuf, Option<Raw>)>;

/// Legge il formato della cache. Dati sbagliati o enormi danno `None`, mai un errore.
pub fn parse_raw(bytes: &[u8]) -> Option<Raw> {
    let w = usize::try_from(u32::from_le_bytes(bytes.get(0..4)?.try_into().ok()?)).ok()?;
    let h = usize::try_from(u32::from_le_bytes(bytes.get(4..8)?.try_into().ok()?)).ok()?;
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
        return None;
    }
    let len = w.checked_mul(h)?.checked_mul(4)?;
    let px = bytes.get(8..)?;
    if px.len() != len {
        return None;
    }
    Some((w, h, px.to_vec()))
}

/// Nome del file di cache: dipende dal percorso, dalla dimensione e dalla data del programma,
/// così se il programma cambia l'icona si rifà.
pub fn cache_file(dir: &Path, exe: &Path) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    exe.to_string_lossy().to_lowercase().hash(&mut h);
    if let Ok(m) = std::fs::metadata(exe) {
        m.len().hash(&mut h);
        if let Ok(t) = m.modified() {
            if let Ok(d) = t.duration_since(std::time::UNIX_EPOCH) {
                d.as_secs().hash(&mut h);
            }
        }
    }
    dir.join(format!("{:016x}.ico.raw", h.finish()))
}

/// Elenco per lo script: una riga «programma|file di cache» per icona.
pub fn list_text(items: &[(PathBuf, PathBuf)]) -> String {
    items
        .iter()
        // Il carattere | non può stare in un nome di file di Windows: è un separatore sicuro.
        .map(|(exe, out)| format!("{}|{}", exe.display(), out.display()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Script PowerShell: per ogni riga estrae l'icona e scrive i pixel nel formato della cache.
/// Se non riesce scrive un file vuoto, così non si riprova a ogni avvio.
pub const SCRIPT: &str = r#"Add-Type -AssemblyName System.Drawing
foreach ($line in Get-Content -LiteralPath $args[0] -Encoding UTF8) {
    $p = $line.Split('|')
    if ($p.Count -lt 2) { continue }
    try {
        $ic = [System.Drawing.Icon]::ExtractAssociatedIcon($p[0])
        $bmp = $ic.ToBitmap()
        $w = $bmp.Width; $h = $bmp.Height
        $buf = New-Object byte[] (8 + $w * $h * 4)
        [BitConverter]::GetBytes([int]$w).CopyTo($buf, 0)
        [BitConverter]::GetBytes([int]$h).CopyTo($buf, 4)
        $i = 8
        for ($y = 0; $y -lt $h; $y++) {
            for ($x = 0; $x -lt $w; $x++) {
                $c = $bmp.GetPixel($x, $y)
                $buf[$i] = $c.R; $buf[$i + 1] = $c.G; $buf[$i + 2] = $c.B; $buf[$i + 3] = $c.A
                $i += 4
            }
        }
        [IO.File]::WriteAllBytes($p[1], $buf)
    } catch {
        [IO.File]::WriteAllBytes($p[1], [byte[]]@())
    }
}
"#;

/// Cartella della cache: `%LOCALAPPDATA%\TweakDesk\icone`, altrimenti la cartella temporanea.
fn cache_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("TweakDesk").join("icone")
}

pub struct Icons {
    dir: PathBuf,
    tex: HashMap<PathBuf, Option<TextureHandle>>,
    wanted: Vec<(PathBuf, PathBuf)>,
    inflight: HashSet<PathBuf>,
    rx: Option<Receiver<Done>>,
}

impl Default for Icons {
    fn default() -> Self {
        Self::new(cache_dir())
    }
}

impl Icons {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            tex: HashMap::new(),
            wanted: Vec::new(),
            inflight: HashSet::new(),
            rx: None,
        }
    }

    /// Da chiamare a ogni giro di disegno: raccoglie le icone arrivate e avvia la richiesta
    /// delle prossime.
    pub fn tick(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.rx {
            if let Ok(done) = rx.try_recv() {
                for (exe, raw) in done {
                    self.inflight.remove(&exe);
                    let t = raw.map(|r| texture(ctx, &exe, r));
                    self.tex.insert(exe, t);
                }
                self.rx = None;
                ctx.request_repaint();
            }
        }
        if self.rx.is_none() && !self.wanted.is_empty() {
            let n = self.wanted.len().min(BATCH);
            let batch: Vec<_> = self.wanted.drain(..n).collect();
            self.rx = Some(spawn_extract(&self.dir, batch, ctx.clone()));
        }
    }

    /// L'icona del programma, se già pronta. Altrimenti la mette in coda e dà `None`.
    pub fn get(&mut self, ctx: &egui::Context, exe: &Path) -> Option<&TextureHandle> {
        if !cfg!(windows) {
            return None;
        }
        if !self.tex.contains_key(exe) {
            let file = cache_file(&self.dir, exe);
            match std::fs::read(&file) {
                Ok(bytes) => {
                    let t = parse_raw(&bytes).map(|r| texture(ctx, exe, r));
                    self.tex.insert(exe.to_path_buf(), t);
                }
                Err(_) => {
                    if self.inflight.insert(exe.to_path_buf()) {
                        self.wanted.push((exe.to_path_buf(), file));
                    }
                }
            }
        }
        self.tex.get(exe).and_then(|t| t.as_ref())
    }
}

fn texture(ctx: &egui::Context, exe: &Path, (w, h, px): Raw) -> TextureHandle {
    let img = ColorImage::from_rgba_unmultiplied([w, h], &px);
    ctx.load_texture(
        format!("icona:{}", exe.display()),
        img,
        TextureOptions::LINEAR,
    )
}

/// Estrae le icone in un thread: PowerShell nascosto, poi lettura dei file scritti.
fn spawn_extract(dir: &Path, batch: Vec<(PathBuf, PathBuf)>, ctx: egui::Context) -> Receiver<Done> {
    let (tx, rx) = channel();
    let dir = dir.to_path_buf();
    std::thread::spawn(move || {
        let _ = std::fs::create_dir_all(&dir);
        let list = dir.join("elenco.txt");
        let script = dir.join("estrai.ps1");
        let ok = std::fs::write(&list, list_text(&batch)).is_ok()
            && std::fs::write(&script, SCRIPT).is_ok();
        if ok {
            let mut cmd = std::process::Command::new("powershell");
            cmd.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
                .arg(&script)
                .arg(&list);
            crate::clean::sys::quiet(&mut cmd);
            let _ = cmd.output();
        }
        let done: Done = batch
            .into_iter()
            .map(|(exe, file)| {
                let raw = std::fs::read(&file).ok().and_then(|b| parse_raw(&b));
                (exe, raw)
            })
            .collect();
        let _ = tx.send(done);
        ctx.request_repaint();
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(w: u32, h: u32, n: usize) -> Vec<u8> {
        let mut v = w.to_le_bytes().to_vec();
        v.extend(h.to_le_bytes());
        v.extend(vec![7u8; n]);
        v
    }

    #[test]
    fn parses_a_valid_icon() {
        let (w, h, px) = parse_raw(&raw(2, 3, 24)).unwrap();
        assert_eq!((w, h, px.len()), (2, 3, 24));
    }

    #[test]
    fn hostile_or_broken_files_give_none() {
        assert!(parse_raw(&[]).is_none());
        assert!(parse_raw(&[0; 7]).is_none());
        assert!(parse_raw(&raw(0, 0, 0)).is_none());
        assert!(parse_raw(&raw(2, 3, 23)).is_none(), "dati troppo corti");
        assert!(parse_raw(&raw(2, 3, 25)).is_none(), "dati in più");
        // Dimensioni gigantesche: non si alloca nulla.
        assert!(parse_raw(&raw(u32::MAX, u32::MAX, 4)).is_none());
        assert!(parse_raw(&raw(257, 1, 257 * 4)).is_none());
        // File vuoto scritto quando l'estrazione fallisce.
        assert!(parse_raw(&[]).is_none());
    }

    #[test]
    fn cache_name_follows_the_program() {
        let dir = std::env::temp_dir().join(format!("td_icone_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let a = dir.join("a.exe");
        let b = dir.join("b.exe");
        std::fs::write(&a, b"1").unwrap();
        std::fs::write(&b, b"1").unwrap();
        let fa = cache_file(&dir, &a);
        assert_eq!(fa, cache_file(&dir, &a), "stesso programma, stesso file");
        assert_ne!(fa, cache_file(&dir, &b));
        std::fs::write(&a, b"123456").unwrap();
        assert_ne!(
            fa,
            cache_file(&dir, &a),
            "il programma è cambiato: icona da rifare"
        );
        assert!(fa.to_string_lossy().ends_with(".ico.raw"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_has_one_line_per_icon() {
        let items = vec![
            (PathBuf::from("C:/A/a.exe"), PathBuf::from("C:/c/1.raw")),
            (PathBuf::from("C:/B/b.exe"), PathBuf::from("C:/c/2.raw")),
        ];
        let t = list_text(&items);
        assert_eq!(t.lines().count(), 2);
        assert!(t.lines().all(|l| l.split('|').count() == 2));
        assert!(list_text(&[]).is_empty());
    }

    #[test]
    fn script_writes_the_same_format_the_reader_expects() {
        assert!(SCRIPT.contains("ExtractAssociatedIcon"));
        // Intestazione: larghezza e altezza a 32 bit, poi R G B A.
        assert!(SCRIPT.contains("GetBytes([int]$w)") && SCRIPT.contains("GetBytes([int]$h)"));
        assert!(SCRIPT.contains("$buf[$i] = $c.R"));
        assert!(SCRIPT.contains("$buf[$i + 3] = $c.A"));
        // Non cancella nulla.
        assert!(!SCRIPT.contains("Remove-Item"));
    }

    #[test]
    fn nothing_is_requested_outside_windows_and_a_missing_cache_is_queued_on_windows() {
        let dir = std::env::temp_dir().join(format!("td_icone2_{}", std::process::id()));
        let mut icons = Icons::new(dir.clone());
        let ctx = egui::Context::default();
        let exe = dir.join("zzz-inesistente.exe");
        assert!(icons.get(&ctx, &exe).is_none());
        if cfg!(windows) {
            assert_eq!(icons.wanted.len(), 1);
            // Una seconda richiesta non rimette in coda lo stesso programma.
            assert!(icons.get(&ctx, &exe).is_none());
            assert_eq!(icons.wanted.len(), 1);
        } else {
            assert!(icons.wanted.is_empty());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
