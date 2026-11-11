//! Interfaccia: due sottoschede, «Windows installato» e «ISO di Windows 11».

use crate::clean::engine::RealEnv;
use crate::clean::sys::{RealSys, Sys};
use crate::debloat::apply::{self, Kind};
use crate::debloat::catalog::{items_for, Group, Item, Level};
use crate::debloat::iso::{self, Compression, Edition, IsoMethod, Plan};
use crate::debloat::netfix;
use crate::debloat::target::Target;
use crate::debloat::unattend;
use crate::debloat::uup;
use crate::install::config::app_dir;
use crate::install::setup::Set;
use crate::install::setup_panel::SetupPanel;
use crate::theme;
use eframe::egui::{self, Color32, RichText};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};

enum Event {
    Log(Kind, String),
    Editions(Result<Vec<Edition>, String>),
    Oscdimg(PathBuf),
    IsoIn(PathBuf),
    Finished,
}

/// Parte del pannello mostrata (scelta dalle sotto-schede in `app.rs`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Installed,
    Iso,
    Scripts,
}

type Checked = HashMap<&'static str, bool>;

/// Spunta le voci fino al livello scelto (Sicuro < Standard < Estremo).
pub fn preset(iso: bool, up_to: Option<Level>) -> Checked {
    items_for(iso)
        .map(|i| (i.id, up_to.is_some_and(|l| i.level <= l)))
        .collect()
}

fn kind_color(k: Kind) -> Color32 {
    match k {
        Kind::Info => theme::LOG_INFO,
        Kind::Ok => theme::LOG_OK,
        Kind::Warn => theme::LOG_WARN,
        Kind::Err => theme::LOG_ERR,
    }
}

pub struct DebloatPanel {
    sub: Part,
    setup: SetupPanel,
    inst: Checked,
    iso_items: Checked,
    log: Vec<(Color32, String)>,
    busy: bool,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    is_admin: bool,
    confirm: bool,
    // ISO
    iso_in: String,
    iso_out: String,
    work: String,
    oscdimg: String,
    method: IsoMethod,
    editions: Vec<(Edition, bool)>,
    compression: Compression,
    updates: Vec<String>,
    uup_zip: String,
    boot_clean: bool,
    boot_bypass: bool,
    boot_setup_only: bool,
    delete_files: bool,
    compact_registry: bool,
    delete_work: bool,
    use_unattend: bool,
    un: unattend::Options,
}

impl DebloatPanel {
    pub fn new() -> Self {
        let (tx, rx) = channel();
        let base = app_dir();
        let oscdimg = iso::find_oscdimg(&RealEnv)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        Self {
            sub: Part::Installed,
            setup: SetupPanel::new(&base, Set::Optimize),
            inst: preset(false, Some(Level::Standard)),
            iso_items: preset(true, Some(Level::Standard)),
            log: Vec::new(),
            busy: false,
            tx,
            rx,
            is_admin: RealSys.is_admin(),
            confirm: false,
            iso_in: String::new(),
            iso_out: base
                .join("Windows11-TweakDesk.iso")
                .to_string_lossy()
                .to_string(),
            work: base.to_string_lossy().to_string(),
            oscdimg,
            method: IsoMethod::Imapi,
            editions: Vec::new(),
            compression: Compression::Max,
            updates: Vec::new(),
            uup_zip: String::new(),
            boot_clean: true,
            boot_bypass: true,
            boot_setup_only: true,
            delete_files: true,
            compact_registry: false,
            delete_work: true,
            use_unattend: true,
            un: unattend::Options::default(),
        }
    }

    fn say(&mut self, c: Color32, m: impl Into<String>) {
        self.log.push((c, m.into()));
        if self.log.len() > 800 {
            self.log.remove(0);
        }
    }

