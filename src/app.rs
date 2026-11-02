//! Interfaccia di TweakDesk.

use crate::catalog;
use crate::engine::{self, Current, Report, Status};
use crate::model::{Entry, Profile, Risk};
use crate::regfile;
use crate::registry::{RegExe, Registry};
use crate::theme;
use eframe::egui::{self, Color32, RichText};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};

pub const APP_TITLE: &str = "TweakDesk";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

enum Event {
    Extracted(Result<Vec<Entry>, String>),
    Status(u64, Result<Current, String>),
    Applied(Result<Report, String>),
    Undone(Result<Report, String>),
}

struct Row {
    entry: Entry,
    selected: bool,
    status: Option<Status>,
}

pub struct App {
    rows: Vec<Row>,
    source: String,
    log: Vec<(Color32, String)>,
    busy: bool,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    is_admin: bool,
    app_dir: PathBuf,
    undo_open: bool,
    undo_list: Vec<PathBuf>,
    generation: u64,
}

fn app_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn value_text(v: &Option<crate::model::Value>) -> String {
    v.as_ref()
        .map_or_else(|| "assente".to_string(), crate::model::Value::show)
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, open: Option<PathBuf>) -> Self {
        theme::apply(&cc.egui_ctx);
        let (tx, rx) = channel();
        let mut app = Self {
            rows: Vec::new(),
            source: String::new(),
            log: Vec::new(),
            busy: false,
            tx,
            rx,
            is_admin: RegExe.is_admin(),
            app_dir: app_dir(),
            undo_open: false,
            undo_list: Vec::new(),
            generation: 0,
        };
        let who = if app.is_admin {
            "amministratore"
        } else {
            "utente normale"
        };
        app.say(theme::LOG_INFO, format!("Avviato come {who}."));
        if let Some(p) = open {
            app.load_path(&p);
        }
        app
    }

    fn backup_root(&self) -> PathBuf {
        self.app_dir.join("Backup")
    }

    fn say(&mut self, color: Color32, msg: String) {
        self.log.push((
            color,
            format!("[{}] {msg}", chrono::Local::now().format("%H:%M:%S")),
        ));
    }

    fn job(&mut self, f: impl FnOnce() -> Event + Send + 'static) {
        self.busy = true;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
    }

    fn set_entries(&mut self, entries: Vec<Entry>, source: String) {
        self.rows = entries
            .into_iter()
            .map(|entry| Row {
                selected: entry.risk != Risk::Security,
                entry,
                status: None,
            })
            .collect();
        self.source = source;
    }

    fn start_extract(&mut self) {
        self.say(
            theme::LOG_INFO,
            "Leggo le impostazioni di questo PC...".into(),
        );
        self.job(|| Event::Extracted(engine::extract(&RegExe, &catalog::catalog())));
    }

    fn start_status(&mut self) {
        self.generation += 1;
        let gen = self.generation;
        let entries: Vec<Entry> = self.rows.iter().map(|r| r.entry.clone()).collect();
        if entries.is_empty() {
            return;
        }
        self.job(move || {
            let refs: Vec<&Entry> = entries.iter().collect();
            Event::Status(gen, engine::read_current(&RegExe, &refs))
        });
    }

    fn load_path(&mut self, path: &Path) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let is_reg = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("reg"));
        if is_reg {
            match std::fs::read(path) {
                Ok(bytes) => {
                    let (entries, skipped) = engine::from_reg(&regfile::decode(&bytes));
                    let n = entries.len();
                    self.set_entries(entries, format!("File .reg: {name}"));
                    self.say(theme::LOG_OK, format!("{n} voci lette da {name}."));
                    if skipped > 0 {
                        self.say(
                            theme::LOG_WARN,
                            format!("{skipped} voci ignorate (rami diversi da HKCU/HKLM)."),
                        );
                    }
                }
                Err(e) => self.say(theme::LOG_ERR, format!("{}: {e}", path.display())),
            }
        } else {
            match Profile::load(path) {
                Ok(p) => {
                    let n = p.entries.len();
                    self.set_entries(
                        p.entries,
                        format!("Profilo di {} ({})", p.source, p.created),
                    );
                    self.say(theme::LOG_OK, format!("Profilo caricato: {n} voci."));
                }
                Err(e) => {
                    self.say(theme::LOG_ERR, e);
                    return;
                }
            }
        }
        self.start_status();
    }

    fn open_profile(&mut self) {
        if let Some(p) = rfd::FileDialog::new()
            .set_title("Apri profilo o file .reg")
            .add_filter("Profilo TweakDesk o .reg", &["json", "reg"])
            .pick_file()
        {
            self.load_path(&p);
        }
    }

    fn selected(&self) -> Vec<&Entry> {
        self.rows
            .iter()
            .filter(|r| r.selected)
            .map(|r| &r.entry)
            .collect()
    }

    fn save_profile(&mut self) {
        let sel: Vec<Entry> = self.selected().into_iter().cloned().collect();
        if sel.is_empty() {
            self.say(
                theme::LOG_WARN,
                "Nessuna voce selezionata da salvare.".into(),
            );
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .set_title("Salva profilo")
            .set_file_name("le-mie-impostazioni.tweakdesk.json")
            .add_filter("Profilo TweakDesk", &["json"])
            .save_file()
        else {
            return;
        };
        let pc = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "questo PC".into());
        let profile = Profile::new(
            sel,
            &pc,
            &chrono::Local::now().format("%d/%m/%Y %H:%M").to_string(),
        );
        match profile.save(&path) {
            Ok(()) => self.say(
                theme::LOG_OK,
                format!(
                    "Profilo salvato: {} ({} voci).",
                    path.display(),
                    profile.entries.len()
                ),
            ),
            Err(e) => self.say(theme::LOG_ERR, e),
        }
    }

    fn export_reg(&mut self) {
        let sel = self.selected();
        if sel.is_empty() {
            self.say(
                theme::LOG_WARN,
                "Nessuna voce selezionata da esportare.".into(),
            );
            return;
        }
        let text = regfile::export(&sel);
        let n = sel.len();
        let Some(path) = rfd::FileDialog::new()
            .set_title("Esporta file .reg")
            .set_file_name("le-mie-impostazioni.reg")
            .add_filter("File di registro", &["reg"])
            .save_file()
        else {
            return;
        };
        match std::fs::write(&path, regfile::encode(&text)) {
            Ok(()) => self.say(
                theme::LOG_OK,
                format!("Esportate {n} voci in {}.", path.display()),
            ),
            Err(e) => self.say(theme::LOG_ERR, format!("{}: {e}", path.display())),
        }
    }

    fn apply_selected(&mut self) {
        let sel: Vec<Entry> = self.selected().into_iter().cloned().collect();
        if sel.is_empty() {
            self.say(theme::LOG_WARN, "Nessuna voce selezionata.".into());
            return;
        }
        let risky = sel.iter().filter(|e| e.risk == Risk::Security).count();
        if risky > 0 {
            let answer = rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Warning)
                .set_title("Voci che riducono la sicurezza")
                .set_description(format!(
                    "Tra le voci selezionate ce ne sono {risky} che riducono la sicurezza del PC \
                     (aggiornamenti, SmartScreen, mitigazioni della CPU...).\n\nApplicarle comunque?"
                ))
                .set_buttons(rfd::MessageButtons::YesNo)
                .show();
            if answer != rfd::MessageDialogResult::Yes {
                return;
            }
        }
        let root = self.backup_root();
        let stamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
        self.say(theme::LOG_INFO, format!("Applico {} voci...", sel.len()));
        self.job(move || {
            let refs: Vec<&Entry> = sel.iter().collect();
            Event::Applied(engine::apply(&RegExe, &refs, &root, &stamp))
        });
    }

    fn start_undo(&mut self, dir: PathBuf) {
        self.undo_open = false;
        self.say(
            theme::LOG_INFO,
            format!("Ripristino da {}...", dir.display()),
        );
        self.job(move || Event::Undone(engine::undo(&RegExe, &dir.join("annulla.json"))));
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

    fn report(&mut self, what: &str, r: Result<Report, String>) {
        match r {
            Err(e) => self.say(theme::LOG_ERR, format!("{what}: {e}")),
            Ok(rep) => {
                let c = if rep.failed.is_empty() {
                    theme::LOG_OK
                } else {
                    theme::LOG_WARN
                };
                self.say(
                    c,
                    format!(
                        "{what}: {} fatte, {} già a posto, {} non riuscite.",
                        rep.done,
                        rep.already,
                        rep.failed.len()
                    ),
                );
                for (label, why) in rep.failed {
                    self.say(theme::LOG_ERR, format!("  {label}: {why}"));
                }
                if let Some(d) = rep.backup_dir {
                    self.say(
                        theme::LOG_INFO,
                        format!(
                            "Copia di sicurezza in {} (si annulla da «Annulla…»).",
                            d.display()
                        ),
                    );
                }
            }
        }
    }

    fn pump(&mut self) {
        let mut refresh = false;
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                Event::Extracted(Ok(entries)) => {
                    let n = entries.len();
                    self.set_entries(entries, "Estratte da questo PC".into());
                    for r in &mut self.rows {
                        r.status = Some(Status::Applied);
                    }
                    self.say(
                        theme::LOG_OK,
                        format!("Trovate {n} impostazioni diverse da quelle di fabbrica."),
                    );
                    self.busy = false;
                }
                Event::Extracted(Err(e)) => {
                    self.say(theme::LOG_ERR, e);
                    self.busy = false;
                }
                Event::Status(gen, res) => {
                    if gen == self.generation {
                        match res {
                            Ok(cur) => {
                                for r in &mut self.rows {
                                    r.status = Some(engine::status(&r.entry, &cur));
                                }
                            }
                            Err(e) => {
                                self.say(theme::LOG_ERR, format!("Lettura del registro: {e}"))
                            }
                        }
                    }
                    self.busy = false;
                }
                Event::Applied(r) => {
                    self.report("Applicazione", r);
                    self.busy = false;
                    refresh = true;
                }
                Event::Undone(r) => {
                    self.report("Annullamento", r);
                    self.busy = false;
                    refresh = true;
                }
            }
        }
        if refresh {
            self.start_status();
        }
    }

    fn status_text(st: &Option<Status>) -> RichText {
        match st {
            None => RichText::new("—").color(theme::MUTED),
            Some(Status::Applied) => RichText::new("Già applicata").color(theme::OK),
            Some(Status::ToApply(now)) => {
                RichText::new(format!("Da applicare (ora: {})", value_text(now))).color(theme::WARN)
            }
        }
    }

    fn table(&mut self, ui: &mut egui::Ui) {
        if self.rows.is_empty() {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Nessuna impostazione caricata").size(20.0).strong());
                ui.label(
                    RichText::new(
                        "«Estrai da questo PC» legge le tue impostazioni diverse da Windows di fabbrica.\n\
                         «Apri profilo…» carica un profilo salvato (o un file .reg) per applicarlo qui.",
                    )
                    .color(theme::MUTED),
                );
            });
            return;
        }
        let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
        for (i, r) in self.rows.iter().enumerate() {
            match groups.iter_mut().find(|(g, _)| *g == r.entry.group) {
                Some((_, v)) => v.push(i),
                None => groups.push((r.entry.group.clone(), vec![i])),
            }
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (gi, (name, idx)) in groups.iter().enumerate() {
                    let on = idx.iter().filter(|&&i| self.rows[i].selected).count();
                    egui::CollapsingHeader::new(
                        RichText::new(format!("{name}  ({on}/{})", idx.len()))
                            .strong()
                            .size(15.0),
                    )
                    .id_salt(format!("grp{gi}"))
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui.small_button("Tutte").clicked() {
                                idx.iter().for_each(|&i| self.rows[i].selected = true);
                            }
                            if ui.small_button("Nessuna").clicked() {
                                idx.iter().for_each(|&i| self.rows[i].selected = false);
                            }
                        });
                        egui::Grid::new(format!("grid{gi}"))
                            .striped(true)
                            .spacing([14.0, 6.0])
                            .show(ui, |ui| {
                                for &i in idx {
                                    let r = &mut self.rows[i];
                                    ui.checkbox(&mut r.selected, "");
                                    let cell = |ui: &mut egui::Ui, w: f32, add: &mut dyn FnMut(&mut egui::Ui) -> egui::Response| {
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(w, 22.0),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            ui.set_min_width(w);
                                            add(ui)
                                        },
                                    )
                                    .inner
                                };
                                let hover = format!(
                                    "{}\\{} = {}",
                                    r.entry.key,
                                    if r.entry.name.is_empty() { "(predefinito)" } else { &r.entry.name },
                                    r.entry.value.show()
                                );
                                cell(ui, 440.0, &mut |ui| ui.label(&r.entry.label)).on_hover_text(hover);
                                let color = if r.entry.risk == Risk::Security { theme::ERR } else { theme::WARN };
                                let tag = r.entry.risk.label();
                                cell(ui, 130.0, &mut |ui| ui.label(RichText::new(tag).color(color).small()));
                                ui.label(Self::status_text(&r.status));
                                    ui.end_row();
                                }
                            });
                    });
                }
            });
    }

    fn undo_window(&mut self, ctx: &egui::Context) {
        let mut open = self.undo_open;
        let mut chosen: Option<PathBuf> = None;
        egui::Window::new("Annulla un'applicazione")
            .open(&mut open)
            .default_size([460.0, 320.0])
            .show(ctx, |ui| {
                if self.undo_list.is_empty() {
                    ui.label("Nessuna copia di sicurezza trovata in questa cartella.");
                }
                ui.label(
                    RichText::new(
                        "Ogni applicazione salva i valori precedenti. Scegli quale annullare:",
                    )
                    .color(theme::MUTED),
                );
                egui::ScrollArea::vertical()
                    .max_height(240.0)
                    .show(ui, |ui| {
                        for d in self.undo_list.iter().rev() {
                            let n = d
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_default();
                            let items = engine::load_undo(&d.join("annulla.json"))
                                .map(|u| u.items.len())
                                .unwrap_or(0);
                            ui.horizontal(|ui| {
                                ui.label(format!("{n}  ·  {items} voci"));
                                if ui
                                    .add_enabled(!self.busy, egui::Button::new("Ripristina"))
                                    .clicked()
                                {
                                    chosen = Some(d.clone());
                                }
                            });
                        }
                    });
            });
        self.undo_open = open;
        if let Some(d) = chosen {
            self.start_undo(d);
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.pump();
        let ctx = ui.ctx().clone();
        let has_rows = !self.rows.is_empty();
        let needs_admin = self
            .rows
            .iter()
            .any(|r| r.selected && r.entry.needs_admin());
        egui::Panel::top("top").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(APP_TITLE)
                        .size(22.0)
                        .strong()
                        .color(theme::PRIMARY),
                );
                ui.label(RichText::new(format!("v{APP_VERSION}")).color(theme::MUTED));
                if !self.source.is_empty() {
                    ui.label(RichText::new(format!("·  {}", self.source)).color(theme::MUTED));
                }
            });
            ui.horizontal_wrapped(|ui| {
                let idle = !self.busy;
                if ui
                    .add_enabled(idle, egui::Button::new("Estrai da questo PC"))
                    .clicked()
                {
                    self.start_extract();
                }
                if ui
                    .add_enabled(idle, egui::Button::new("Apri profilo o .reg…"))
                    .clicked()
                {
                    self.open_profile();
                }
                ui.separator();
                if ui
                    .add_enabled(has_rows, egui::Button::new("Salva profilo…"))
                    .clicked()
                {
                    self.save_profile();
                }
                if ui
                    .add_enabled(has_rows, egui::Button::new("Esporta .reg…"))
                    .clicked()
                {
                    self.export_reg();
                }
                ui.separator();
                let n = self.selected().len();
                let apply = egui::Button::new(
                    RichText::new(format!("Applica su questo PC ({n})")).strong(),
                );
                if ui.add_enabled(idle && n > 0, apply).clicked() {
                    self.apply_selected();
                }
                if ui
                    .add_enabled(idle, egui::Button::new("Annulla…"))
                    .clicked()
                {
                    self.undo_list = engine::list_backups(&self.backup_root());
                    self.undo_open = true;
                }
                if self.busy {
                    ui.spinner();
                }
            });
            if needs_admin && !self.is_admin {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(
                            "Alcune voci selezionate richiedono i privilegi di amministratore.",
                        )
                        .color(theme::WARN),
                    );
                    if ui.button("Riavvia come amministratore").clicked() {
                        self.relaunch_as_admin(&ctx);
                    }
                });
            }
            ui.add_space(4.0);
        });
        egui::Panel::bottom("log").show(ui, |ui| {
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
        egui::CentralPanel::default().show(ui, |ui| self.table(ui));
        if self.undo_open {
            self.undo_window(&ctx);
        }
        if self.busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
    }
}
