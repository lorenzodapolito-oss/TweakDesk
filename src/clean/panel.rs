//! Interfaccia: Pulizia, Avvio automatico e Backup di sistema.

use crate::clean::backup::{self, Compression, Plan};
use crate::clean::engine::{self, human, RealEnv, Stats};
use crate::clean::startup::{self, Entry, Source};
use crate::clean::sys::{RealSys, Sys};
use crate::clean::targets::{Group, Target, TARGETS};
use crate::install::config::app_dir;
use crate::theme;
use eframe::egui::{self, Color32, RichText};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};

enum Event {
    Log(Color32, String),
    Sizes(Vec<(&'static str, Stats)>),
    Cleaned { freed: u64, failed: u64 },
    Startup(Vec<Entry>),
    Backup(Result<u64, String>),
    Done,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Tab {
    Clean,
    Startup,
    Backup,
}

pub struct CleanPanel {
    tab: Tab,
    checked: HashMap<&'static str, bool>,
    sizes: HashMap<&'static str, Stats>,
    startup: Vec<Entry>,
    startup_loaded: bool,
    show_system: bool,
    log: Vec<(Color32, String)>,
    busy: bool,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    confirm_clean: bool,
    confirm_delete: Option<usize>,
    is_admin: bool,
    bk_drive: String,
    bk_image: String,
    bk_name: String,
    bk_comp: Compression,
}

fn default_image() -> String {
    let base = app_dir();
    base.join("install.wim").to_string_lossy().to_string()
}

impl CleanPanel {
    pub fn set_tab(&mut self, t: Tab) {
        self.tab = t;
    }

    pub fn new() -> Self {
        let (tx, rx) = channel();
        let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        Self {
            tab: Tab::Clean,
            checked: TARGETS.iter().map(|t| (t.id, t.default_on)).collect(),
            sizes: HashMap::new(),
            startup: Vec::new(),
            startup_loaded: false,
            show_system: false,
            log: Vec::new(),
            busy: false,
            tx,
            rx,
            confirm_clean: false,
            confirm_delete: None,
            is_admin: RealSys.is_admin(),
            bk_drive: drive,
            bk_image: default_image(),
            bk_name: "Windows 11".into(),
            bk_comp: Compression::Max,
        }
    }

    fn say(&mut self, c: Color32, m: impl Into<String>) {
        self.log.push((c, m.into()));
        if self.log.len() > 500 {
            self.log.remove(0);
        }
    }

    fn selected(&self) -> Vec<&'static Target> {
        TARGETS
            .iter()
            .filter(|t| self.checked.get(t.id).copied().unwrap_or(false))
            .collect()
    }

    fn pump(&mut self) {
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                Event::Log(c, m) => self.say(c, m),
                Event::Sizes(v) => {
                    self.sizes = v.into_iter().collect();
                    let total: u64 = self
                        .selected()
                        .iter()
                        .map(|t| self.sizes.get(t.id).map_or(0, |s| s.bytes))
                        .sum();
                    self.say(
                        theme::LOG_OK,
                        format!(
                            "Analisi finita: si possono liberare circa {}.",
                            human(total)
                        ),
                    );
                }
                Event::Cleaned { freed, failed } => {
                    self.say(
                        theme::LOG_OK,
                        format!("Pulizia finita: liberati {}.", human(freed)),
                    );
                    if failed > 0 {
                        self.say(
                            theme::LOG_WARN,
                            format!("{failed} file non si sono potuti eliminare (in uso: chiudi i programmi e riprova)."),
                        );
                    }
                    self.sizes.clear();
                }
                Event::Startup(v) => {
                    self.startup = v;
                    self.startup_loaded = true;
                }
                Event::Backup(r) => match r {
                    Ok(n) => self.say(theme::LOG_OK, format!("Backup completato ({}).", human(n))),
                    Err(e) => self.say(theme::LOG_ERR, format!("Backup non riuscito: {e}")),
                },
                Event::Done => self.busy = false,
            }
        }
    }

