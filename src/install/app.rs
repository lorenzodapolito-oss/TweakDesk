//! Interfaccia di InstallDesk.

use crate::install::catalog::CATALOG;
use crate::install::config::{self, AppConfig};
use crate::install::ext_panel::ExtPanel;
use crate::install::manager::{self, Status};
use crate::install::scan::{self, Item};
use crate::install::setup_panel::SetupPanel;
use crate::install::winget::{Installed, RealWinget};
use crate::theme;
use eframe::egui::{self, Color32, RichText};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

enum Event {
    Log(Color32, String),
    Checked(String, Status),
    Downloaded(String, Result<PathBuf, String>),
    Scanned(Result<Vec<Installed>, String>),
    PcState(String, PcState),
    Done,
}

#[derive(Clone, Debug, PartialEq)]
enum PcState {
    Waiting,
    Downloading,
    Downloaded,
    AlreadyHave,
    Failed(String),
}

#[derive(Clone, Copy, PartialEq)]
pub enum Tab {
    Pc,
    Folder,
    Ext,
    Setup,
}

struct PcRow {
    inst: Installed,
    selected: bool,
    state: Option<PcState>,
}

struct Row {
    item: Item,
    selected: bool,
    status: Option<Status>,
}

pub struct InstallPanel {
    cfg: AppConfig,
    cfg_path: PathBuf,
    app_dir: PathBuf,
    rows: Vec<Row>,
    log: Vec<(Color32, String)>,
    busy: bool,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    catalog_open: bool,
    catalog_filter: String,
    catalog_sel: BTreeSet<usize>,
    tab: Tab,
    pc: Vec<PcRow>,
    pc_filter: String,
    cancel: Arc<AtomicBool>,
    confirm_install: bool,
    bulk: bool,
    ext: ExtPanel,
    setup: SetupPanel,
}

impl InstallPanel {
    pub fn set_tab(&mut self, t: Tab) {
        self.tab = t;
    }

    pub fn new() -> Self {
        let app_dir = config::app_dir();
        let cfg_path = app_dir.join(config::CONFIG_FILE);
        let cfg = config::load(&cfg_path);
        let (tx, rx) = channel();
        let app_dir_copy = app_dir.clone();
        let mut app = Self {
            cfg,
            cfg_path,
            app_dir,
            rows: Vec::new(),
            log: Vec::new(),
            busy: false,
            tx,
            rx,
            catalog_open: false,
            catalog_filter: String::new(),
            catalog_sel: BTreeSet::new(),
            tab: Tab::Pc,
            pc: Vec::new(),
            pc_filter: String::new(),
            cancel: Arc::new(AtomicBool::new(false)),
            confirm_install: false,
            bulk: false,
            ext: ExtPanel::new(&app_dir_copy),
            setup: SetupPanel::new(&app_dir_copy),
        };
        app.refresh();
        let dir = app.root().display().to_string();
        app.say(theme::LOG_INFO, format!("Cartella degli installer: {dir}"));
        app
    }

    fn root(&self) -> PathBuf {
        self.cfg.effective_dir(&self.app_dir)
    }

    fn say(&mut self, color: Color32, msg: String) {
        let t = chrono::Local::now().format("%H:%M:%S");
        self.log.push((color, format!("[{t}] {msg}")));
    }

    fn save_cfg(&mut self) {
        if let Err(e) = config::save(&self.cfg_path, &self.cfg) {
            self.say(
                theme::LOG_ERR,
                format!("Impossibile salvare le impostazioni: {e}"),
            );
        }
    }

    fn refresh(&mut self) {
        let old: Vec<(String, bool, Option<Status>)> = self
            .rows
            .drain(..)
            .map(|r| (r.item.program, r.selected, r.status))
            .collect();
        let root = self.root();
        self.rows = scan::scan(&root, &self.cfg)
            .into_iter()
            .map(|item| {
                let prev = old.iter().find(|(p, _, _)| *p == item.program);
                Row {
                    selected: prev.is_none_or(|p| p.1),
                    status: prev.and_then(|p| p.2.clone()),
                    item,
                }
            })
            .collect();
    }

