//! Backup dell'intero sistema in un file `install.wim` (come «Backup di sistema» di Dism++):
//! copia ombra (VSS) del disco + `dism /Capture-Image`, così anche i file in uso vengono copiati in modo coerente.

use crate::clean::sys::Sys;
use regex::Regex;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compression {
    Max,
    Fast,
    None,
}

impl Compression {
    pub fn arg(self) -> &'static str {
        match self {
            Compression::Max => "max",
            Compression::Fast => "fast",
            Compression::None => "none",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Compression::Max => "Massima (più piccolo, più lento)",
            Compression::Fast => "Veloce",
            Compression::None => "Nessuna (molto grande)",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Plan {
    /// Unità da salvare, per esempio `C:`.
    pub drive: String,
    pub image: PathBuf,
    pub name: String,
    pub compression: Compression,
}

/// Controlla i dati inseriti dall'utente prima di toccare il sistema.
pub fn validate(p: &Plan) -> Result<(), String> {
    let Ok(re) = Regex::new(r"^[A-Za-z]:$") else {
        return Err("errore interno".into());
    };
    if !re.is_match(&p.drive) {
        return Err("Unità non valida (esempio: C:)".into());
    }
    let ext_ok = p
        .image
        .extension()
        .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case("wim"));
    if !ext_ok {
        return Err("Il file deve finire con .wim".into());
    }
    let dir = p
        .image
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .ok_or("Scegli una cartella di destinazione")?;
    if !dir.is_dir() {
        return Err(format!("La cartella {} non esiste", dir.display()));
    }
    if p.image.exists() {
        return Err(format!(
            "{} esiste già: scegli un altro nome (non lo sovrascrivo)",
            p.image.display()
        ));
    }
    if p.name.trim().is_empty() || p.name.contains('"') {
        return Err("Nome dell'immagine non valido".into());
    }
    Ok(())
}

/// Percorso del file di destinazione relativo all'unità copiata (se sta sulla stessa unità).
fn relative_on_drive(image: &Path, drive: &str) -> Option<String> {
    let s = image.to_string_lossy().replace('/', "\\");
    let (d, rest) = s.split_at_checked(2)?;
    if d.eq_ignore_ascii_case(drive) {
        Some(rest.to_string())
    } else {
        None
    }
}

/// File di configurazione di DISM: cosa non salvare.
pub fn config_ini(p: &Plan) -> String {
    let mut s = String::from(
        "[ExclusionList]\r\n\\$ntfs.log\r\n\\hiberfil.sys\r\n\\pagefile.sys\r\n\\swapfile.sys\r\n\\System Volume Information\r\n\\$Recycle.Bin\r\n\\RECYCLER\r\n\\Windows\\CSC\r\n",
    );
    if let Some(rel) = relative_on_drive(&p.image, &p.drive) {
        s.push_str(&rel);
        s.push_str("\r\n");
    }
    s.push_str("\r\n[CompressionExclusionList]\r\n*.zip\r\n*.rar\r\n*.7z\r\n*.cab\r\n*.mp4\r\n*.mkv\r\n*.jpg\r\n*.jpeg\r\n*.png\r\n");
    s
}

/// (ID, percorso del dispositivo) dalla riga `ID|\\?\GLOBALROOT\Device\HarddiskVolumeShadowCopyN\`.
pub fn parse_shadow(out: &str) -> Option<(String, String)> {
    let re = Regex::new(
        r"(\{[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}\})\|(\\\\\?\\GLOBALROOT\\Device\\HarddiskVolumeShadowCopy\d+)\\?",
    )
    .ok()?;
    let c = re.captures(out)?;
    Some((
        c.get(1)?.as_str().to_string(),
        format!("{}\\", c.get(2)?.as_str()),
    ))
}

fn create_shadow(sys: &dyn Sys, drive: &str) -> Result<(String, String), String> {
    let script = format!(
        "$r = Invoke-CimMethod -ClassName Win32_ShadowCopy -MethodName Create -Arguments @{{Volume='{drive}\\';Context='ClientAccessible'}}; \
         if ($r.ReturnValue -ne 0) {{ throw ('VSS errore ' + $r.ReturnValue) }}; \
         $s = Get-CimInstance Win32_ShadowCopy | Where-Object {{ $_.ID -eq $r.ShadowID }}; \
         Write-Output ($s.ID + '|' + $s.DeviceObject)"
    );
    let out = sys.run("powershell", &["-NoProfile", "-Command", &script])?;
    parse_shadow(&out).ok_or_else(|| format!("Copia ombra non riconosciuta: {}", out.trim()))
}

fn delete_shadow(sys: &dyn Sys, id: &str) -> Result<(), String> {
    // L'ID è già stato validato come GUID da parse_shadow.
    let script = format!(
        "Get-CimInstance Win32_ShadowCopy | Where-Object {{ $_.ID -eq '{id}' }} | Remove-CimInstance"
    );
    sys.run("powershell", &["-NoProfile", "-Command", &script])
        .map(|_| ())
}

/// Esegue il backup. `log` riceve i passaggi. Restituisce la dimensione del file creato (se leggibile).
pub fn run(
    sys: &dyn Sys,
    plan: &Plan,
    work_dir: &Path,
    log: &mut dyn FnMut(&str),
) -> Result<u64, String> {
    validate(plan)?;
    if !sys.is_admin() {
        return Err(
            "Servono i permessi di amministratore (riavvia TweakDesk come amministratore)".into(),
        );
    }
    std::fs::create_dir_all(work_dir).map_err(|e| format!("{}: {e}", work_dir.display()))?;
    let ini = work_dir.join("backup-esclusioni.ini");
    std::fs::write(&ini, config_ini(plan)).map_err(|e| format!("{}: {e}", ini.display()))?;

    log("Creo la copia ombra del disco (permette di copiare anche i file in uso)…");
    let (id, device) = create_shadow(sys, &plan.drive)?;
    let link = work_dir.join(format!("snapshot-{}", std::process::id()));
    let link_str = link.to_string_lossy().to_string();

    let result = (|| {
        sys.run("cmd", &["/c", "mklink", "/d", &link_str, &device])?;
        log("Copio il sistema nel file .wim (può richiedere molto tempo)…");
        let image = format!("/ImageFile:{}", plan.image.display());
        let dir = format!("/CaptureDir:{link_str}");
        let name = format!("/Name:{}", plan.name.trim());
        let cfg = format!("/ConfigFile:{}", ini.display());
        let comp = format!("/Compress:{}", plan.compression.arg());
        sys.run(
            "dism",
            &[
                "/Capture-Image",
                &image,
                &dir,
                &name,
                "/Description:Backup creato con TweakDesk",
                &cfg,
                &comp,
                "/CheckIntegrity",
            ],
        )
    })();

    log("Pulizia: tolgo il collegamento e la copia ombra…");
    let _ = sys.run("cmd", &["/c", "rmdir", &link_str]);
    if let Err(e) = delete_shadow(sys, &id) {
        log(&format!("Attenzione: copia ombra non eliminata ({e})"));
    }
    let _ = std::fs::remove_file(&ini);

    result?;
    Ok(std::fs::metadata(&plan.image).map(|m| m.len()).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clean::sys::fake::FakeSys;

    const SHADOW: &str =
        "{3F2504E0-4F89-11D3-9A0C-0305E82C3301}|\\\\?\\GLOBALROOT\\Device\\HarddiskVolumeShadowCopy12\r\n";

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("tdbk_{}_{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn plan(d: &Path) -> Plan {
        Plan {
            drive: "C:".into(),
            image: d.join("install.wim"),
            name: "Windows 11".into(),
            compression: Compression::Max,
        }
    }

    #[test]
    fn validation_rules() {
        let d = tmp("val");
        assert!(validate(&plan(&d)).is_ok());
        let mut p = plan(&d);
        p.drive = "C:\\".into();
        assert!(validate(&p).is_err());
        let mut p = plan(&d);
        p.image = d.join("x.txt");
        assert!(validate(&p).is_err());
        let mut p = plan(&d);
        p.image = d.join("nuova/cartella/x.wim");
        assert!(validate(&p).is_err());
        let mut p = plan(&d);
        p.name = "a\"b".into();
        assert!(validate(&p).is_err());
        let p = plan(&d);
        std::fs::write(&p.image, b"x").unwrap();
        assert!(validate(&p).unwrap_err().contains("esiste già"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn shadow_output_is_parsed_strictly() {
        let (id, dev) = parse_shadow(SHADOW).unwrap();
        assert_eq!(id, "{3F2504E0-4F89-11D3-9A0C-0305E82C3301}");
        assert_eq!(
            dev,
            "\\\\?\\GLOBALROOT\\Device\\HarddiskVolumeShadowCopy12\\"
        );
        assert!(parse_shadow("errore").is_none());
        assert!(parse_shadow("{x}|\\\\?\\GLOBALROOT\\Device\\Foo").is_none());
    }

    #[test]
    fn config_excludes_swap_and_the_image_itself() {
        let p = Plan {
            drive: "D:".into(),
            image: PathBuf::from("D:\\Backup\\install.wim"),
            name: "x".into(),
            compression: Compression::Fast,
        };
        let c = config_ini(&p);
        assert!(c.contains("\\pagefile.sys"));
        assert!(c.contains("\\hiberfil.sys"));
        assert!(c.contains("\r\n\\Backup\\install.wim\r\n"));
        let other = Plan {
            drive: "C:".into(),
            ..p
        };
        assert!(!config_ini(&other).contains("install.wim"));
    }

    #[test]
    fn full_run_creates_and_cleans_up() {
        let d = tmp("run");
        let sys = FakeSys {
            admin: true,
            ..FakeSys::default()
        };
        sys.responses
            .borrow_mut()
            .push(("Invoke-CimMethod".into(), SHADOW.into()));
        let p = plan(&d);
        let mut lines = Vec::new();
        run(&sys, &p, &d.join("lavoro"), &mut |l| {
            lines.push(l.to_string())
        })
        .unwrap();
        let c = sys.commands.borrow().clone();
        assert!(c
            .iter()
            .any(|x| x.contains("mklink /d") && x.contains("HarddiskVolumeShadowCopy12\\")));
        let dism = c.iter().find(|x| x.starts_with("dism ")).unwrap();
        assert!(dism.contains("/Capture-Image"));
        assert!(dism.contains("/Compress:max"));
        assert!(dism.contains("/Name:Windows 11"));
        // Ordine: ombra, link, dism, rimozione link, rimozione ombra.
        let pos = |s: &str| c.iter().position(|x| x.contains(s)).unwrap();
        assert!(pos("Invoke-CimMethod") < pos("mklink") && pos("mklink") < pos("dism"));
        assert!(pos("dism") < pos("rmdir") && pos("rmdir") < pos("Remove-CimInstance"));
        assert!(!lines.is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn cleans_up_even_when_dism_fails() {
        let d = tmp("fail");
        let sys = FakeSys {
            admin: true,
            ..FakeSys::default()
        };
        sys.responses
            .borrow_mut()
            .push(("Invoke-CimMethod".into(), SHADOW.into()));
        sys.failing.borrow_mut().push("dism ".into());
        let r = run(&sys, &plan(&d), &d.join("w"), &mut |_| {});
        assert!(r.is_err());
        let c = sys.commands.borrow().clone();
        assert!(c.iter().any(|x| x.contains("Remove-CimInstance")));
        assert!(c.iter().any(|x| x.contains("rmdir")));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn needs_admin_and_does_nothing_otherwise() {
        let d = tmp("adm");
        let sys = FakeSys::default();
        let r = run(&sys, &plan(&d), &d.join("w"), &mut |_| {});
        assert!(r.unwrap_err().contains("amministratore"));
        assert!(sys.commands.borrow().is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }
}
