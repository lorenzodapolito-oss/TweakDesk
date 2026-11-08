//! Crea una ISO di Windows 11 modificata: copia la ISO, applica le voci a ogni edizione scelta,
//! pulisce `boot.wim`, scrive il file di risposte e ricostruisce la ISO avviabile con `oscdimg`.
//! Non integra driver.

use crate::clean::engine::Env;
use crate::clean::sys::Sys;
use crate::debloat::apply::{self, Kind};
use crate::debloat::catalog::{items_for, Item};
use crate::debloat::paths;
use crate::debloat::target::{join_rel, ps_quote, Target};
use crate::debloat::unattend;
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compression {
    Max,
    Fast,
    None,
    /// Formato ESD (`install.esd`): il più piccolo, molto lento da creare.
    Esd,
}

impl Compression {
    pub const ALL: [Compression; 4] = [
        Compression::Max,
        Compression::Fast,
        Compression::None,
        Compression::Esd,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Compression::Max => "Massima (WIM)",
            Compression::Fast => "Veloce (WIM)",
            Compression::None => "Nessuna (WIM, molto grande)",
            Compression::Esd => "ESD (il più piccolo, molto lento)",
        }
    }
    fn arg(self) -> &'static str {
        match self {
            Compression::Max => "max",
            Compression::Fast => "fast",
            Compression::None => "none",
            Compression::Esd => "recovery",
        }
    }
}

/// Come si crea il file .iso finale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IsoMethod {
    /// Strumento già dentro Windows (IMAPI): niente da scaricare.
    Imapi,
    /// `oscdimg.exe` del Windows ADK.
    Oscdimg,
}