    fn spawn(&mut self, ctx: &egui::Context, job: impl FnOnce(&Sender<Event>) + Send + 'static) {
        self.busy = true;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            job(&tx);
            let _ = tx.send(Event::Done);
            ctx.request_repaint();
        });
    }

    fn analyze(&mut self, ctx: &egui::Context) {
        let targets: Vec<&'static Target> = TARGETS.iter().collect();
        self.say(theme::LOG_INFO, "Analizzo…");
        self.spawn(ctx, move |tx| {
            let env = RealEnv;
            let v = targets
                .iter()
                .map(|t| (t.id, engine::scan(t, &env)))
                .collect();
            let _ = tx.send(Event::Sizes(v));
        });
    }

    fn clean_now(&mut self, ctx: &egui::Context) {
        let targets = self.selected();
        self.say(theme::LOG_INFO, "Pulisco…");
        self.spawn(ctx, move |tx| {
            let (env, sys) = (RealEnv, RealSys);
            let (mut freed, mut failed) = (0u64, 0u64);
            for t in targets {
                let out = engine::clean(t, &env, &sys);
                freed = freed.saturating_add(out.stats.bytes);
                failed = failed.saturating_add(out.stats.failed);
                let (c, m) = match &out.error {
                    Some(e) => (theme::LOG_WARN, format!("{}: saltato ({e})", t.label)),
                    None => (
                        theme::LOG_INFO,
                        format!("{}: {}", t.label, human(out.stats.bytes)),
                    ),
                };
                let _ = tx.send(Event::Log(c, m));
            }
            let _ = tx.send(Event::Cleaned { freed, failed });
        });
    }

    fn load_startup(&mut self, ctx: &egui::Context) {
        self.startup_loaded = true;
        self.spawn(ctx, |tx| {
            let _ = tx.send(Event::Startup(startup::list(&RealSys, &RealEnv)));
        });
    }

    fn toggle(&mut self, ctx: &egui::Context, i: usize, enable: bool) {
        let Some(e) = self.startup.get(i).cloned() else {
            return;
        };
        match startup::set_enabled(&RealSys, &e, enable) {
            Ok(()) => {
                if let Some(x) = self.startup.get_mut(i) {
                    x.enabled = enable;
                }
                self.say(
                    theme::LOG_OK,
                    format!(
                        "{}: {}",
                        e.name,
                        if enable { "attivato" } else { "disattivato" }
                    ),
                );
            }
            Err(err) => {
                let hint = if e.needs_admin && !self.is_admin {
                    " (serve l'amministratore)"
                } else {
                    ""
                };
                self.say(theme::LOG_ERR, format!("{}: {err}{hint}", e.name));
                self.load_startup(ctx);
            }
        }
    }

    fn delete_entry(&mut self, ctx: &egui::Context, i: usize) {
        let Some(e) = self.startup.get(i).cloned() else {
            return;
        };
        let dir = app_dir().join("BackupAvvio");
        match startup::delete(&RealSys, &e, &dir) {
            Ok(b) => self.say(
                theme::LOG_OK,
                format!("{} eliminato (copia in {})", e.name, b.display()),
            ),
            Err(err) => self.say(theme::LOG_ERR, format!("{}: {err}", e.name)),
        }
        self.load_startup(ctx);
    }

    fn start_backup(&mut self, ctx: &egui::Context) {
        let plan = Plan {
            drive: self.bk_drive.trim().to_string(),
            image: PathBuf::from(self.bk_image.trim()),
            name: self.bk_name.clone(),
            compression: self.bk_comp,
        };
        if let Err(e) = backup::validate(&plan) {
            self.say(theme::LOG_ERR, e);
            return;
        }
        self.say(theme::LOG_INFO, "Avvio il backup del sistema…");
        self.spawn(ctx, move |tx| {
            let tx2 = tx.clone();
            let r = backup::run(
                &RealSys,
                &plan,
                &app_dir().join("lavoro-backup"),
                &mut |m| {
                    let _ = tx2.send(Event::Log(theme::LOG_INFO, m.to_string()));
                },
            );
            let _ = tx.send(Event::Backup(r));
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
        .min_size(egui::vec2(190.0, 40.0));
        ui.add_enabled(enabled, b).clicked()
    }

    fn relaunch_as_admin(&mut self, ctx: &egui::Context) {
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let cmd = format!(
            "Start-Process -FilePath '{}' -Verb RunAs",
            exe.display().to_string().replace('\'', "''")
        );
        match std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &cmd])
            .spawn()
        {
            Ok(_) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Err(e) => self.say(
                theme::LOG_ERR,
                format!("Impossibile riavviare come amministratore: {e}"),
            ),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        self.pump();
        let ctx = ui.ctx().clone();
        if self.tab == Tab::Startup && !self.startup_loaded && !self.busy {
            self.load_startup(&ctx);
        }
        egui::Panel::top("clean_top").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if self.tab != Tab::Backup {
                    ui.selectable_value(&mut self.tab, Tab::Clean, "Pulizia");
                    ui.selectable_value(&mut self.tab, Tab::Startup, "Avvio automatico");
                }
                if self.busy {
                    ui.spinner();
                }
                if !self.is_admin {
                    if self.tab != Tab::Backup {
                        ui.separator();
                    }
                    ui.label(
                        RichText::new("Non sei amministratore: alcune voci verranno saltate.")
                            .color(theme::WARN),
                    );
                    if ui.button("Riavvia come amministratore").clicked() {
                        self.relaunch_as_admin(&ctx);
                    }
                }
            });
            ui.add_space(4.0);
        });
        egui::Panel::bottom("clean_log").show(ui, |ui| {
            egui::Frame::new()
                .fill(theme::LOG_BG)
                .inner_margin(6.0)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(110.0)
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
        egui::CentralPanel::default().show(ui, |ui| match self.tab {
            Tab::Clean => self.clean_tab(ui, &ctx),
            Tab::Startup => self.startup_tab(ui, &ctx),
            Tab::Backup => self.backup_tab(ui, &ctx),
        });
        if self.confirm_clean {
            let n = self.selected().len();
            egui::Window::new("Pulire adesso?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(&ctx, |ui| {
                    ui.label(format!("Verranno puliti {n} elementi. I file eliminati non vanno nel cestino."));
                    if self.selected().iter().any(|t| t.id == "windows_old") {
                        ui.label(RichText::new("Windows.old: dopo non potrai tornare alla versione precedente di Windows.").color(theme::WARN));
                    }
                    ui.label(RichText::new("Consiglio: chiudi prima i browser e i programmi selezionati.").color(theme::MUTED));
                    ui.horizontal(|ui| {
                        if Self::big_button(ui, "Pulisci", theme::INSTALL, n > 0) {
                            self.confirm_clean = false;
                            self.clean_now(&ctx);
                        }
                        if ui.button("Annulla").clicked() {
                            self.confirm_clean = false;
                        }
                    });
                });
        }
        if let Some(i) = self.confirm_delete {
            let name = self
                .startup
                .get(i)
                .map(|e| e.name.clone())
                .unwrap_or_default();
            egui::Window::new("Eliminare la voce?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(&ctx, |ui| {
                    ui.label(format!("«{name}» non partirà più con Windows. Una copia di sicurezza resta nella cartella BackupAvvio accanto al programma."));
                    ui.label(RichText::new("Per provare soltanto, meglio disattivarla (casella a sinistra).").color(theme::MUTED));
                    ui.horizontal(|ui| {
                        if Self::big_button(ui, "Elimina", theme::INSTALL, true) {
                            self.confirm_delete = None;
                            self.delete_entry(&ctx, i);
                        }
                        if ui.button("Annulla").clicked() {
                            self.confirm_delete = None;
                        }
                    });
                });
        }
        if self.busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
    }

    fn clean_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            if Self::big_button(ui, "1  Analizza", theme::SCAN, !self.busy) {
                self.analyze(ctx);
            }
            if Self::big_button(
                ui,
                "2  Pulisci",
                theme::INSTALL,
                !self.busy && !self.selected().is_empty(),
            ) {
                self.confirm_clean = true;
            }
            if ui.button("Seleziona i consigliati").clicked() {
                for t in TARGETS {
                    self.checked.insert(t.id, t.default_on);
                }
            }
            if ui.button("Deseleziona tutto").clicked() {
                for t in TARGETS {
                    self.checked.insert(t.id, false);
                }
            }
            let total: u64 = self
                .selected()
                .iter()
                .map(|t| self.sizes.get(t.id).map_or(0, |s| s.bytes))
                .sum();
            if !self.sizes.is_empty() {
                ui.label(RichText::new(format!("Selezionato: {}", human(total))).strong());
            }
        });
        ui.add_space(6.0);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for g in Group::ALL {
                    egui::CollapsingHeader::new(RichText::new(g.title()).strong().size(16.0))
                        .default_open(g != Group::Avanzate)
                        .show(ui, |ui| {
                            for t in TARGETS.iter().filter(|t| t.group == g) {
                                ui.horizontal(|ui| {
                                    let mut on = self.checked.get(t.id).copied().unwrap_or(false);
                                    if ui.checkbox(&mut on, t.label).changed() {
                                        self.checked.insert(t.id, on);
                                    }
                                    if t.admin {
                                        ui.label(
                                            RichText::new("amministratore")
                                                .small()
                                                .color(theme::WARN),
                                        );
                                    }
                                    if let Some(s) = self.sizes.get(t.id) {
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                let text = if s.bytes == 0 && s.files == 0 {
                                                    "—".to_string()
                                                } else {
                                                    format!(
                                                        "{}  ({} file)",
                                                        human(s.bytes),
                                                        s.files
                                                    )
                                                };
                                                ui.label(RichText::new(text).color(theme::MUTED));
                                            },
                                        );
                                    }
                                });
                                if !t.hint.is_empty() {
                                    ui.label(
                                        RichText::new(format!("      {}", t.hint))
                                            .small()
                                            .color(theme::MUTED),
                                    );
                                }
                            }
                        });
                }
            });
    }

    fn startup_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!self.busy, egui::Button::new("Aggiorna elenco"))
                .clicked()
            {
                self.load_startup(ctx);
            }
            ui.checkbox(&mut self.show_system, "Mostra anche le voci di Windows");
        });
        ui.label(
            RichText::new("Togli la spunta per impedire a un programma di partire con Windows (si può riattivare in qualsiasi momento). «Elimina» lo rimuove, salvando una copia.")
                .color(theme::MUTED),
        );
        ui.add_space(4.0);
        let mut toggle: Option<(usize, bool)> = None;
        let mut delete: Option<usize> = None;
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("startup_grid")
                    .num_columns(5)
                    .striped(true)
                    .spacing([14.0, 6.0])
                    .show(ui, |ui| {
                        for h in ["Attivo", "Nome", "Comando", "Origine", ""] {
                            ui.label(RichText::new(h).strong());
                        }
                        ui.end_row();
                        for (i, e) in self.startup.iter().enumerate() {
                            if e.system && !self.show_system {
                                continue;
                            }
                            let mut on = e.enabled;
                            if ui.checkbox(&mut on, "").changed() {
                                toggle = Some((i, on));
                            }
                            ui.label(RichText::new(&e.name).strong());
                            ui.label(RichText::new(&e.command).monospace().small());
                            let tag = if e.source == Source::Task && e.system {
                                " (Windows)"
                            } else {
                                ""
                            };
                            ui.label(format!("{}{tag}", e.origin));
                            if ui.small_button("Elimina").clicked() {
                                delete = Some(i);
                            }
                            ui.end_row();
                        }
                    });
                if self.startup.is_empty() && self.startup_loaded && !self.busy {
                    ui.label(
                        RichText::new("Nessuna voce trovata (o non sei su Windows).")
                            .color(theme::MUTED),
                    );
                }
            });
        if let Some((i, on)) = toggle {
            self.toggle(ctx, i, on);
        }
        if let Some(i) = delete {
            self.confirm_delete = Some(i);
        }
    }

    fn backup_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.label(
            RichText::new("Salva tutto il disco di Windows in un file install.wim (come «Backup di sistema» di Dism++). Si usa una copia ombra del disco, quindi si può continuare a lavorare e anche i file in uso vengono copiati.")
                .color(theme::MUTED),
        );
        ui.add_space(8.0);
        egui::Grid::new("backup_grid")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                ui.label("Disco da salvare:");
                ui.add(egui::TextEdit::singleline(&mut self.bk_drive).desired_width(60.0));
                ui.end_row();
                ui.label("File di destinazione:");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.bk_image).desired_width(430.0));
                    if ui.button("Scegli…").clicked() {
                        if let Some(p) = rfd::FileDialog::new()
                            .add_filter("Immagine WIM", &["wim"])
                            .set_file_name("install.wim")
                            .save_file()
                        {
                            self.bk_image = p.to_string_lossy().to_string();
                        }
                    }
                });
                ui.end_row();
                ui.label("Nome dell'immagine:");
                ui.add(egui::TextEdit::singleline(&mut self.bk_name).desired_width(260.0));
                ui.end_row();
                ui.label("Compressione:");
                egui::ComboBox::from_id_salt("bk_comp")
                    .selected_text(self.bk_comp.label())
                    .show_ui(ui, |ui| {
                        for c in [Compression::Max, Compression::Fast, Compression::None] {
                            ui.selectable_value(&mut self.bk_comp, c, c.label());
                        }
                    });
                ui.end_row();
            });
        ui.add_space(8.0);
        ui.label(RichText::new("Consiglio: salva su un altro disco o su un disco esterno, con spazio pari ad almeno la metà dello spazio usato da Windows. Il file non viene mai sovrascritto.").color(theme::MUTED));
        ui.add_space(10.0);
        if Self::big_button(
            ui,
            "Crea il backup",
            theme::DOWNLOAD,
            !self.busy && self.is_admin,
        ) {
            self.start_backup(ctx);
        }
        if !self.is_admin {
            ui.label(RichText::new("Per il backup servono i permessi di amministratore: usa «Riavvia come amministratore» in alto.").color(theme::WARN));
        }
        ui.add_space(8.0);
        ui.label(RichText::new("Ripristino: da un supporto di installazione o da WinPE con  dism /Apply-Image /ImageFile:install.wim /Index:1 /ApplyDir:C:\\  (Dism++ può ripristinare lo stesso file).").small().color(theme::MUTED));
    }
}