    fn choose_folder(&mut self) {
        if let Some(dir) = rfd::FileDialog::new()
            .set_title("Cartella degli installer")
            .pick_folder()
        {
            self.cfg.download_dir = dir.display().to_string();
            self.save_cfg();
            self.refresh();
            self.say(
                theme::LOG_INFO,
                format!("Nuova cartella: {}", dir.display()),
            );
        }
    }

    fn open_folder(&mut self) {
        let root = self.root();
        if let Err(e) = std::fs::create_dir_all(&root) {
            self.say(theme::LOG_ERR, format!("{}: {e}", root.display()));
            return;
        }
        #[cfg(windows)]
        {
            let _ = std::process::Command::new("explorer").arg(&root).spawn();
        }
        #[cfg(not(windows))]
        {
            let _ = std::process::Command::new("xdg-open").arg(&root).spawn();
        }
    }

    fn start_check(&mut self) {
        let jobs: Vec<Item> = self
            .rows
            .iter()
            .filter(|r| r.item.winget_id.is_some())
            .map(|r| r.item.clone())
            .collect();
        if jobs.is_empty() {
            self.say(
                theme::LOG_WARN,
                "Nessun programma riconosciuto da controllare.".into(),
            );
            return;
        }
        self.busy = true;
        self.say(
            theme::LOG_INFO,
            format!("Controllo versioni di {} programmi...", jobs.len()),
        );
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let w = RealWinget;
            for it in jobs {
                let st = manager::check(&w, &it);
                let _ = tx.send(Event::Checked(it.program.clone(), st));
            }
            let _ = tx.send(Event::Done);
        });
    }

    /// `(programma, id winget)` da scaricare.
    fn start_download(&mut self, jobs: Vec<(String, String)>) {
        if jobs.is_empty() {
            self.say(theme::LOG_WARN, "Niente da scaricare.".into());
            return;
        }
        self.busy = true;
        let root = self.root();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let w = RealWinget;
            for (program, id) in jobs {
                let _ = tx.send(Event::Log(theme::LOG_INFO, format!("Scarico {program}...")));
                let res = manager::download_update(&w, &root, &program, &id);
                let _ = tx.send(Event::Downloaded(program, res));
            }
            let _ = tx.send(Event::Done);
        });
    }

    fn download_selected(&mut self) {
        let jobs = self
            .rows
            .iter()
            .filter(|r| r.selected && matches!(r.status, Some(Status::Outdated { .. })))
            .filter_map(|r| {
                r.item
                    .winget_id
                    .clone()
                    .map(|id| (r.item.program.clone(), id))
            })
            .collect();
        self.start_download(jobs);
    }

    fn download_from_catalog(&mut self) {
        let jobs = self
            .catalog_sel
            .iter()
            .filter_map(|&i| CATALOG.get(i))
            .map(|e| (e.name.to_string(), e.winget_id.to_string()))
            .collect();
        self.catalog_sel.clear();
        self.start_download(jobs);
    }

    fn make_scripts(&mut self) {
        let items: Vec<Item> = self
            .rows
            .iter()
            .filter(|r| r.selected)
            .map(|r| r.item.clone())
            .collect();
        if items.is_empty() {
            self.say(theme::LOG_WARN, "Nessun programma selezionato.".into());
            return;
        }
        let root = self.root();
        match manager::write_scripts(&root, &items) {
            Ok(n) => self.say(
                theme::LOG_OK,
                format!(
                    "Creati {n} script singoli e Installa-tutto.cmd in {}",
                    root.display()
                ),
            ),
            Err(e) => self.say(theme::LOG_ERR, format!("Errore: {e}")),
        }
    }

    /// Passo 1: elenca i programmi installati nel PC che winget sa scaricare.
    fn start_scan(&mut self) {
        self.busy = true;
        self.say(
            theme::LOG_INFO,
            "Scansione dei programmi installati nel PC...".into(),
        );
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Event::Scanned(
                crate::install::winget::Winget::list_installed(&RealWinget),
            ));
            let _ = tx.send(Event::Done);
        });
    }

    /// Passo 2: scarica i programmi del PC selezionati (salta quelli già presenti aggiornati).
    fn download_pc(&mut self) {
        let jobs: Vec<Installed> = self
            .pc
            .iter()
            .filter(|r| r.selected)
            .map(|r| r.inst.clone())
            .collect();
        if jobs.is_empty() {
            self.say(
                theme::LOG_WARN,
                "Prima scansiona il PC e seleziona qualcosa.".into(),
            );
            return;
        }
        self.busy = true;
        self.bulk = true;
        self.cancel.store(false, Ordering::SeqCst);
        for r in self.pc.iter_mut().filter(|r| r.selected) {
            r.state = Some(PcState::Waiting);
        }
        let (root, tx, cancel) = (self.root(), self.tx.clone(), self.cancel.clone());
        self.say(
            theme::LOG_INFO,
            format!("Scarico {} programmi in {}", jobs.len(), root.display()),
        );
        std::thread::spawn(move || {
            let w = RealWinget;
            for inst in jobs {
                if cancel.load(Ordering::SeqCst) {
                    let _ = tx.send(Event::Log(theme::LOG_WARN, "Download interrotto.".into()));
                    break;
                }
                let target = inst
                    .available
                    .clone()
                    .unwrap_or_else(|| inst.version.clone());
                if manager::is_current(&root, &inst.name, &target) {
                    let _ = tx.send(Event::PcState(inst.id.clone(), PcState::AlreadyHave));
                    continue;
                }
                let _ = tx.send(Event::PcState(inst.id.clone(), PcState::Downloading));
                let st = match manager::download_update(&w, &root, &inst.name, &inst.id) {
                    Ok(_) => PcState::Downloaded,
                    Err(e) => PcState::Failed(e),
                };
                let _ = tx.send(Event::PcState(inst.id.clone(), st));
            }
            let _ = tx.send(Event::Done);
        });
    }

    /// Passo 3: scrive ed esegue lo script che installa in silenzio i programmi selezionati.
    fn install_selected(&mut self) {
        let items: Vec<Item> = self
            .rows
            .iter()
            .filter(|r| r.selected)
            .map(|r| r.item.clone())
            .collect();
        if items.is_empty() {
            self.say(theme::LOG_WARN, "Nessun installer selezionato.".into());
            return;
        }
        let root = self.root();
        match manager::write_run_script(&root, &items) {
            Err(e) => self.say(theme::LOG_ERR, format!("Errore: {e}")),
            Ok(path) => {
                #[cfg(windows)]
                {
                    let r = std::process::Command::new("cmd")
                        .arg("/c")
                        .arg("start")
                        .arg("")
                        .arg(&path)
                        .spawn();
                    match r {
                        Ok(_) => self.say(theme::LOG_OK, format!("Installazione di {} programmi avviata (conferma l'avviso di amministratore).", items.len())),
                        Err(e) => self.say(theme::LOG_ERR, format!("{}: {e}", path.display())),
                    }
                }
                #[cfg(not(windows))]
                self.say(
                    theme::LOG_WARN,
                    format!(
                        "Script creato: {} (l'installazione è solo per Windows).",
                        path.display()
                    ),
                );
            }
        }
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

    fn pc_table(&mut self, ui: &mut egui::Ui) {
        if self.pc.is_empty() {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Nessuna scansione ancora").size(18.0).strong());
                ui.label(RichText::new("Premi «1 Scansiona il PC»: InstallDesk elenca tutti i programmi installati\nche winget sa scaricare. Poi «2 Scarica» li salva nella cartella predefinita.").color(theme::MUTED));
            });
            return;
        }
        ui.horizontal(|ui| {
            ui.label("Filtra:");
            ui.text_edit_singleline(&mut self.pc_filter);
            if ui.button("Seleziona tutti").clicked() {
                self.pc.iter_mut().for_each(|r| r.selected = true);
            }
            if ui.button("Nessuno").clicked() {
                self.pc.iter_mut().for_each(|r| r.selected = false);
            }
            if ui.button("Solo con aggiornamento").clicked() {
                self.pc
                    .iter_mut()
                    .for_each(|r| r.selected = r.inst.available.is_some());
            }
            let n = self.pc.iter().filter(|r| r.selected).count();
            ui.label(
                RichText::new(format!("{n} di {} selezionati", self.pc.len())).color(theme::MUTED),
            );
        });
        let f = self.pc_filter.to_lowercase();
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("pcrows")
                    .striped(true)
                    .num_columns(5)
                    .spacing([14.0, 8.0])
                    .show(ui, |ui| {
                        for h in ["", "Programma", "Installata", "Disponibile", "Stato"] {
                            ui.label(RichText::new(h).strong().color(theme::MUTED));
                        }
                        ui.end_row();
                        for r in self
                            .pc
                            .iter_mut()
                            .filter(|r| f.is_empty() || r.inst.name.to_lowercase().contains(&f))
                        {
                            ui.checkbox(&mut r.selected, "");
                            ui.label(RichText::new(&r.inst.name).strong());
                            ui.label(&r.inst.version);
                            match &r.inst.available {
                                Some(a) => ui.label(RichText::new(a).color(theme::WARN).strong()),
                                None => ui.label(RichText::new("—").color(theme::MUTED)),
                            };
                            let (txt, col) = match &r.state {
                                None => ("", theme::MUTED),
                                Some(PcState::Waiting) => ("In coda", theme::MUTED),
                                Some(PcState::Downloading) => ("Scarico...", theme::SCAN),
                                Some(PcState::Downloaded) => ("Scaricato", theme::OK),
                                Some(PcState::AlreadyHave) => ("Già presente", theme::OK),
                                Some(PcState::Failed(_)) => ("Errore", theme::ERR),
                            };
                            let resp = ui.label(RichText::new(txt).color(col));
                            if let Some(PcState::Failed(e)) = &r.state {
                                resp.on_hover_text(e);
                            }
                            ui.end_row();
                        }
                    });
            });
    }

    /// Dopo il download di massa: aggiorna l'elenco e crea gli script silenziosi.
    fn finish_bulk(&mut self) {
        self.refresh();
        let names: Vec<String> = self
            .pc
            .iter()
            .filter(|r| matches!(r.state, Some(PcState::Downloaded | PcState::AlreadyHave)))
            .map(|r| r.inst.name.clone())
            .collect();
        let items: Vec<Item> = manager::items_for_names(
            &self.rows.iter().map(|r| r.item.clone()).collect::<Vec<_>>(),
            &names,
        );
        let failed = self
            .pc
            .iter()
            .filter(|r| matches!(r.state, Some(PcState::Failed(_))))
            .count();
        if !items.is_empty() {
            let root = self.root();
            match manager::write_scripts(&root, &items) {
                Ok(n) => self.say(
                    theme::LOG_OK,
                    format!("Pronti {n} script di installazione silenziosa."),
                ),
                Err(e) => self.say(theme::LOG_ERR, format!("Script non creati: {e}")),
            }
        }
        if failed > 0 {
            self.say(
                theme::LOG_WARN,
                format!("{failed} programmi non scaricati (vedi sopra)."),
            );
        }
    }

    fn pump(&mut self) {
        let mut refresh = false;
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                Event::Log(c, m) => self.say(c, m),
                Event::Checked(program, st) => {
                    let (c, msg) = match &st {
                        Status::UpToDate => (theme::LOG_OK, format!("{program}: aggiornato")),
                        Status::Outdated { latest } => {
                            (theme::LOG_WARN, format!("{program}: disponibile {latest}"))
                        }
                        Status::Unknown(e) => (theme::LOG_ERR, format!("{program}: {e}")),
                    };
                    self.say(c, msg);
                    if let Some(r) = self.rows.iter_mut().find(|r| r.item.program == program) {
                        r.status = Some(st);
                    }
                }
                Event::Downloaded(program, res) => match res {
                    Ok(p) => {
                        self.say(
                            theme::LOG_OK,
                            format!("{program}: scaricato {}", p.display()),
                        );
                        refresh = true;
                    }
                    Err(e) => self.say(theme::LOG_ERR, format!("{program}: {e}")),
                },
                Event::Scanned(res) => match res {
                    Ok(list) => {
                        self.say(
                            theme::LOG_OK,
                            format!("Trovati {} programmi scaricabili.", list.len()),
                        );
                        self.pc = list
                            .into_iter()
                            .map(|inst| PcRow {
                                inst,
                                selected: true,
                                state: None,
                            })
                            .collect();
                        self.pc.sort_by_key(|r| r.inst.name.to_lowercase());
                    }
                    Err(e) => self.say(theme::LOG_ERR, format!("Scansione non riuscita: {e}")),
                },
                Event::PcState(id, st) => {
                    if let PcState::Failed(e) = &st {
                        let msg = format!("{id}: {e}");
                        self.say(theme::LOG_ERR, msg);
                    }
                    if let Some(r) = self.pc.iter_mut().find(|r| r.inst.id == id) {
                        r.state = Some(st);
                    }
                }
                Event::Done => {
                    self.busy = false;
                    if std::mem::take(&mut self.bulk) {
                        self.finish_bulk();
                    }
                    refresh = true;
                    self.say(theme::LOG_INFO, "Operazione terminata.".into());
                }
            }
        }
        if refresh {
            self.refresh();
        }
    }

    /// Sposta nel registro i messaggi della scheda estensioni.
    fn drain_ext(&mut self) {
        let msgs = std::mem::take(&mut self.ext.msgs)
            .into_iter()
            .chain(std::mem::take(&mut self.setup.msgs));
        for (c, m) in msgs.collect::<Vec<_>>() {
            self.say(c, m);
        }
    }

    fn status_text(st: &Option<Status>) -> RichText {
        match st {
            None => RichText::new("—").color(theme::MUTED),
            Some(Status::UpToDate) => RichText::new("Aggiornato").color(theme::OK),
            Some(Status::Outdated { latest }) => RichText::new(format!("Nuova: {latest}"))
                .color(theme::WARN)
                .strong(),
            Some(Status::Unknown(_)) => RichText::new("Non verificabile").color(theme::ERR),
        }
    }

    fn table(&mut self, ui: &mut egui::Ui) {
        if self.rows.is_empty() {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("La cartella è vuota").size(18.0).strong());
                ui.label(
                    RichText::new("Usa «Aggiungi programmi» per scaricarne dal catalogo,\noppure copia qui i tuoi installer (anche in sottocartelle).")
                        .color(theme::MUTED),
                );
            });
            return;
        }
        let mut changed_args: Vec<(String, String)> = Vec::new();
        egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("rows").striped(true).num_columns(7).spacing([14.0, 8.0]).show(ui, |ui| {
                for h in ["", "Programma", "Versione", "Stato", "Tipo", "Opzioni silenziose", "File"] {
                    ui.label(RichText::new(h).strong().color(theme::MUTED));
                }
                ui.end_row();
                for r in &mut self.rows {
                    ui.checkbox(&mut r.selected, "");
                    ui.label(RichText::new(&r.item.program).strong());
                    ui.label(r.item.version.as_deref().unwrap_or("?"));
                    let resp = ui.label(Self::status_text(&r.status));
                    if let Some(Status::Unknown(e)) = &r.status {
                        resp.on_hover_text(e);
                    }
                    let kind = r.item.kind;
                    let k = ui.label(if kind.is_reliable() {
                        RichText::new(kind.label())
                    } else {
                        RichText::new(kind.label()).color(theme::WARN)
                    });
                    if !kind.is_reliable() {
                        k.on_hover_text("Tipo non riconosciuto: le opzioni sono un tentativo, verifica a mano.");
                    }
                    let before = r.item.args.clone();
                    ui.add_sized([300.0, 24.0], egui::TextEdit::singleline(&mut r.item.args));
                    if r.item.args != before {
                        changed_args.push((r.item.program.clone(), r.item.args.clone()));
                    }
                    ui.label(RichText::new(&r.item.rel_path).color(theme::MUTED));
                    ui.end_row();
                }
            });
        });
        if !changed_args.is_empty() {
            for (program, args) in changed_args {
                self.cfg.silent_overrides.insert(program, args);
            }
            self.save_cfg();
        }
    }

    fn catalog_window(&mut self, ctx: &egui::Context) {
        let mut open = self.catalog_open;
        egui::Window::new("Aggiungi programmi dal catalogo")
            .open(&mut open)
            .default_size([420.0, 520.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Filtra:");
                    ui.text_edit_singleline(&mut self.catalog_filter);
                });
                let f = self.catalog_filter.to_lowercase();
                egui::ScrollArea::vertical()
                    .max_height(380.0)
                    .show(ui, |ui| {
                        for (i, e) in CATALOG.iter().enumerate() {
                            if !f.is_empty() && !e.name.to_lowercase().contains(&f) {
                                continue;
                            }
                            let mut on = self.catalog_sel.contains(&i);
                            if ui
                                .checkbox(&mut on, format!("{}  ({})", e.name, e.winget_id))
                                .changed()
                            {
                                if on {
                                    self.catalog_sel.insert(i);
                                } else {
                                    self.catalog_sel.remove(&i);
                                }
                            }
                        }
                    });
                ui.separator();
                let n = self.catalog_sel.len();
                if ui
                    .add_enabled(
                        !self.busy && n > 0,
                        egui::Button::new(format!("Scarica {n} programmi")),
                    )
                    .clicked()
                {
                    self.download_from_catalog();
                }
            });
        self.catalog_open = open;
    }
}