impl IsoMethod {
    pub const ALL: [IsoMethod; 2] = [IsoMethod::Imapi, IsoMethod::Oscdimg];
    pub fn label(self) -> &'static str {
        match self {
            IsoMethod::Imapi => "Strumento di Windows (IMAPI, senza ADK)",
            IsoMethod::Oscdimg => "oscdimg del Windows ADK",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edition {
    pub index: u32,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct Plan {
    pub iso_in: PathBuf,
    pub iso_out: PathBuf,
    /// Cartella di lavoro: ci vuole spazio per circa 3 volte la ISO.
    pub work: PathBuf,
    /// Indici delle edizioni da tenere.
    pub editions: Vec<u32>,
    /// Id delle voci del catalogo da applicare.
    pub items: Vec<String>,
    pub compression: Compression,
    /// Aggiornamenti (.msu / .cab) da integrare in ogni edizione, prima delle voci del catalogo.
    pub updates: Vec<PathBuf>,
    /// Pulisce `boot.wim` (componenti superati) e salta i controlli di TPM/RAM nell'installazione.
    pub boot_clean: bool,
    pub boot_bypass: bool,
    /// Tiene in `boot.wim` solo l'indice 2 (Setup).
    pub boot_setup_only: bool,
    /// Toglie dalla ISO file inutili (autorun, support, lingue non scelte).
    pub delete_files: bool,
    pub keep_langs: Vec<String>,
    pub compact_registry: bool,
    pub unattend: Option<unattend::Options>,
    pub method: IsoMethod,
    /// Serve solo con `IsoMethod::Oscdimg` e come ripiego se IMAPI non riesce.
    pub oscdimg: PathBuf,
    /// Cancella la cartella di lavoro alla fine.
    pub delete_work: bool,
}

const WORK_SUB: &str = "TweakDesk-ISO";

pub fn parse_editions(out: &str) -> Vec<Edition> {
    out.lines()
        .filter_map(|l| {
            let (i, n) = l.trim().split_once('|')?;
            Some(Edition {
                index: i.trim().parse().ok()?,
                name: n.trim().to_string(),
            })
        })
        .collect()
}

/// Lettera dell'unità dopo `Mount-DiskImage`.
pub fn parse_drive_letter(out: &str) -> Option<char> {
    let l = out.lines().map(str::trim).rfind(|l| !l.is_empty())?;
    let mut it = l.chars();
    let c = it.next()?;
    (it.next().is_none() && c.is_ascii_alphabetic()).then(|| c.to_ascii_uppercase())
}

fn ps(sys: &dyn Sys, script: &str) -> Result<String, String> {
    sys.run(
        "powershell",
        &[
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ],
    )
}

/// Cerca `oscdimg.exe` (fa parte del Windows ADK).
pub fn find_oscdimg(env: &dyn Env) -> Option<PathBuf> {
    let rel = "Windows Kits\\10\\Assessment and Deployment Kit\\Deployment Tools\\amd64\\Oscdimg\\oscdimg.exe";
    ["ProgramFiles(x86)", "ProgramFiles"]
        .iter()
        .filter_map(|v| env.var(v))
        .map(|base| join_rel(Path::new(&base), rel))
        .find(|p| p.is_file())
}

/// Programma di installazione del Windows ADK (scaricato direttamente da Microsoft).
pub const ADK_URL: &str =
    "https://download.microsoft.com/download/6/7/4/674ec7db-7c89-4f2b-8363-689055c2b430/adk/adksetup.exe";
/// Pagina ufficiale, se il download diretto non funziona.
pub const ADK_PAGE: &str = "https://learn.microsoft.com/windows-hardware/get-started/adk-install";

/// Scarica e installa solo i «Deployment Tools» del Windows ADK (contengono `oscdimg`).
pub fn adk_install(
    sys: &dyn Sys,
    env: &dyn Env,
    work: &Path,
    log: &mut dyn FnMut(Kind, &str),
) -> Result<PathBuf, String> {
    if let Some(p) = find_oscdimg(env) {
        log(Kind::Ok, "oscdimg.exe è già presente");
        return Ok(p);
    }
    if !sys.is_admin() {
        return Err(
            "Servono i permessi di amministratore (riavvia TweakDesk come amministratore)".into(),
        );
    }
    fs::create_dir_all(work).map_err(|e| format!("{}: {e}", work.display()))?;
    let setup = work.join("adksetup.exe");
    let _ = fs::remove_file(&setup);
    log(
        Kind::Info,
        "Scarico il programma di installazione del Windows ADK da Microsoft…",
    );
    let dl = format!(
        "$ProgressPreference = 'SilentlyContinue'; [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; \
         Invoke-WebRequest -UseBasicParsing -Uri {} -OutFile {}",
        ps_quote(ADK_URL),
        ps_quote(&setup.to_string_lossy())
    );
    ps(sys, &dl)
        .map_err(|e| format!("Download non riuscito: {e}. Puoi scaricarlo a mano da {ADK_PAGE}"))?;
    let size = fs::metadata(&setup).map(|m| m.len()).unwrap_or(0);
    if size < 1_000_000 {
        let _ = fs::remove_file(&setup);
        return Err(format!(
            "Il file scaricato non è valido. Scaricalo a mano da {ADK_PAGE}"
        ));
    }
    log(
        Kind::Info,
        "Installo i Deployment Tools (qualche minuto, serve internet)…",
    );
    let run = format!(
        "$p = Start-Process -FilePath {} -ArgumentList '/quiet','/norestart','/ceip','off','/features','OptionId.DeploymentTools' -Wait -PassThru; \
         if ($p.ExitCode -ne 0 -and $p.ExitCode -ne 3010) {{ exit $p.ExitCode }}",
        ps_quote(&setup.to_string_lossy())
    );
    let r = ps(sys, &run);
    let _ = fs::remove_file(&setup);
    r.map_err(|e| format!("Installazione non riuscita: {e}"))?;
    let found = find_oscdimg(env)
        .ok_or("Installato, ma oscdimg.exe non è stato trovato: indica il file a mano")?;
    log(
        Kind::Ok,
        &format!("oscdimg.exe pronto: {}", found.display()),
    );
    Ok(found)
}

pub fn selected_items(ids: &[String]) -> Result<Vec<&'static Item>, String> {
    let mut v = Vec::new();
    for id in ids {
        let i = items_for(true)
            .find(|i| i.id == id)
            .ok_or_else(|| format!("Voce sconosciuta o non valida per la ISO: {id}"))?;
        v.push(i);
    }
    Ok(v)
}

pub fn validate(p: &Plan) -> Result<(), String> {
    let is_ext = |path: &Path, e: &str| {
        path.extension()
            .is_some_and(|x| x.to_string_lossy().eq_ignore_ascii_case(e))
    };
    if !p.iso_in.is_file() || !is_ext(&p.iso_in, "iso") {
        return Err("Scegli il file .iso di Windows 11".into());
    }
    if !is_ext(&p.iso_out, "iso") {
        return Err("Il file di uscita deve finire con .iso".into());
    }
    let out_dir = p
        .iso_out
        .parent()
        .filter(|d| !d.as_os_str().is_empty() && d.is_dir())
        .ok_or("La cartella della ISO di uscita non esiste")?;
    let _ = out_dir;
    if p.iso_out.exists() {
        return Err(format!(
            "{} esiste già: scegli un altro nome (non lo sovrascrivo)",
            p.iso_out.display()
        ));
    }
    if p.iso_out == p.iso_in {
        return Err("La ISO di uscita non può essere quella di partenza".into());
    }
    if !p.work.is_dir() {
        return Err("La cartella di lavoro non esiste".into());
    }
    if p.editions.is_empty() {
        return Err("Scegli almeno un'edizione da tenere".into());
    }
    if !p.oscdimg.is_file() {
        return Err(
            "oscdimg.exe non trovato: installa il Windows ADK (Deployment Tools) o indica il file"
                .into(),
        );
    }
    for u in &p.updates {
        if !u.is_file() || !(is_ext(u, "msu") || is_ext(u, "cab")) {
            return Err(format!(
                "{}: non è un file .msu o .cab esistente",
                u.display()
            ));
        }
    }
    selected_items(&p.items)?;
    if let Some(u) = &p.unattend {
        unattend::validate(u)?;
    }
    Ok(())
}

fn drive_label(name: &str) -> String {
    let s: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .take(16)
        .collect();
    if s.is_empty() {
        "WIN11".into()
    } else {
        s
    }
}

/// Elenca le edizioni dentro una ISO (la monta e la smonta).
pub fn list_editions(sys: &dyn Sys, iso: &Path) -> Result<Vec<Edition>, String> {
    let q = ps_quote(&iso.to_string_lossy());
    let letter = parse_drive_letter(&ps(
        sys,
        &format!("$i = Mount-DiskImage -ImagePath {q} -PassThru; ($i | Get-Volume).DriveLetter"),
    )?)
    .ok_or("Non riesco a leggere la lettera dell'unità della ISO")?;
    let script = format!(
        "$w = '{letter}:\\sources\\install.wim'; if (-not (Test-Path -LiteralPath $w)) {{ $w = '{letter}:\\sources\\install.esd' }}; \
         Get-WindowsImage -ImagePath $w | ForEach-Object {{ \"$($_.ImageIndex)|$($_.ImageName)\" }}"
    );
    let r = ps(sys, &script);
    let _ = ps(
        sys,
        &format!("Dismount-DiskImage -ImagePath {q} | Out-Null"),
    );
    let list = parse_editions(&r?);
    if list.is_empty() {
        return Err("Nessuna edizione trovata nella ISO".into());
    }
    Ok(list)
}

fn dism(sys: &dyn Sys, args: &[&str]) -> Result<String, String> {
    sys.run("dism", args)
}

fn unmount(sys: &dyn Sys, dir: &Path, commit: bool) -> Result<(), String> {
    let d = format!("/MountDir:{}", dir.display());
    dism(
        sys,
        &[
            "/Unmount-Image",
            &d,
            if commit { "/Commit" } else { "/Discard" },
        ],
    )
    .map(|_| ())
}

fn mount(sys: &dyn Sys, wim: &Path, index: u32, dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    dism(
        sys,
        &[
            "/Mount-Image",
            &format!("/ImageFile:{}", wim.display()),
            &format!("/Index:{index}"),
            &format!("/MountDir:{}", dir.display()),
        ],
    )
    .map(|_| ())
}

fn export(
    sys: &dyn Sys,
    src: &Path,
    index: u32,
    dest: &Path,
    comp: Compression,
) -> Result<(), String> {
    dism(
        sys,
        &[
            "/Export-Image",
            &format!("/SourceImageFile:{}", src.display()),
            &format!("/SourceIndex:{index}"),
            &format!("/DestinationImageFile:{}", dest.display()),
            &format!("/Compress:{}", comp.arg()),
        ],
    )
    .map(|_| ())
}

fn replace_file(new: &Path, old: &Path) -> Result<(), String> {
    if !new.is_file() {
        return Err(format!("File non creato: {}", new.display()));
    }
    if old.exists() {
        fs::remove_file(old).map_err(|e| format!("{}: {e}", old.display()))?;
    }
    fs::rename(new, old)
        .or_else(|_| {
            fs::copy(new, old)
                .map(|_| ())
                .and_then(|()| fs::remove_file(new))
        })
        .map_err(|e| format!("{}: {e}", old.display()))
}

/// Modifica `boot.wim`: pulizia e salto dei controlli di installazione.
fn process_boot(
    sys: &dyn Sys,
    plan: &Plan,
    iso: &Path,
    work: &Path,
    log: &mut dyn FnMut(Kind, &str),
) -> Result<(), String> {
    let boot = iso.join("sources").join("boot.wim");
    if !boot.is_file() {
        log(Kind::Warn, "boot.wim non trovato: salto");
        return Ok(());
    }
    if !(plan.boot_clean || plan.boot_bypass || plan.boot_setup_only) {
        return Ok(());
    }
    if plan.boot_clean || plan.boot_bypass {
        let dir = work.join("mnt_boot");
        log(Kind::Info, "boot.wim: monto l'immagine Setup (indice 2)…");
        mount(sys, &boot, 2, &dir)?;
        let r: Result<(), String> = (|| {
            if plan.boot_bypass {
                let hive = join_rel(&dir, "Windows\\System32\\config\\SYSTEM");
                sys.run("reg", &["load", "HKLM\\zTD_BOOT", &hive.to_string_lossy()])?;
                for (name, key) in [
                    ("BypassTPMCheck", "Setup\\LabConfig"),
                    ("BypassSecureBootCheck", "Setup\\LabConfig"),
                    ("BypassRAMCheck", "Setup\\LabConfig"),
                    ("BypassCPUCheck", "Setup\\LabConfig"),
                    ("BypassStorageCheck", "Setup\\LabConfig"),
                    ("AllowUpgradesWithUnsupportedTPMOrCPU", "Setup\\MoSetup"),
                ] {
                    let full = format!("HKLM\\zTD_BOOT\\{key}");
                    let _ = sys.run(
                        "reg",
                        &["add", &full, "/v", name, "/t", "REG_DWORD", "/d", "1", "/f"],
                    );
                }
                for attempt in 0..5 {
                    if sys.run("reg", &["unload", "HKLM\\zTD_BOOT"]).is_ok() {
                        break;
                    }
                    if attempt < 4 {
                        std::thread::sleep(std::time::Duration::from_millis(400));
                    }
                }
                log(Kind::Ok, "boot.wim: controlli di installazione disattivati");
            }
            if plan.boot_clean {
                let img = format!("/Image:{}", dir.display());
                match dism(
                    sys,
                    &[
                        &img,
                        "/Cleanup-Image",
                        "/StartComponentCleanup",
                        "/ResetBase",
                    ],
                ) {
                    Ok(_) => log(Kind::Ok, "boot.wim: componenti superati puliti"),
                    Err(e) => log(Kind::Warn, &format!("boot.wim: pulizia non riuscita ({e})")),
                }
            }
            Ok(())
        })();
        if let Err(e) = r {
            let _ = unmount(sys, &dir, false);
            return Err(e);
        }
        unmount(sys, &dir, true)?;
    }
    let new = work.join("boot_new.wim");
    let _ = fs::remove_file(&new);
    let indexes: &[u32] = if plan.boot_setup_only { &[2] } else { &[1, 2] };
    for i in indexes {
        export(sys, &boot, *i, &new, Compression::Max)?;
    }
    replace_file(&new, &boot)?;
    log(Kind::Ok, "boot.wim pronto");
    Ok(())
}

/// Applica le voci a ogni edizione e riscrive `install.wim` (o `install.esd`).
/// Copia gli aggiornamenti in una cartella con nomi semplici (DISM non ama spazi e accenti).
/// I nomi originali restano quando sono semplici: i pacchetti «checkpoint» li usano.
fn stage_updates(plan: &Plan, work: &Path) -> Result<Option<PathBuf>, String> {
    if plan.updates.is_empty() {
        return Ok(None);
    }
    let dir = work.join("upd");
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for (n, u) in plan.updates.iter().enumerate() {
        let dest = dir.join(paths::plain_file_name(u, n + 1));
        fs::copy(u, &dest).map_err(|e| format!("{}: {e}", u.display()))?;
    }
    Ok(Some(dir))
}

fn add_updates(
    sys: &dyn Sys,
    mnt: &Path,
    upd: &Path,
    log: &mut dyn FnMut(Kind, &str),
) -> Result<(), String> {
    dism(
        sys,
        &[
            &format!("/Image:{}", mnt.display()),
            "/Add-Package",
            &format!("/PackagePath:{}", upd.display()),
        ],
    )?;
    log(Kind::Ok, "Aggiornamenti integrati");
    if let Err(e) = dism(
        sys,
        &[
            &format!("/Image:{}", mnt.display()),
            "/Cleanup-Image",
            "/StartComponentCleanup",
        ],
    ) {
        log(
            Kind::Warn,
            &format!("Pulizia dei componenti non riuscita: {e}"),
        );
    }
    Ok(())
}

fn process_install(
    sys: &dyn Sys,
    plan: &Plan,
    iso: &Path,
    work: &Path,
    items: &[&Item],
    log: &mut dyn FnMut(Kind, &str),
) -> Result<(), String> {
    let sources = iso.join("sources");
    let wim = sources.join("install.wim");
    let esd = sources.join("install.esd");
    // Immagine di partenza e indici validi al suo interno.
    let (src, indexes): (PathBuf, Vec<u32>) = if wim.is_file() {
        (wim.clone(), plan.editions.clone())
    } else if esd.is_file() {
        log(
            Kind::Info,
            "install.esd: converto le edizioni scelte in WIM…",
        );
        let tmp = work.join("install_da_esd.wim");
        let _ = fs::remove_file(&tmp);
        for i in &plan.editions {
            export(sys, &esd, *i, &tmp, Compression::Max)?;
        }
        let n = u32::try_from(plan.editions.len()).unwrap_or(0);
        (tmp, (1..=n).collect())
    } else {
        return Err("install.wim / install.esd non trovato nella ISO".into());
    };

    let upd = stage_updates(plan, work)?;
    for (n, idx) in indexes.iter().enumerate() {
        let dir = work.join(format!("mnt_{idx}"));
        log(
            Kind::Info,
            &format!("Edizione {} di {}: monto l'immagine…", n + 1, indexes.len()),
        );
        mount(sys, &src, *idx, &dir)?;
        if let Some(u) = &upd {
            log(
                Kind::Info,
                "Integro gli aggiornamenti (può richiedere molto tempo)…",
            );
            if let Err(e) = add_updates(sys, &dir, u, log) {
                let _ = unmount(sys, &dir, false);
                return Err(format!("Aggiornamenti non integrati: {e}"));
            }
        }
        let target = Target::offline(&dir);
        let (rep, _undo) = apply::apply(items, &target, sys, log);
        log(
            Kind::Info,
            &format!(
                "Voci: {} fatte, {} senza effetto, {} fallite",
                rep.done, rep.skipped, rep.failed
            ),
        );
        if plan.compact_registry {
            match apply::compact_hives(&target, sys, work) {
                Ok(c) => log(Kind::Ok, &format!("Registro compattato ({c} rami)")),
                Err(e) => log(Kind::Warn, &format!("Registro non compattato: {e}")),
            }
        }
        log(
            Kind::Info,
            "Salvo le modifiche (può richiedere alcuni minuti)…",
        );
        if let Err(e) = unmount(sys, &dir, true) {
            let _ = unmount(sys, &dir, false);
            return Err(format!("Salvataggio non riuscito: {e}"));
        }
    }

    let new_name = if plan.compression == Compression::Esd {
        "install_new.esd"
    } else {
        "install_new.wim"
    };
    let new = work.join(new_name);
    let _ = fs::remove_file(&new);
    log(
        Kind::Info,
        "Riscrivo l'immagine con la compressione scelta…",
    );
    for idx in &indexes {
        export(sys, &src, *idx, &new, plan.compression)?;
    }
    let dest = if plan.compression == Compression::Esd {
        esd.clone()
    } else {
        wim.clone()
    };
    for old in [&wim, &esd] {
        if old.exists() {
            fs::remove_file(old).map_err(|e| format!("{}: {e}", old.display()))?;
        }
    }
    if src != wim {
        let _ = fs::remove_file(&src);
    }
    replace_file(&new, &dest)?;
    log(
        Kind::Ok,
        &format!(
            "{} pronto",
            dest.file_name()
                .map_or(String::new(), |n| n.to_string_lossy().to_string())
        ),
    );
    Ok(())
}

/// Toglie file inutili dalla radice della ISO.
pub fn delete_iso_files(iso: &Path, keep_langs: &[String]) -> Vec<String> {
    let mut removed = Vec::new();
    let auto = iso.join("autorun.inf");
    if auto.is_file() && fs::remove_file(&auto).is_ok() {
        removed.push("autorun.inf".to_string());
    }
    let support = iso.join("support");
    if support.is_dir() && fs::remove_dir_all(&support).is_ok() {
        removed.push("support".to_string());
    }
    let Ok(re) = Regex::new(r"^[A-Za-z]{2,3}-[A-Za-z]{2,4}$") else {
        return removed;
    };
    if let Ok(rd) = fs::read_dir(iso.join("sources")) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if re.is_match(&name)
                && e.path().is_dir()
                && !keep_langs.iter().any(|k| k.eq_ignore_ascii_case(&name))
                && fs::remove_dir_all(e.path()).is_ok()
            {
                removed.push(format!("sources\\{name}"));
            }
        }
    }
    removed
}