    fn pump(&mut self) {
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                Event::Log(k, m) => self.say(kind_color(k), m),
                Event::Editions(r) => match r {
                    Ok(list) => {
                        let pick = list
                            .iter()
                            .position(|e| e.name.contains("Pro") && !e.name.contains(" N"))
                            .unwrap_or(0);
                        self.editions = list
                            .into_iter()
                            .enumerate()
                            .map(|(i, e)| (e, i == pick))
                            .collect();
                        self.say(
                            theme::LOG_OK,
                            format!("{} edizioni trovate.", self.editions.len()),
                        );
                    }
                    Err(e) => self.say(
                        theme::LOG_ERR,
                        format!("Lettura della ISO non riuscita: {e}"),
                    ),
                },
                Event::IsoIn(p) => self.iso_in = p.to_string_lossy().to_string(),
                Event::Oscdimg(p) => self.oscdimg = p.to_string_lossy().to_string(),
                Event::Finished => self.busy = false,
            }
        }
    }

    fn spawn(&mut self, ctx: &egui::Context, job: impl FnOnce(&Sender<Event>) + Send + 'static) {
        self.busy = true;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            job(&tx);
            let _ = tx.send(Event::Finished);
            ctx.request_repaint();
        });
    }

    fn chosen(items: &Checked, iso: bool) -> Vec<&'static Item> {
        items_for(iso)
            .filter(|i| items.get(i.id).copied().unwrap_or(false))
            .collect()
    }

    fn apply_installed(&mut self, ctx: &egui::Context) {
        let items = Self::chosen(&self.inst, false);
        self.say(theme::LOG_INFO, format!("Applico {} voci…", items.len()));
        self.spawn(ctx, move |tx| {
            let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
            let target = Target::online(PathBuf::from(root));
            let t2 = tx.clone();
            let (rep, undo) = apply::apply(&items, &target, &RealSys, &mut |k, m| {
                let _ = t2.send(Event::Log(k, m.to_string()));
            });
            let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
            let file = app_dir().join("BackupDebloat").join(format!("annulla-{stamp}.json"));
            let msg = match apply::save_undo(&undo, &file) {
                Ok(()) if !undo.is_empty() => format!("Per tornare indietro (registro e servizi): pulsante «Annulla». {} modifiche salvate.", undo.len()),
                Ok(()) => String::new(),
                Err(e) => format!("Annulla non salvato: {e}"),
            };
            let _ = tx.send(Event::Log(
                Kind::Ok,
                format!("Finito: {} fatte, {} senza effetto, {} fallite. {msg}", rep.done, rep.skipped, rep.failed),
            ));
        });
    }

    fn latest_undo() -> Option<PathBuf> {
        let dir = app_dir().join("BackupDebloat");
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .collect();
        files.sort();
        files.pop()
    }

    fn undo_last(&mut self, ctx: &egui::Context) {
        let Some(file) = Self::latest_undo() else {
            self.say(theme::LOG_WARN, "Niente da annullare.");
            return;
        };
        self.spawn(ctx, move |tx| match apply::load_undo(&file) {
            Ok(steps) => {
                let rep = apply::undo(&steps, &RealSys);
                let _ = tx.send(Event::Log(
                    Kind::Ok,
                    format!(
                        "Annullate {} modifiche ({} non riuscite).",
                        rep.done, rep.failed
                    ),
                ));
                let _ = std::fs::rename(&file, file.with_extension("json.fatto"));
            }
            Err(e) => {
                let _ = tx.send(Event::Log(Kind::Err, e));
            }
        });
    }

    fn install_adk(&mut self, ctx: &egui::Context) {
        let work = PathBuf::from(self.work.trim());
        self.say(
            theme::LOG_INFO,
            "Installo il Windows ADK (Deployment Tools)…",
        );
        self.spawn(ctx, move |tx| {
            let t2 = tx.clone();
            match iso::adk_install(&RealSys, &RealEnv, &work, &mut |k, m| {
                let _ = t2.send(Event::Log(k, m.to_string()));
            }) {
                Ok(p) => {
                    let _ = tx.send(Event::Oscdimg(p));
                }
                Err(e) => {
                    let _ = tx.send(Event::Log(Kind::Err, e));
                }
            }
        });
    }

    fn run_uup(&mut self, ctx: &egui::Context) {
        let zip = PathBuf::from(self.uup_zip.trim());
        let base = PathBuf::from(self.work.trim());
        self.say(theme::LOG_INFO, "Avvio UUP dump…");
        self.spawn(ctx, move |tx| {
            let t2 = tx.clone();
            match uup::run(&RealSys, &zip, &base, &netfix::hosts_path(), &mut |k, m| {
                let _ = t2.send(Event::Log(k, m.to_string()));
            }) {
                Ok(p) => {
                    let _ = tx.send(Event::IsoIn(p));
                }
                Err(e) => {
                    let _ = tx.send(Event::Log(Kind::Err, format!("UUP dump: {e}")));
                }
            }
        });
    }

    fn net_job(&mut self, ctx: &egui::Context, what: u8) {
        self.say(theme::LOG_INFO, "Controllo la rete…");
        self.spawn(ctx, move |tx| {
            let log = |k: Kind, m: String| {
                let _ = tx.send(Event::Log(k, m));
            };
            let r = match what {
                0 => netfix::preflight(&RealSys, &netfix::hosts_path(), &mut |k, m| {
                    log(k, m.to_string())
                }),
                1 => netfix::set_public_dns(&RealSys)
                    .map(|_| log(Kind::Ok, "DNS impostati: 1.1.1.1 e 8.8.8.8".into())),
                _ => netfix::reset_dns(&RealSys)
                    .map(|_| log(Kind::Ok, "DNS automatici ripristinati".into())),
            };
            if let Err(e) = r {
                log(Kind::Err, e);
            }
        });
    }

    fn read_editions(&mut self, ctx: &egui::Context) {
        let path = PathBuf::from(self.iso_in.trim());
        if !path.is_file() {
            self.say(theme::LOG_ERR, "Scegli prima il file .iso");
            return;
        }
        self.say(theme::LOG_INFO, "Leggo le edizioni della ISO…");
        self.spawn(ctx, move |tx| {
            let _ = tx.send(Event::Editions(iso::list_editions(&RealSys, &path)));
        });
    }

    fn plan(&self) -> Plan {
        Plan {
            iso_in: PathBuf::from(self.iso_in.trim()),
            iso_out: PathBuf::from(self.iso_out.trim()),
            work: PathBuf::from(self.work.trim()),
            editions: self
                .editions
                .iter()
                .filter(|(_, on)| *on)
                .map(|(e, _)| e.index)
                .collect(),
            items: Self::chosen(&self.iso_items, true)
                .iter()
                .map(|i| i.id.to_string())
                .collect(),
            compression: self.compression,
            updates: self.updates.iter().map(PathBuf::from).collect(),
            boot_clean: self.boot_clean,
            boot_bypass: self.boot_bypass,
            boot_setup_only: self.boot_setup_only,
            delete_files: self.delete_files,
            keep_langs: vec![self.un.language.clone(), "en-US".into()],
            compact_registry: self.compact_registry,
            unattend: self.use_unattend.then(|| self.un.clone()),
            method: self.method,
            oscdimg: PathBuf::from(self.oscdimg.trim()),
            delete_work: self.delete_work,
        }
    }

    fn build_iso(&mut self, ctx: &egui::Context) {
        let plan = self.plan();
        if let Err(e) = iso::validate(&plan) {
            self.say(theme::LOG_ERR, e);
            return;
        }
        self.say(
            theme::LOG_INFO,
            "Avvio la creazione della ISO (può richiedere molto tempo)…",
        );
        self.spawn(ctx, move |tx| {
            let t2 = tx.clone();
            let r = iso::build(&RealSys, &plan, &mut |k, m| {
                let _ = t2.send(Event::Log(k, m.to_string()));
            });
            if let Err(e) = r {
                let _ = tx.send(Event::Log(Kind::Err, format!("ISO non creata: {e}")));
            }
        });
    }

    fn big_button(ui: &mut egui::Ui, text: &str, color: Color32, enabled: bool) -> bool {
        let b = egui::Button::new(
            RichText::new(text)
                .size(17.0)
                .strong()
                .color(theme::ON_COLOR),
        )
        .fill(color)
        .min_size(egui::vec2(200.0, 40.0));
        ui.add_enabled(enabled, b).clicked()
    }

    fn items_ui(ui: &mut egui::Ui, iso: bool, checked: &mut Checked) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Scelta rapida:").strong());
            if ui.button("Sicuro").clicked() {
                *checked = preset(iso, Some(Level::Sicuro));
            }
            if ui.button("Standard (include il Sicuro)").clicked() {
                *checked = preset(iso, Some(Level::Standard));
            }
            if ui.button("Estremo (tutto)").clicked() {
                *checked = preset(iso, Some(Level::Estremo));
            }
            if ui.button("Nessuna").clicked() {
                *checked = preset(iso, None);
            }
            let n = checked.values().filter(|v| **v).count();
            ui.label(RichText::new(format!("{n} voci scelte")).color(theme::MUTED));
        });
        ui.add_space(4.0);
        for level in [Level::Sicuro, Level::Standard, Level::Estremo] {
            let color = match level {
                Level::Sicuro => Color32::from_rgb(0x1E, 0x6B, 0x3A),
                Level::Standard => Color32::from_rgb(0x1F, 0x4E, 0x8C),
                Level::Estremo => Color32::from_rgb(0x9B, 0x2C, 0x2C),
            };
            egui::CollapsingHeader::new(
                RichText::new(level.title())
                    .strong()
                    .size(16.0)
                    .color(color),
            )
            .id_salt(("lvl", iso, level.title()))
            .default_open(level != Level::Estremo)
            .show(ui, |ui| {
                for g in Group::ALL {
                    let items: Vec<&'static Item> = items_for(iso)
                        .filter(|i| i.level == level && i.group == g)
                        .collect();
                    if items.is_empty() {
                        continue;
                    }
                    ui.label(RichText::new(g.title()).color(theme::MUTED).small());
                    for i in items {
                        let mut on = checked.get(i.id).copied().unwrap_or(false);
                        if ui.checkbox(&mut on, i.label).changed() {
                            checked.insert(i.id, on);
                        }
                        if !i.warn.is_empty() {
                            ui.label(
                                RichText::new(format!("      ! {}", i.warn))
                                    .small()
                                    .color(theme::WARN),
                            );
                        }
                    }
                    ui.add_space(3.0);
                }
            });
        }
    }

    fn path_row(
        ui: &mut egui::Ui,
        label: &str,
        value: &mut String,
        pick: impl FnOnce() -> Option<PathBuf>,
    ) {
        ui.label(label);
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(value).desired_width(480.0));
            if ui.button("Scegli…").clicked() {
                if let Some(p) = pick() {
                    *value = p.to_string_lossy().to_string();
                }
            }
        });
        ui.end_row();
    }

    fn installed_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            let n = Self::chosen(&self.inst, false).len();
            if Self::big_button(
                ui,
                "Applica",
                theme::INSTALL,
                !self.busy && n > 0 && self.is_admin,
            ) {
                self.confirm = true;
            }
            if Self::latest_undo().is_some()
                && ui
                    .add_enabled(
                        !self.busy,
                        egui::Button::new("Annulla l'ultima applicazione"),
                    )
                    .clicked()
            {
                self.undo_last(ctx);
            }
            if !self.is_admin {
                ui.label(RichText::new("Servono i permessi di amministratore.").color(theme::WARN));
            }
        });
        ui.label(
            RichText::new("Agisce sul Windows in uso. «Annulla» ripristina registro e servizi; app, funzionalità e file tolti non si recuperano. Nessun punto di ripristino viene creato.")
                .color(theme::MUTED),
        );
        ui.add_space(4.0);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                Self::items_ui(ui, false, &mut self.inst);
            });
    }

    fn iso_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("iso_grid").num_columns(1).spacing([8.0, 4.0]).show(ui, |ui| {
                Self::path_row(ui, "ISO di Windows 11 da modificare:", &mut self.iso_in, || {
                    rfd::FileDialog::new().add_filter("ISO", &["iso"]).pick_file()
                });
                Self::path_row(ui, "ISO da creare:", &mut self.iso_out, || {
                    rfd::FileDialog::new().add_filter("ISO", &["iso"]).set_file_name("Windows11-TweakDesk.iso").save_file()
                });
                Self::path_row(ui, "Cartella di lavoro (serve spazio per circa 3 volte la ISO):", &mut self.work, || {
                    rfd::FileDialog::new().pick_folder()
                });
                ui.horizontal(|ui| {
                    ui.label("Metodo per creare la ISO:");
                    egui::ComboBox::from_id_salt("iso_method")
                        .selected_text(self.method.label())
                        .show_ui(ui, |ui| {
                            for m in IsoMethod::ALL {
                                ui.selectable_value(&mut self.method, m, m.label());
                            }
                        });
                });
                Self::path_row(ui, "oscdimg.exe (solo per il metodo ADK o come ripiego se IMAPI non riesce):", &mut self.oscdimg, || {
                    rfd::FileDialog::new().add_filter("oscdimg", &["exe"]).pick_file()
                });
            });
            if self.method == IsoMethod::Oscdimg && self.oscdimg.trim().is_empty() {
                ui.label(RichText::new("oscdimg.exe non trovato: scarica e installa il Windows ADK (solo «Deployment Tools») con il pulsante, oppure indica il file.").color(theme::WARN));
            }
            ui.horizontal(|ui| {
                let can = !self.busy && self.is_admin;
                if ui
                    .add_enabled(can, egui::Button::new(RichText::new("Scarica e installa ADK (Deployment Tools)").strong()).fill(theme::DOWNLOAD))
                    .clicked()
                {
                    self.install_adk(ctx);
                }
                if ui.button("Apri la pagina ufficiale dell'ADK").clicked() {
                    let _ = RealSys.run("cmd", &["/c", "start", "", iso::ADK_PAGE]);
                }
                if !self.is_admin {
                    ui.label(RichText::new("Servono i permessi di amministratore.").color(theme::WARN));
                }
            });
            ui.label(RichText::new("Se la cartella di lavoro ha spazi, accenti o è lunga, uso automaticamente una cartella corta (es. C:\\TDwork).").color(theme::MUTED));
            ui.add_space(6.0);
            egui::CollapsingHeader::new(RichText::new("ISO da UUP dump (ultima versione con gli aggiornamenti)").strong().size(16.0))
                .default_open(false)
                .show(ui, |ui| {
                    ui.label(RichText::new("Su uupdump.net scegli la versione e «Scarica e converti in ISO», con «Includi aggiornamenti»; poi indica qui lo zip scaricato.").color(theme::MUTED));
                    egui::Grid::new("uup_grid").num_columns(1).show(ui, |ui| {
                        Self::path_row(ui, "Zip di UUP dump:", &mut self.uup_zip, || {
                            rfd::FileDialog::new().add_filter("zip", &["zip"]).pick_file()
                        });
                    });
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(!self.busy && self.is_admin, egui::Button::new(RichText::new("Crea la ISO con UUP dump").strong()).fill(theme::DOWNLOAD))
                            .clicked()
                        {
                            self.run_uup(ctx);
                        }
                        if ui.add_enabled(!self.busy, egui::Button::new("Controlla la rete")).clicked() {
                            self.net_job(ctx, 0);
                        }
                        if ui.add_enabled(!self.busy && self.is_admin, egui::Button::new("Imposta DNS pubblici")).clicked() {
                            self.net_job(ctx, 1);
                        }
                        if ui.add_enabled(!self.busy && self.is_admin, egui::Button::new("DNS automatici")).clicked() {
                            self.net_job(ctx, 2);
                        }
                        if ui.button("Apri uupdump.net").clicked() {
                            let _ = RealSys.run("cmd", &["/c", "start", "", "https://uupdump.net"]);
                        }
                    });
                });
            egui::CollapsingHeader::new(RichText::new("Aggiornamenti da integrare (.msu / .cab)").strong().size(16.0))
                .default_open(false)
                .show(ui, |ui| {
                    ui.label(RichText::new("Vengono integrati in ogni edizione prima delle voci scelte. Se un aggiornamento fallisce, la creazione si ferma.").color(theme::MUTED));
                    ui.horizontal(|ui| {
                        if ui.button("Aggiungi file…").clicked() {
                            if let Some(files) = rfd::FileDialog::new().add_filter("Aggiornamenti", &["msu", "cab"]).pick_files() {
                                for f in files {
                                    let s = f.to_string_lossy().to_string();
                                    if !self.updates.contains(&s) {
                                        self.updates.push(s);
                                    }
                                }
                            }
                        }
                        if ui.button("Apri il Catalogo di Microsoft Update").clicked() {
                            let _ = RealSys.run("cmd", &["/c", "start", "", "https://www.catalog.update.microsoft.com"]);
                        }
                    });
                    let mut remove = None;
                    for (i, u) in self.updates.iter().enumerate() {
                        ui.horizontal(|ui| {
                            if ui.small_button("Togli").clicked() {
                                remove = Some(i);
                            }
                            ui.label(u);
                        });
                    }
                    if let Some(i) = remove {
                        self.updates.remove(i);
                    }
                });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(!self.busy, egui::Button::new("Leggi le edizioni della ISO")).clicked() {
                    self.read_editions(ctx);
                }
                if !self.is_admin {
                    ui.label(RichText::new("Servono i permessi di amministratore.").color(theme::WARN));
                }
            });
            if !self.editions.is_empty() {
                ui.label(RichText::new("Edizioni da tenere (le altre vengono tolte):").strong());
                for (e, on) in &mut self.editions {
                    ui.checkbox(on, format!("{}  (indice {})", e.name, e.index));
                }
            }
            ui.add_space(6.0);
            egui::CollapsingHeader::new(RichText::new("Immagine e file").strong().size(16.0))
                .default_open(true)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Compressione:");
                        egui::ComboBox::from_id_salt("iso_comp")
                            .selected_text(self.compression.label())
                            .show_ui(ui, |ui| {
                                for c in Compression::ALL {
                                    ui.selectable_value(&mut self.compression, c, c.label());
                                }
                            });
                    });
                    ui.checkbox(&mut self.boot_clean, "boot.wim: pulisci i componenti superati");
                    ui.checkbox(&mut self.boot_bypass, "boot.wim: salta i controlli di TPM, Secure Boot, RAM e CPU nell'installazione");
                    ui.checkbox(&mut self.boot_setup_only, "boot.wim: tieni solo Setup (indice 2), togli WinPE (indice 1)");
                    ui.checkbox(&mut self.delete_files, "Togli dalla ISO file inutili (autorun, support, lingue non usate)");
                    ui.checkbox(&mut self.compact_registry, "Compatta i file di registro (guadagno piccolo)");
                    ui.checkbox(&mut self.delete_work, "Cancella la cartella di lavoro alla fine");
                });
            egui::CollapsingHeader::new(RichText::new("Installazione automatica (file di risposte)").strong().size(16.0))
                .default_open(true)
                .show(ui, |ui| {
                    ui.checkbox(&mut self.use_unattend, "Aggiungi autounattend.xml alla ISO");
                    if self.use_unattend {
                        ui.checkbox(&mut self.un.local_account, "Account locale: salta l'account Microsoft e la connessione a internet");
                        ui.add_enabled_ui(self.un.local_account, |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Nome utente:");
                                ui.add(egui::TextEdit::singleline(&mut self.un.account_name).desired_width(160.0));
                                ui.label("Password (vuota = nessuna):");
                                ui.add(egui::TextEdit::singleline(&mut self.un.account_password).password(true).desired_width(160.0));
                            });
                        });
                        ui.horizontal(|ui| {
                            ui.label("Nome del PC (vuoto = automatico):");
                            ui.add(egui::TextEdit::singleline(&mut self.un.computer_name).desired_width(160.0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Lingua:");
                            ui.add(egui::TextEdit::singleline(&mut self.un.language).desired_width(70.0));
                            ui.label("Tastiera:");
                            ui.add(egui::TextEdit::singleline(&mut self.un.keyboard).desired_width(110.0));
                            ui.label("Fuso orario:");
                            ui.add(egui::TextEdit::singleline(&mut self.un.timezone).desired_width(200.0));
                        });
                        ui.checkbox(&mut self.un.bypass_checks, "Salta i controlli di installazione (anche nel file di risposte)");
                        ui.checkbox(&mut self.un.skip_screens, "Salta le schermate iniziali (licenza, privacy, rete)");
                        ui.checkbox(&mut self.un.prevent_encryption, "Niente cifratura automatica del disco");
                        ui.checkbox(&mut self.un.compact_os, "Compact OS dopo la prima installazione (risparmia circa 2 GB)");
                        ui.label(RichText::new("La password resta scritta in chiaro dentro la ISO.").small().color(theme::WARN));
                    }
                });
            ui.add_space(6.0);
            ui.label(RichText::new("Voci da applicare a ogni edizione").strong().size(16.0));
            Self::items_ui(ui, true, &mut self.iso_items);
            ui.add_space(10.0);
            let ready = !self.busy
                && self.is_admin
                && self.editions.iter().any(|(_, on)| *on);
            if Self::big_button(ui, "Crea la ISO", theme::DOWNLOAD, ready) {
                self.build_iso(ctx);
            }
            if self.editions.is_empty() {
                ui.label(RichText::new("Prima leggi le edizioni della ISO.").color(theme::MUTED));
            }
            ui.add_space(8.0);
        });
    }

    pub fn show(&mut self, ui: &mut egui::Ui, part: Part) {
        self.sub = part;
        self.pump();
        let msgs = std::mem::take(&mut self.setup.msgs);
        self.log.extend(msgs);
        let ctx = ui.ctx().clone();
        if self.busy {
            egui::Panel::top("debloat_top").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new("Lavoro in corso…").color(theme::MUTED));
                });
            });
        }
        egui::Panel::bottom("debloat_log").show(ui, |ui| {
            egui::Frame::new()
                .fill(theme::LOG_BG)
                .inner_margin(6.0)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(120.0)
                        .auto_shrink([false, true])
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            for (c, m) in &self.log {
                                ui.label(RichText::new(m).color(*c).monospace());
                            }
                        });
                });
        });
        egui::CentralPanel::default().show(ui, |ui| match self.sub {
            Part::Installed => self.installed_tab(ui, &ctx),
            Part::Iso => self.iso_tab(ui, &ctx),
            Part::Scripts => self.setup.ui(ui),
        });
        if self.confirm {
            let items = Self::chosen(&self.inst, false);
            egui::Window::new("Applicare le modifiche?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(&ctx, |ui| {
                    ui.label(format!("Verranno applicate {} voci a questo Windows. Non viene creato nessun punto di ripristino.", items.len()));
                    for i in items.iter().filter(|i| !i.warn.is_empty()) {
                        ui.label(RichText::new(format!("{}: {}", i.label, i.warn)).small().color(theme::WARN));
                    }
                    ui.horizontal(|ui| {
                        if Self::big_button(ui, "Applica", theme::INSTALL, true) {
                            self.confirm = false;
                            self.apply_installed(&ctx);
                        }
                        if ui.button("Annulla").clicked() {
                            self.confirm = false;
                        }
                    });
                });
        }
        if self.busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_follow_levels() {
        let s = preset(false, Some(Level::Sicuro));
        for i in items_for(false) {
            assert_eq!(s[i.id], i.level == Level::Sicuro, "{}", i.id);
        }
        let st = preset(false, Some(Level::Standard));
        for i in items_for(false) {
            assert_eq!(st[i.id], i.level != Level::Estremo, "{}", i.id);
        }
        assert!(preset(true, Some(Level::Estremo)).values().all(|v| *v));
        assert!(preset(true, None).values().all(|v| !*v));
    }

    #[test]
    fn iso_presets_only_contain_iso_items() {
        let p = preset(true, Some(Level::Estremo));
        assert!(p.contains_key("edge"));
        assert!(!p.contains_key("power"));
        assert!(preset(false, Some(Level::Estremo)).contains_key("power"));
    }
}