impl InstallPanel {
    pub fn show(&mut self, ui: &mut egui::Ui) {
        self.pump();
        self.drain_ext();
        let ctx = ui.ctx().clone();
        let programs = matches!(self.tab, Tab::Pc | Tab::Folder);
        if programs {
            egui::Panel::top("top").show(ui, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("Cartella predefinita:");
                    ui.label(RichText::new(self.root().display().to_string()).strong());
                    if ui.button("Cambia…").clicked() {
                        self.choose_folder();
                    }
                    if ui.button("Apri").clicked() {
                        self.open_folder();
                    }
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let idle = !self.busy;
                    if Self::big_button(ui, "1  Scansiona il PC", theme::SCAN, idle) {
                        self.tab = Tab::Pc;
                        self.start_scan();
                    }
                    if Self::big_button(
                        ui,
                        "2  Scarica",
                        theme::DOWNLOAD,
                        idle && !self.pc.is_empty(),
                    ) {
                        self.download_pc();
                    }
                    if Self::big_button(
                        ui,
                        "3  Installa",
                        theme::INSTALL,
                        idle && !self.rows.is_empty(),
                    ) {
                        self.confirm_install = true;
                    }
                    if self.busy {
                        ui.spinner();
                        if ui.button("Interrompi").clicked() {
                            self.cancel.store(true, Ordering::SeqCst);
                        }
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.selectable_value(&mut self.tab, Tab::Pc, "Programmi del PC");
                    ui.selectable_value(&mut self.tab, Tab::Folder, "Installer scaricati");
                    ui.separator();
                    let idle = !self.busy;
                    if ui
                        .add_enabled(idle, egui::Button::new("Controlla aggiornamenti"))
                        .clicked()
                    {
                        self.start_check();
                    }
                    if ui
                        .add_enabled(idle, egui::Button::new("Scarica aggiornamenti"))
                        .clicked()
                    {
                        self.download_selected();
                    }
                    if ui.button("Aggiungi programmi…").clicked() {
                        self.catalog_open = true;
                    }
                    if ui
                        .add_enabled(idle, egui::Button::new("Crea script silenziosi"))
                        .clicked()
                    {
                        self.make_scripts();
                    }
                    if ui
                        .add_enabled(idle, egui::Button::new("Aggiorna elenco"))
                        .clicked()
                    {
                        self.refresh();
                    }
                });
                ui.add_space(4.0);
            });
        }
        egui::Panel::bottom("log").show(ui, |ui| {
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
            Tab::Pc => self.pc_table(ui),
            Tab::Folder => self.table(ui),
            Tab::Ext => self.ext.ui(ui),
            Tab::Setup => self.setup.ui(ui),
        });
        if self.confirm_install {
            let n = self.rows.iter().filter(|r| r.selected).count();
            egui::Window::new("Installare i programmi?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(&ctx, |ui| {
                    ui.label(format!("Verranno installati in silenzio {n} programmi dalla cartella degli installer (scheda «Installer scaricati»). Windows chiederà l'autorizzazione di amministratore."));
                    ui.horizontal(|ui| {
                        if Self::big_button(ui, "Installa", theme::INSTALL, n > 0) {
                            self.confirm_install = false;
                            self.install_selected();
                        }
                        if ui.button("Annulla").clicked() {
                            self.confirm_install = false;
                        }
                    });
                });
        }
        if self.catalog_open {
            self.catalog_window(&ctx);
        }
        if self.busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
    }
}