/// Tutto tranne copia della ISO iniziale e creazione della ISO finale (provabile senza Windows).
pub fn process(
    sys: &dyn Sys,
    plan: &Plan,
    iso: &Path,
    work: &Path,
    log: &mut dyn FnMut(Kind, &str),
) -> Result<(), String> {
    let items = selected_items(&plan.items)?;
    process_install(sys, plan, iso, work, &items, log)?;
    process_boot(sys, plan, iso, work, log)?;
    if plan.delete_files {
        let removed = delete_iso_files(iso, &plan.keep_langs);
        log(
            Kind::Ok,
            &format!("File inutili tolti dalla ISO: {}", removed.len()),
        );
    }
    if let Some(u) = &plan.unattend {
        let xml = unattend::generate(u)?;
        let f = iso.join("autounattend.xml");
        fs::write(&f, xml).map_err(|e| format!("{}: {e}", f.display()))?;
        log(Kind::Ok, "File di risposte scritto (autounattend.xml)");
    }
    Ok(())
}

/// Codice C# che scrive il risultato di IMAPI su file (senza `unsafe`).
const IMAPI_CS: &str = "using System; using System.IO; using System.Runtime.InteropServices; using System.Runtime.InteropServices.ComTypes;\n\
public static class TdIso {\n\
  public static void Create(string path, object stream, int blockSize, int totalBlocks) {\n\
    IStream s = (IStream)stream; byte[] buf = new byte[blockSize]; IntPtr pcb = Marshal.AllocHGlobal(4);\n\
    try { using (FileStream o = File.Create(path)) { while (totalBlocks-- > 0) { s.Read(buf, blockSize, pcb); int n = Marshal.ReadInt32(pcb); o.Write(buf, 0, n); } } }\n\
    finally { Marshal.FreeHGlobal(pcb); }\n\
  }\n\
}";

/// Script PowerShell che crea la ISO (UDF 1.02, avvio BIOS ed EFI) con IMAPI.
pub fn imapi_script(iso: &Path, out: &Path, label: &str, bios: &Path, efi: &Path) -> String {
    format!(
        "$ErrorActionPreference = 'Stop'\n\
Add-Type -TypeDefinition @'\n{IMAPI_CS}\n'@\n\
function New-Boot($file, $platform) {{\n\
  $st = New-Object -ComObject ADODB.Stream; $st.Open(); $st.Type = 1; $st.LoadFromFile($file)\n\
  $b = New-Object -ComObject IMAPI2FS.BootOptions; $b.PlatformId = $platform; $b.Emulation = 0; $b.AssignBootImage($st); $b\n\
}}\n\
$fsi = New-Object -ComObject IMAPI2FS.MsftFileSystemImage\n\
$fsi.FileSystemsToCreate = 4\n\
$fsi.UDFRevision = 258\n\
$fsi.FreeMediaBlocks = 0\n\
$fsi.VolumeName = {label}\n\
$fsi.BootImageOptionsArray = @((New-Boot {bios} 0), (New-Boot {efi} 239))\n\
$fsi.Root.AddTree({iso}, $false)\n\
$r = $fsi.CreateResultImage()\n\
[TdIso]::Create({out}, $r.ImageStream, $r.BlockSize, $r.TotalBlocks)\n",
        label = ps_quote(label),
        bios = ps_quote(&bios.to_string_lossy()),
        efi = ps_quote(&efi.to_string_lossy()),
        iso = ps_quote(&iso.to_string_lossy()),
        out = ps_quote(&out.to_string_lossy()),
    )
}

fn iso_label(plan: &Plan) -> String {
    drive_label(
        &plan
            .iso_out
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default(),
    )
}

fn efi_file(iso: &Path) -> PathBuf {
    let dir = join_rel(iso, "efi\\microsoft\\boot");
    let nop = dir.join("efisys_noprompt.bin");
    if nop.is_file() {
        nop
    } else {
        dir.join("efisys.bin")
    }
}

fn build_with_imapi(sys: &dyn Sys, plan: &Plan, iso: &Path) -> Result<(), String> {
    let bios = join_rel(iso, "boot\\etfsboot.com");
    let efi = efi_file(iso);
    for f in [&bios, &efi] {
        if !f.is_file() {
            return Err(format!("File di avvio mancante nella ISO: {}", f.display()));
        }
    }
    let script = imapi_script(iso, &plan.iso_out, &iso_label(plan), &bios, &efi);
    ps(sys, &script).map(|_| ())
}

fn build_with_oscdimg(sys: &dyn Sys, plan: &Plan, iso: &Path) -> Result<(), String> {
    let efi_rel = if efi_file(iso).ends_with("efisys_noprompt.bin") {
        "efi\\microsoft\\boot\\efisys_noprompt.bin"
    } else {
        "efi\\microsoft\\boot\\efisys.bin"
    };
    let boot = format!("-bootdata:2#p0,e,bboot\\etfsboot.com#pEF,e,b{efi_rel}");
    let script = format!(
        "Set-Location -LiteralPath {}; & {} -m -o -u2 -udfver102 {} {} . {}; if ($LASTEXITCODE -ne 0) {{ exit 1 }}",
        ps_quote(&iso.to_string_lossy()),
        ps_quote(&plan.oscdimg.to_string_lossy()),
        ps_quote(&format!("-l{}", iso_label(plan))),
        ps_quote(&boot),
        ps_quote(&plan.iso_out.to_string_lossy()),
    );
    ps(sys, &script).map(|_| ())
}

fn largest_file_in(dir: &Path) -> u64 {
    fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| e.metadata().ok())
                .filter(|m| m.is_file())
                .map(|m| m.len())
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0)
}

/// Crea il file .iso con il metodo scelto; se IMAPI non riesce, prova oscdimg quando c'è.
fn build_iso_file(
    sys: &dyn Sys,
    plan: &Plan,
    iso: &Path,
    log: &mut dyn FnMut(Kind, &str),
) -> Result<(), String> {
    let first = match plan.method {
        IsoMethod::Imapi => build_with_imapi(sys, plan, iso),
        IsoMethod::Oscdimg => build_with_oscdimg(sys, plan, iso),
    };
    let mut result = first;
    if let (Err(e), IsoMethod::Imapi) = (&result, plan.method) {
        if plan.oscdimg.is_file() {
            log(
                Kind::Warn,
                &format!("IMAPI non è riuscito ({e}): provo con oscdimg"),
            );
            let _ = fs::remove_file(&plan.iso_out);
            result = build_with_oscdimg(sys, plan, iso);
        }
    }
    result?;
    // Controllo: la ISO deve poter contenere almeno il file più grande (install.wim/esd).
    let need = largest_file_in(&iso.join("sources"));
    let got = fs::metadata(&plan.iso_out).map(|m| m.len()).unwrap_or(0);
    if got == 0 {
        return Err("La ISO non è stata creata".into());
    }
    if got < need {
        let _ = fs::remove_file(&plan.iso_out);
        return Err(format!(
            "La ISO creata è troppo piccola ({got} byte, serviva almeno {need}): file troncato, non la tengo"
        ));
    }
    Ok(())
}

/// Esegue tutto. `log` riceve i passaggi.
pub fn build(sys: &dyn Sys, plan: &Plan, log: &mut dyn FnMut(Kind, &str)) -> Result<(), String> {
    validate(plan)?;
    if !sys.is_admin() {
        return Err(
            "Servono i permessi di amministratore (riavvia TweakDesk come amministratore)".into(),
        );
    }
    let root = paths::safe_root(&plan.work);
    if root != plan.work {
        log(
            Kind::Warn,
            &format!(
                "La cartella di lavoro ha spazi, accenti o è troppo lunga: uso {}",
                root.display()
            ),
        );
        fs::create_dir_all(&root).map_err(|e| format!("{}: {e}", root.display()))?;
    }
    let work = root.join(WORK_SUB);
    paths::check_work(&work)?;
    let iso_size = fs::metadata(&plan.iso_in).map(|m| m.len()).unwrap_or(0);
    paths::check_space(sys, &root, iso_size.saturating_mul(3))?;
    let iso = work.join("iso");
    let _ = dism(sys, &["/Cleanup-Wim"]);
    if work.exists() {
        fs::remove_dir_all(&work).map_err(|e| format!("{}: {e}", work.display()))?;
    }
    fs::create_dir_all(&iso).map_err(|e| format!("{}: {e}", iso.display()))?;

    log(Kind::Info, "Monto la ISO e copio i file…");
    let q = ps_quote(&plan.iso_in.to_string_lossy());
    let letter = parse_drive_letter(&ps(
        sys,
        &format!("$i = Mount-DiskImage -ImagePath {q} -PassThru; ($i | Get-Volume).DriveLetter"),
    )?)
    .ok_or("Non riesco a leggere la lettera dell'unità della ISO")?;
    let copy = format!(
        "robocopy '{letter}:\\' {} /E /NFL /NDL /NJH /NJS /NC /NS /NP | Out-Null; if ($LASTEXITCODE -ge 8) {{ exit 1 }} else {{ exit 0 }}",
        ps_quote(&iso.to_string_lossy())
    );
    let copied = ps(sys, &copy);
    let _ = ps(
        sys,
        &format!("Dismount-DiskImage -ImagePath {q} | Out-Null"),
    );
    copied.map_err(|e| format!("Copia della ISO non riuscita: {e}"))?;
    // I file copiati dalla ISO sono di sola lettura.
    let _ = sys.run(
        "attrib",
        &["-R", &format!("{}\\*", iso.display()), "/S", "/D"],
    );

    let r = process(sys, plan, &iso, &work, log);
    if let Err(e) = r {
        let _ = dism(sys, &["/Cleanup-Wim"]);
        return Err(e);
    }
    log(Kind::Info, "Creo la ISO avviabile…");
    build_iso_file(sys, plan, &iso, log)?;
    if plan.delete_work {
        let _ = fs::remove_dir_all(&work);
    }
    log(Kind::Ok, &format!("ISO pronta: {}", plan.iso_out.display()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clean::sys::fake::FakeSys;
    use std::collections::HashMap;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("tdiso_{}_{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn touch(p: &Path) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, b"x").unwrap();
    }

    fn plan(d: &Path) -> Plan {
        touch(&d.join("in.iso"));
        touch(&d.join("oscdimg.exe"));
        fs::create_dir_all(d.join("lavoro")).unwrap();
        Plan {
            iso_in: d.join("in.iso"),
            iso_out: d.join("out.iso"),
            work: d.join("lavoro"),
            editions: vec![6],
            items: vec!["news".into(), "ctx".into()],
            compression: Compression::Max,
            updates: Vec::new(),
            boot_clean: true,
            boot_bypass: true,
            boot_setup_only: true,
            delete_files: true,
            keep_langs: vec!["it-it".into(), "en-us".into()],
            compact_registry: false,
            unattend: Some(unattend::Options::default()),
            method: IsoMethod::Imapi,
            oscdimg: d.join("oscdimg.exe"),
            delete_work: true,
        }
    }

    /// Fa finta che DISM crei il file di destinazione.
    fn hook_export(sys: &FakeSys) {
        sys.hooks.borrow_mut().push((
            "/Export-Image".into(),
            Box::new(|line: &str| {
                if let Some(i) = line.find("/DestinationImageFile:") {
                    let rest = &line[i + "/DestinationImageFile:".len()..];
                    let path = rest.split(" /").next().unwrap_or(rest);
                    let _ = fs::write(path, b"nuovo");
                }
            }),
        ));
    }

    #[test]
    fn parsers() {
        let e = parse_editions("1|Windows 11 Home\r\n6|Windows 11 Pro\r\nrumore\r\n");
        assert_eq!(e.len(), 2);
        assert_eq!(
            e[1],
            Edition {
                index: 6,
                name: "Windows 11 Pro".into()
            }
        );
        assert_eq!(parse_drive_letter("\r\nE\r\n"), Some('E'));
        assert_eq!(parse_drive_letter("errore lungo"), None);
        assert_eq!(parse_drive_letter(""), None);
    }

    #[test]
    fn validation_rules() {
        let d = tmp("val");
        assert!(validate(&plan(&d)).is_ok());
        let bad = |f: &dyn Fn(&mut Plan)| {
            let mut p = plan(&d);
            f(&mut p);
            assert!(validate(&p).is_err());
        };
        bad(&|p| p.iso_in = d.join("manca.iso"));
        bad(&|p| p.iso_out = d.join("out.txt"));
        bad(&|p| p.iso_out = d.join("nuova/out.iso"));
        bad(&|p| p.work = d.join("non-esiste"));
        bad(&|p| p.editions.clear());
        bad(&|p| {
            p.method = IsoMethod::Oscdimg;
            p.oscdimg = d.join("no.exe");
        });
        bad(&|p| p.items = vec!["inventata".into()]);
        bad(&|p| p.items = vec!["power".into()]); // solo sistema installato
        bad(&|p| {
            p.unattend = Some(unattend::Options {
                account_name: "<x>".into(),
                ..unattend::Options::default()
            })
        });
        let mut p = plan(&d);
        touch(&p.iso_out);
        assert!(validate(&p).unwrap_err().contains("esiste già"));
        p.iso_out = p.iso_in.clone();
        assert!(validate(&p).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    struct TestEnv(HashMap<String, String>);
    impl Env for TestEnv {
        fn var(&self, n: &str) -> Option<String> {
            self.0.get(n).cloned()
        }
    }

    fn adk_env(d: &Path) -> TestEnv {
        TestEnv(
            [(
                "ProgramFiles(x86)".to_string(),
                d.join("pf").to_string_lossy().to_string(),
            )]
            .into(),
        )
    }

    fn oscdimg_path(d: &Path) -> PathBuf {
        join_rel(
            &d.join("pf"),
            "Windows Kits\\10\\Assessment and Deployment Kit\\Deployment Tools\\amd64\\Oscdimg\\oscdimg.exe",
        )
    }

    #[test]
    fn adk_install_downloads_then_installs_only_deployment_tools() {
        let d = tmp("adk");
        let sys = FakeSys {
            admin: true,
            ..FakeSys::default()
        };
        let dest = d.join("w").join("adksetup.exe");
        sys.hooks.borrow_mut().push((
            "Invoke-WebRequest".into(),
            Box::new(move |_| {
                let _ = fs::write(&dest, vec![0u8; 2_000_000]);
            }),
        ));
        let exe = oscdimg_path(&d);
        sys.hooks
            .borrow_mut()
            .push(("Start-Process".into(), Box::new(move |_| touch(&exe))));
        let p = adk_install(&sys, &adk_env(&d), &d.join("w"), &mut |_, _| {}).unwrap();
        assert!(p.ends_with("oscdimg.exe"));
        let c = sys.commands.borrow().clone();
        assert!(c[0].contains(ADK_URL) && c[0].contains("Invoke-WebRequest"));
        assert!(c[1].contains("OptionId.DeploymentTools") && c[1].contains("'/quiet'"));
        assert!(
            !d.join("w").join("adksetup.exe").exists(),
            "l'installer scaricato viene tolto"
        );
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn adk_install_is_skipped_when_oscdimg_exists() {
        let d = tmp("adk2");
        touch(&oscdimg_path(&d));
        let sys = FakeSys::default();
        assert!(adk_install(&sys, &adk_env(&d), &d.join("w"), &mut |_, _| {}).is_ok());
        assert!(sys.commands.borrow().is_empty());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn adk_install_needs_admin_and_rejects_bad_downloads() {
        let d = tmp("adk3");
        let sys = FakeSys::default();
        assert!(
            adk_install(&sys, &adk_env(&d), &d.join("w"), &mut |_, _| {})
                .unwrap_err()
                .contains("amministratore")
        );
        let sys = FakeSys {
            admin: true,
            ..FakeSys::default()
        };
        let dest = d.join("w").join("adksetup.exe");
        sys.hooks.borrow_mut().push((
            "Invoke-WebRequest".into(),
            Box::new(move |_| {
                let _ = fs::write(&dest, b"<html>errore</html>");
            }),
        ));
        let e = adk_install(&sys, &adk_env(&d), &d.join("w"), &mut |_, _| {}).unwrap_err();
        assert!(e.contains("non è valido"));
        assert!(!sys
            .commands
            .borrow()
            .iter()
            .any(|c| c.contains("Start-Process")));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn delete_files_keeps_chosen_languages() {
        let d = tmp("del");
        touch(&d.join("autorun.inf"));
        touch(&d.join("support/x.txt"));
        touch(&d.join("sources/it-it/a.mui"));
        touch(&d.join("sources/en-US/a.mui"));
        touch(&d.join("sources/de-de/a.mui"));
        touch(&d.join("sources/install.wim"));
        touch(&d.join("sources/sxs/netfx.cab"));
        let r = delete_iso_files(&d, &["it-IT".into(), "en-us".into()]);
        assert_eq!(r.len(), 3);
        assert!(d.join("sources/it-it").exists() && d.join("sources/en-US").exists());
        assert!(!d.join("sources/de-de").exists());
        assert!(!d.join("autorun.inf").exists() && !d.join("support").exists());
        // Mai toccati: l'immagine e i sorgenti di .NET.
        assert!(d.join("sources/install.wim").exists());
        assert!(d.join("sources/sxs/netfx.cab").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn process_runs_the_whole_chain() {
        let d = tmp("proc");
        let p = plan(&d);
        let iso = d.join("iso");
        touch(&iso.join("sources/install.wim"));
        touch(&iso.join("sources/boot.wim"));
        touch(&iso.join("autorun.inf"));
        let work = d.join("w");
        fs::create_dir_all(&work).unwrap();
        let sys = FakeSys {
            admin: true,
            ..FakeSys::default()
        };
        hook_export(&sys);
        let mut log = Vec::new();
        process(&sys, &p, &iso, &work, &mut |k, m| {
            log.push((k, m.to_string()))
        })
        .unwrap();
        let c = sys.commands.borrow().clone();
        let pos = |s: &str| c.iter().position(|x| x.contains(s)).unwrap_or(usize::MAX);
        assert!(pos("/Mount-Image /ImageFile:") < pos("/Unmount-Image"));
        assert!(c.iter().any(|x| x.contains("/Index:6")));
        assert!(c.iter().any(|x| x.contains("reg load HKLM\\zTD_SOFTWARE")));
        assert!(c
            .iter()
            .any(|x| x.contains("LabConfig") && x.contains("BypassTPMCheck")));
        assert!(c
            .iter()
            .any(|x| x.contains("/SourceIndex:2") && x.contains("boot")));
        assert!(!c
            .iter()
            .any(|x| x.contains("/SourceIndex:1") && x.contains("boot")));
        assert!(c.iter().any(|x| x.contains("/Compress:max")));
        assert!(iso.join("autounattend.xml").is_file());
        assert!(!iso.join("autorun.inf").exists());
        assert_eq!(fs::read(iso.join("sources/install.wim")).unwrap(), b"nuovo");
        assert_eq!(fs::read(iso.join("sources/boot.wim")).unwrap(), b"nuovo");
        assert!(log.iter().all(|(k, _)| *k != Kind::Err));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn esd_compression_replaces_wim_with_esd() {
        let d = tmp("esd");
        let mut p = plan(&d);
        p.compression = Compression::Esd;
        p.boot_clean = false;
        p.boot_bypass = false;
        p.boot_setup_only = false;
        p.unattend = None;
        let iso = d.join("iso");
        touch(&iso.join("sources/install.wim"));
        let work = d.join("w");
        fs::create_dir_all(&work).unwrap();
        let sys = FakeSys {
            admin: true,
            ..FakeSys::default()
        };
        hook_export(&sys);
        process(&sys, &p, &iso, &work, &mut |_, _| {}).unwrap();
        assert!(iso.join("sources/install.esd").is_file());
        assert!(!iso.join("sources/install.wim").exists());
        assert!(sys
            .commands
            .borrow()
            .iter()
            .any(|x| x.contains("/Compress:recovery")));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn esd_source_is_converted_first() {
        let d = tmp("esdsrc");
        let mut p = plan(&d);
        p.boot_clean = false;
        p.boot_bypass = false;
        p.boot_setup_only = false;
        p.unattend = None;
        p.editions = vec![4, 6];
        let iso = d.join("iso");
        touch(&iso.join("sources/install.esd"));
        let work = d.join("w");
        fs::create_dir_all(&work).unwrap();
        let sys = FakeSys {
            admin: true,
            ..FakeSys::default()
        };
        hook_export(&sys);
        process(&sys, &p, &iso, &work, &mut |_, _| {}).unwrap();
        let c = sys.commands.borrow().clone();
        assert!(c
            .iter()
            .any(|x| x.contains("/SourceIndex:4") && x.contains("install.esd")));
        // Dopo la conversione le edizioni sono 1 e 2.
        assert!(c
            .iter()
            .any(|x| x.contains("/Mount-Image") && x.contains("/Index:1")));
        assert!(c
            .iter()
            .any(|x| x.contains("/Mount-Image") && x.contains("/Index:2")));
        assert!(iso.join("sources/install.wim").is_file());
        assert!(!iso.join("sources/install.esd").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn failed_unmount_discards_and_reports() {
        let d = tmp("fail");
        let p = plan(&d);
        let iso = d.join("iso");
        touch(&iso.join("sources/install.wim"));
        let work = d.join("w");
        fs::create_dir_all(&work).unwrap();
        let sys = FakeSys {
            admin: true,
            ..FakeSys::default()
        };
        sys.failing.borrow_mut().push("/Commit".into());
        let r = process(&sys, &p, &iso, &work, &mut |_, _| {});
        assert!(r.is_err());
        assert!(sys.commands.borrow().iter().any(|x| x.contains("/Discard")));
        // L'immagine originale non è stata toccata.
        assert_eq!(fs::read(iso.join("sources/install.wim")).unwrap(), b"x");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn build_needs_admin_and_does_nothing_otherwise() {
        let d = tmp("adm");
        let sys = FakeSys::default();
        let r = build(&sys, &plan(&d), &mut |_, _| {});
        assert!(r.unwrap_err().contains("amministratore"));
        assert!(sys.commands.borrow().is_empty());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn full_build_sequence() {
        let d = tmp("full");
        let p = plan(&d);
        let sys = FakeSys {
            admin: true,
            ..FakeSys::default()
        };
        sys.responses
            .borrow_mut()
            .push(("Mount-DiskImage".into(), "E\r\n".into()));
        let out = p.iso_out.clone();
        let work_iso = p.work.join(WORK_SUB).join("iso");
        let wi = work_iso.clone();
        sys.hooks.borrow_mut().push((
            "robocopy".into(),
            Box::new(move |_| {
                touch(&wi.join("sources/install.wim"));
                touch(&wi.join("sources/boot.wim"));
            }),
        ));
        hook_export(&sys);
        sys.hooks.borrow_mut().push((
            "oscdimg".into(),
            Box::new(move |_| {
                let _ = fs::write(&out, vec![0u8; 100_000]);
            }),
        ));
        let mut p2 = p.clone();
        p2.method = IsoMethod::Oscdimg;
        p2.delete_work = false;
        build(&sys, &p2, &mut |_, _| {}).unwrap();
        let c = sys.commands.borrow().clone();
        let pos = |s: &str| c.iter().position(|x| x.contains(s)).unwrap_or(usize::MAX);
        assert!(pos("/Cleanup-Wim") < pos("robocopy"));
        assert!(pos("robocopy") < pos("/Mount-Image"));
        assert!(pos("/Mount-Image") < pos("oscdimg"));
        let osc = c.iter().find(|x| x.contains("oscdimg")).unwrap();
        assert!(osc.contains(
            "-bootdata:2#p0,e,bboot\\etfsboot.com#pEF,e,befi\\microsoft\\boot\\efisys.bin"
        ));
        assert!(osc.contains("-lout"));
        assert!(c.iter().any(|x| x.contains("Dismount-DiskImage")));
        assert!(p2.iso_out.is_file());
        let _ = fs::remove_dir_all(&d);
    }

    fn full_env(d: &Path, hooks_iso: bool) -> (FakeSys, Plan) {
        let p = plan(d);
        let sys = FakeSys {
            admin: true,
            ..FakeSys::default()
        };
        sys.responses
            .borrow_mut()
            .push(("Mount-DiskImage".into(), "E\r\n".into()));
        let wi = p.work.join(WORK_SUB).join("iso");
        sys.hooks.borrow_mut().push((
            "robocopy".into(),
            Box::new(move |_| {
                touch(&wi.join("sources/install.wim"));
                touch(&wi.join("sources/boot.wim"));
                touch(&wi.join("boot/etfsboot.com"));
                touch(&wi.join("efi/microsoft/boot/efisys_noprompt.bin"));
            }),
        ));
        hook_export(&sys);
        if hooks_iso {
            let out = p.iso_out.clone();
            sys.hooks.borrow_mut().push((
                "IMAPI2FS".into(),
                Box::new(move |_| {
                    let _ = fs::write(&out, b"iso-grande");
                }),
            ));
        }
        (sys, p)
    }

    #[test]
    fn imapi_script_has_udf_and_both_boot_images() {
        let s = imapi_script(
            Path::new("C:\\w\\iso"),
            Path::new("C:\\o.iso"),
            "WIN11",
            Path::new("C:\\w\\iso\\boot\\etfsboot.com"),
            Path::new("C:\\w\\iso\\efi\\microsoft\\boot\\efisys_noprompt.bin"),
        );
        assert!(s.contains("IMAPI2FS.MsftFileSystemImage"));
        assert!(s.contains("FileSystemsToCreate = 4"));
        assert!(s.contains("UDFRevision = 258"));
        assert!(s.contains("etfsboot.com' 0") && s.contains("efisys_noprompt.bin' 239"));
        assert!(s.contains("AddTree('C:\\w\\iso', $false)"));
        assert!(s.contains("[TdIso]::Create('C:\\o.iso'"));
        assert!(!s.contains("unsafe"));
    }

    #[test]
    fn imapi_build_is_the_default_path() {
        let d = tmp("imapi");
        let (sys, mut p) = full_env(&d, true);
        p.delete_work = false;
        build(&sys, &p, &mut |_, _| {}).unwrap();
        let c = sys.commands.borrow().clone();
        assert!(c.iter().any(|x| x.contains("IMAPI2FS.MsftFileSystemImage")));
        assert!(!c.iter().any(|x| x.contains("oscdimg")));
        assert!(p.iso_out.is_file());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn imapi_failure_falls_back_to_oscdimg() {
        let d = tmp("fallback");
        let (sys, mut p) = full_env(&d, false);
        p.delete_work = false;
        sys.failing.borrow_mut().push("IMAPI2FS".into());
        let out = p.iso_out.clone();
        sys.hooks.borrow_mut().push((
            "oscdimg".into(),
            Box::new(move |_| {
                let _ = fs::write(&out, b"iso-grande");
            }),
        ));
        let mut log = Vec::new();
        build(&sys, &p, &mut |k, m| log.push((k, m.to_string()))).unwrap();
        assert!(log
            .iter()
            .any(|(k, m)| *k == Kind::Warn && m.contains("provo con oscdimg")));
        assert!(sys.commands.borrow().iter().any(|x| x.contains("oscdimg")));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn imapi_failure_without_oscdimg_is_an_error() {
        let d = tmp("nofallback");
        let (sys, mut p) = full_env(&d, false);
        p.delete_work = false;
        let _ = fs::remove_file(&p.oscdimg);
        sys.failing.borrow_mut().push("IMAPI2FS".into());
        assert!(build(&sys, &p, &mut |_, _| {}).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn truncated_iso_is_rejected_and_removed() {
        let d = tmp("small");
        let (sys, mut p) = full_env(&d, false);
        p.delete_work = false;
        // install.wim finto più grande della ISO prodotta.
        let out = p.iso_out.clone();
        let wi = p
            .work
            .join(WORK_SUB)
            .join("iso")
            .join("sources")
            .join("install.wim");
        sys.hooks.borrow_mut().push((
            "IMAPI2FS".into(),
            Box::new(move |_| {
                let _ = fs::write(&wi, vec![0u8; 5000]);
                let _ = fs::write(&out, b"x");
            }),
        ));
        let e = build(&sys, &p, &mut |_, _| {}).unwrap_err();
        assert!(e.contains("troppo piccola"));
        assert!(!p.iso_out.exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn updates_are_staged_with_plain_names_and_added_before_items() {
        let d = tmp("upd");
        let (sys, mut p) = full_env(&d, true);
        p.delete_work = false;
        touch(&d.join("kb1-x64.msu"));
        touch(&d.join("Aggiornamento è.msu"));
        p.updates = vec![d.join("kb1-x64.msu"), d.join("Aggiornamento è.msu")];
        build(&sys, &p, &mut |_, _| {}).unwrap();
        let upd = p.work.join(WORK_SUB).join("upd");
        assert!(upd.join("kb1-x64.msu").is_file());
        assert!(upd.join("pkg_02.msu").is_file());
        let c = sys.commands.borrow().clone();
        let a = c.iter().position(|x| x.contains("/Add-Package")).unwrap();
        assert!(c[a].contains("/PackagePath:"));
        assert!(c[a].contains("/Image:"));
        assert!(c.iter().any(|x| x.contains("/StartComponentCleanup")));
        let m = c.iter().position(|x| x.contains("/Mount-Image")).unwrap();
        assert!(m < a);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn failed_update_discards_the_image_and_stops() {
        let d = tmp("updfail");
        let (sys, mut p) = full_env(&d, true);
        p.delete_work = false;
        touch(&d.join("kb1.msu"));
        p.updates = vec![d.join("kb1.msu")];
        sys.failing.borrow_mut().push("/Add-Package".into());
        let e = build(&sys, &p, &mut |_, _| {}).unwrap_err();
        assert!(e.contains("Aggiornamenti non integrati"));
        let c = sys.commands.borrow().clone();
        assert!(c
            .iter()
            .any(|x| x.contains("/Unmount-Image") && x.contains("/Discard")));
        assert!(!p.iso_out.exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn invalid_update_files_are_rejected() {
        let d = tmp("updbad");
        let mut p = plan(&d);
        p.updates = vec![d.join("manca.msu")];
        assert!(validate(&p).is_err());
        touch(&d.join("x.exe"));
        p.updates = vec![d.join("x.exe")];
        assert!(validate(&p).unwrap_err().contains(".msu"));
        touch(&d.join("ok.CAB"));
        p.updates = vec![d.join("ok.CAB")];
        assert!(validate(&p).is_ok());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn low_disk_space_stops_before_copying() {
        let d = tmp("space");
        let (sys, p) = full_env(&d, true);
        sys.responses
            .borrow_mut()
            .push(("Get-PSDrive".into(), "10".into()));
        // Su Linux il percorso non ha lettera: il controllo non si applica, il build prosegue.
        assert!(build(&sys, &p, &mut |_, _| {}).is_ok());
        let _ = fs::remove_dir_all(&d);
    }
}
