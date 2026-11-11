//! Scheda «Non portabili»: installer da aggiornare con winget e script silenziosi.

use crate::liber::inst_update::{self, IStatus};
use crate::liber::installers::{self, Entry, Installer, Store};
use crate::liber::panel::{avatar_color, group, kind_color, main_of, rank};
use crate::liber::update::Kind;
use crate::liber::winget::{self, Pkg, RealRunner, Runner};
use crate::theme;
use eframe::egui::{self, Color32, RichText};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

enum Ev {
    Loaded(Vec<Installer>, Store),
    Status(String, IStatus),
    Log(Kind, String),
    Found(Result<Vec<Pkg>, String>),
    Finished,
    Reload,
}

struct IRow {
    ins: Installer,
    entry: Entry,
    status: IStatus,
}

/// Finestra «Associa a winget»: ricerca di un pacchetto e scelta.
pub struct Assoc {
    pub idx: usize,
    pub query: String,
    pub results: Vec<Pkg>,
    pub busy: bool,
    pub error: Option<String>,
}

impl Assoc {
    pub fn new(idx: usize, query: String) -> Self {
        Self {
            idx,
            query,
            results: Vec::new(),
            busy: false,
            error: None,
        }
    }

    /// Mostra l'esito di una ricerca.
    pub fn found(&mut self, res: Result<Vec<Pkg>, String>) {
        self.busy = false;
        match res {
            Ok(v) => {
                self.error = v.is_empty().then(|| "Nessun risultato.".to_string());
                self.results = v;
            }
            Err(e) => self.error = Some(e),
        }
    }
}

pub enum AssocAct {
    None,
    Search(String),
    Choose(String),
    Close,
}

pub fn assoc_dialog(ctx: &egui::Context, a: &mut Assoc, name: &str) -> AssocAct {
    let mut act = AssocAct::None;
    egui::Window::new(format!("Associa «{name}» a winget"))
        .collapsible(false)
        .default_width(520.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                let te = ui.add(egui::TextEdit::singleline(&mut a.query).desired_width(280.0));
                let go = ui
                    .add_enabled(!a.busy, egui::Button::new("Cerca"))
                    .clicked()
                    || (te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                if go && !a.busy {
                    a.busy = true;
                    act = AssocAct::Search(a.query.clone());
                }
                if ui.button("Chiudi").clicked() {
                    act = AssocAct::Close;
                }
            });
            if a.busy {
                ui.label("Cerco…");
            }
            if let Some(e) = &a.error {
                ui.label(RichText::new(e).color(theme::ERR));
            }
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .show(ui, |ui| {
                    for p in &a.results {
                        if ui
                            .button(format!("{} — {} ({})", p.name, p.id, p.version))
                            .clicked()
                        {
                            act = AssocAct::Choose(p.id.clone());
                        }
                    }
                });
        });
    act
}

struct Switches {
    idx: usize,
    text: String,
}

pub struct InstApp {
    root: PathBuf,
    rows: Vec<IRow>,
    store: Store,
    pub filter: String,
    cat: Option<String>,
    loading: bool,
    busy: bool,
    started: bool,
    logs: Vec<(Color32, String)>,
    tx: Sender<Ev>,
    rx: Receiver<Ev>,
    runner: Arc<dyn Runner>,
    assoc: Option<Assoc>,
    switches: Option<Switches>,
    confirm_trash: Option<usize>,
}

impl InstApp {
    pub fn new(root: PathBuf) -> Self {
        let (tx, rx) = channel();
        let _ = std::fs::create_dir_all(&root);
        Self {
            root,
            rows: Vec::new(),
            store: Store::default(),
            filter: String::new(),
            cat: None,
            loading: false,
            busy: false,
            started: false,
            logs: Vec::new(),
            tx,
            rx,
            runner: Arc::new(RealRunner),
            assoc: None,
            switches: None,
            confirm_trash: None,
        }
    }

    pub fn set_root(&mut self, root: PathBuf) {
        let _ = std::fs::create_dir_all(&root);
        self.root = root;
        self.rows.clear();
        self.started = false;
    }

    pub fn root(&self) -> &PathBuf {
        &self.root
    }

    pub fn has_ids(&self) -> bool {
        self.rows.iter().any(|r| r.entry.winget_id.is_some())
    }

    pub fn busy(&self) -> bool {
        self.busy || self.loading
    }

    pub fn take_logs(&mut self) -> Vec<(Color32, String)> {
        std::mem::take(&mut self.logs)
    }

    fn say(&mut self, k: Kind, m: impl Into<String>) {
        self.logs.push((kind_color(k), m.into()));
    }

    /// Legge la cartella in un altro thread; `auto` avvia poi controllo e download.
    pub fn load(&mut self, ctx: &egui::Context) {
        self.started = true;
        self.loading = true;
        let root = self.root.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let v = installers::scan(&root);
            let st = installers::load_store(&root);
            let _ = tx.send(Ev::Loaded(v, st));
            ctx.request_repaint();
        });
    }

    pub fn ensure_loaded(&mut self, ctx: &egui::Context) {
        if !self.started {
            self.load(ctx);
        }
    }

    pub fn pump(&mut self, ctx: &egui::Context) {
        let mut reload = false;
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                Ev::Loaded(v, st) => {
                    let old: Vec<(String, IStatus)> = self
                        .rows
                        .iter()
                        .map(|r| (r.ins.rel.clone(), r.status.clone()))
                        .collect();
                    self.rows = v
                        .into_iter()
                        .map(|ins| {
                            let entry = st.items.get(&ins.rel).cloned().unwrap_or_default();
                            let status = old
                                .iter()
                                .find(|(r, _)| *r == ins.rel)
                                .map_or(IStatus::Unchecked, |(_, s)| s.clone());
                            IRow { ins, entry, status }
                        })
                        .collect();
                    self.store = st;
                    self.loading = false;
                }
                Ev::Status(rel, s) => {
                    if let Some(r) = self.rows.iter_mut().find(|r| r.ins.rel == rel) {
                        r.status = s;
                    }
                }
                Ev::Log(k, m) => self.say(k, m),
                Ev::Found(res) => {
                    if let Some(a) = &mut self.assoc {
                        a.found(res);
                    }
                }
                Ev::Finished => self.busy = false,
                Ev::Reload => reload = true,
            }
        }
        if reload {
            self.load(ctx);
        }
    }

    fn spawn(&mut self, ctx: &egui::Context, job: impl FnOnce(&Sender<Ev>) + Send + 'static) {
        self.busy = true;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            job(&tx);
            let _ = tx.send(Ev::Finished);
            ctx.request_repaint();
        });
    }

    /// Controlla con winget; se `install`, scarica anche i nuovi installer (non li esegue).
    pub fn check(&mut self, ctx: &egui::Context, only: Option<usize>, install: bool) {
        let targets: Vec<(Installer, Entry)> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(i, r)| only.is_none_or(|o| o == *i) && r.entry.winget_id.is_some())
            .map(|(_, r)| (r.ins.clone(), r.entry.clone()))
            .collect();
        if targets.is_empty() {
            self.say(
                Kind::Warn,
                "Nessun programma associato a winget: tasto destro → «Associa a winget…».",
            );
            return;
        }
        for r in &mut self.rows {
            if targets.iter().any(|(i, _)| i.rel == r.ins.rel) {
                r.status = IStatus::Checking;
            }
        }
        let runner = Arc::clone(&self.runner);
        let root = self.root.clone();
        let mut store = self.store.clone();
        self.spawn(ctx, move |tx| {
            let log = |k: Kind, m: String| {
                let _ = tx.send(Ev::Log(k, m));
            };
            if !winget::available(&*runner) {
                log(
                    Kind::Err,
                    "winget non è disponibile su questo PC (serve «App Installer» di Windows)."
                        .into(),
                );
                for (i, _) in &targets {
                    let _ = tx.send(Ev::Status(
                        i.rel.clone(),
                        IStatus::Error("winget non disponibile".into()),
                    ));
                }
                return;
            }
            let mut changed = false;
            for (ins, e) in targets {
                let s = inst_update::check(&*runner, &ins, &e);
                match &s {
                    IStatus::Newer(v) => {
                        log(Kind::Info, format!("{}: nuova versione {v}", ins.name))
                    }
                    IStatus::Current(v) => {
                        log(Kind::Info, format!("{}: già aggiornato ({v})", ins.name))
                    }
                    IStatus::Error(m) => log(Kind::Warn, format!("{}: {m}", ins.name)),
                    _ => {}
                }
                let latest = match &s {
                    IStatus::Newer(v) => Some(v.clone()),
                    _ => None,
                };
                let _ = tx.send(Ev::Status(ins.rel.clone(), s));
                let Some(latest) = latest.filter(|_| install) else {
                    continue;
                };
                let _ = tx.send(Ev::Status(ins.rel.clone(), IStatus::Downloading));
                match inst_update::update(&*runner, &root, &ins, &mut store, &latest) {
                    Ok(p) => {
                        changed = true;
                        log(
                            Kind::Ok,
                            format!("{}: scaricato {}", ins.name, installers::base_name_of(&p)),
                        );
                        let _ = tx.send(Ev::Status(ins.rel.clone(), IStatus::Done(latest)));
                    }
                    Err(m) => {
                        log(Kind::Err, format!("{}: {m}", ins.name));
                        let _ = tx.send(Ev::Status(ins.rel.clone(), IStatus::Error(m)));
                    }
                }
            }
            if changed {
                let _ = tx.send(Ev::Reload);
            }
            log(Kind::Info, "Controllo dei non portabili finito.".into());
        });
    }

    /// Crea lo script silenzioso di ogni installer e quello che li lancia tutti.
    pub fn create_scripts(&mut self) {
        let mut made: Vec<(String, String)> = Vec::new();
        let mut unknown = Vec::new();
        let mut errors = Vec::new();
        for r in &self.rows {
            match installers::write_script(&r.ins, r.entry.switches.as_deref()) {
                Ok(Some(p)) => {
                    let rel = p
                        .strip_prefix(&self.root)
                        .map(|x| x.to_string_lossy().replace('\\', "/"))
                        .unwrap_or_else(|_| installers::script_name(&r.ins));
                    made.push((r.ins.name.clone(), rel));
                }
                Ok(None) => unknown.push(r.ins.name.clone()),
                Err(e) => errors.push(e),
            }
        }
        let master = self.root.join("InstallaTutto.cmd");
        if made.is_empty() {
            self.say(
                Kind::Warn,
                "Nessuno script creato: metti degli installer (.exe, .msi) nella cartella.",
            );
        } else if let Err(e) = std::fs::write(&master, installers::master_text(&made)) {
            errors.push(format!("{}: {e}", master.display()));
        } else {
            self.say(
                Kind::Ok,
                format!(
                    "Creati {} script silenziosi e InstallaTutto.cmd",
                    made.len()
                ),
            );
        }
        for n in unknown {
            self.say(Kind::Warn, format!("{n}: tipo di installer non riconosciuto, tasto destro → «Parametri silenziosi…»"));
        }
        for e in errors {
            self.say(Kind::Err, e);
        }
    }

    fn run_script(&mut self, idx: usize) {
        let Some(r) = self.rows.get(idx) else {
            return;
        };
        match installers::write_script(&r.ins, r.entry.switches.as_deref()) {
            Ok(Some(p)) => {
                let path = p.to_string_lossy().replace('\'', "''");
                let cmd = format!("Start-Process -FilePath '{path}' -Verb RunAs");
                let name = r.ins.name.clone();
                match crate::clean::sys::quiet(std::process::Command::new("powershell").args([
                    "-NoProfile",
                    "-Command",
                    cmd.as_str(),
                ]))
                .spawn()
                {
                    Ok(_) => self.say(
                        Kind::Info,
                        format!("Installo {name} (conferma i permessi di amministratore)"),
                    ),
                    Err(e) => self.say(Kind::Err, format!("{name}: {e}")),
                }
            }
            Ok(None) => {
                let n = r.ins.name.clone();
                self.say(
                    Kind::Warn,
                    format!("{n}: tipo non riconosciuto, imposta i «Parametri silenziosi…»"),
                );
            }
            Err(e) => self.say(Kind::Err, e),
        }
    }

    fn set_entry(&mut self, idx: usize, f: impl FnOnce(&mut Entry)) {
        let Some(r) = self.rows.get_mut(idx) else {
            return;
        };
        f(&mut r.entry);
        self.store.items.insert(r.ins.rel.clone(), r.entry.clone());
        if let Err(e) = installers::save_store(&self.root, &self.store) {
            self.say(Kind::Err, e);
        }
    }

    pub fn open_folder(&mut self) {
        if let Err(e) = std::process::Command::new("explorer")
            .arg(&self.root)
            .spawn()
        {
            self.say(Kind::Err, format!("{}: {e}", self.root.display()));
        }
    }

    fn search_web(&mut self, idx: usize) {
        let Some(name) = self.rows.get(idx).map(|r| r.ins.name.clone()) else {
            return;
        };
        let url = crate::liber::search::url(&name);
        if let Err(e) = std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url.as_str()])
            .spawn()
        {
            self.say(Kind::Err, format!("Ricerca non aperta: {e}"));
        }
    }

    fn status_text(r: &IRow) -> (String, Color32) {
        match &r.status {
            IStatus::Unchecked => (
                if r.entry.winget_id.is_some() {
                    "da controllare".into()
                } else {
                    "senza pacchetto winget".into()
                },
                theme::MUTED,
            ),
            IStatus::Checking => ("controllo…".into(), theme::MUTED),
            IStatus::Current(v) => (format!("aggiornato ({v})"), theme::OK),
            IStatus::Newer(v) => (format!("nuova versione {v}"), theme::WARN),
            IStatus::Downloading => ("scarico…".into(), theme::PRIMARY),
            IStatus::Done(v) => (format!("scaricato {v}: pronto da installare"), theme::OK),
            IStatus::NoId => ("senza pacchetto winget".into(), theme::MUTED),
            IStatus::Error(e) => (format!("errore: {e}"), theme::ERR),
        }
    }

    pub fn side(&mut self, ui: &mut egui::Ui) {
        let mut mains: Vec<(String, usize)> = Vec::new();
        for r in &self.rows {
            let m = main_of(r.ins.category.as_deref().unwrap_or("Altro")).to_string();
            match mains.iter_mut().find(|(x, _)| *x == m) {
                Some((_, n)) => *n += 1,
                None => mains.push((m, 1)),
            }
        }
        mains.sort_by_key(|(m, _)| rank(m));
        ui.add_space(6.0);
        ui.label(RichText::new("Categorie").strong().color(theme::MUTED));
        if ui
            .selectable_label(self.cat.is_none(), format!("Tutti ({})", self.rows.len()))
            .clicked()
        {
            self.cat = None;
        }
        for (c, n) in &mains {
            let sel = self.cat.as_deref() == Some(c.as_str());
            if ui.selectable_label(sel, format!("{c} ({n})")).clicked() {
                self.cat = Some(c.clone());
            }
        }
    }

    pub fn list(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let pairs: Vec<(String, String)> = self
            .rows
            .iter()
            .map(|r| {
                (
                    r.ins.category.clone().unwrap_or_else(|| "Altro".into()),
                    r.ins.name.clone(),
                )
            })
            .collect();
        let groups = group(&pairs, &self.filter);
        let mut install = None;
        let mut upd = None;
        let mut search = None;
        let mut trash = None;
        let mut assoc = None;
        let mut sw = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if self.rows.is_empty() {
                ui.add_space(20.0);
                let t = if self.loading {
                    "Leggo la cartella…".to_string()
                } else {
                    format!(
                        "Nessun installer in {}.\nMetti qui i programmi non portabili (.exe, .msi), anche in sottocartelle per categoria.",
                        self.root.display()
                    )
                };
                ui.label(RichText::new(t).size(16.0).color(theme::MUTED));
            }
            for (cat, idx) in &groups {
                if self.cat.as_deref().is_some_and(|c| c != main_of(cat)) {
                    continue;
                }
                ui.add_space(4.0);
                ui.label(RichText::new(cat).size(18.0).strong().color(theme::PRIMARY));
                ui.separator();
                let gap = 10.0;
                let avail = ui.available_width() - 18.0;
                let cols = (((avail + gap) / (250.0 + gap)).floor() as usize).max(1);
                let tile_w = ((avail - gap * (cols as f32 - 1.0)) / cols as f32 - 24.0).max(170.0);
                for row_idx in idx.chunks(cols) {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
                        for &i in row_idx {
                            let Some(r) = self.rows.get(i) else {
                                continue;
                            };
                            let tile = egui::Frame::new()
                                .fill(theme::SURFACE)
                                .stroke(egui::Stroke::new(1.0, theme::BORDER))
                                .corner_radius(12.0)
                                .inner_margin(egui::Margin::same(10))
                                .show(ui, |ui| {
                                    ui.vertical(|ui| {
                                        ui.set_width(tile_w);
                                        ui.horizontal(|ui| {
                                            let (rect, _) = ui.allocate_exact_size(egui::vec2(46.0, 46.0), egui::Sense::hover());
                                            ui.painter().rect_filled(rect, 12.0, avatar_color(&r.ins.name));
                                            let c = r.ins.name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
                                            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, c, egui::FontId::proportional(24.0), theme::TEXT);
                                            ui.vertical(|ui| {
                                                ui.add(egui::Label::new(RichText::new(&r.ins.name).strong().size(15.0)).truncate());
                                                let kind = if r.entry.switches.is_some() { "parametri tuoi" } else { r.ins.kind.label() };
                                                let v = inst_update::local_version(&r.ins, &r.entry).map_or(String::new(), |v| format!(" · {v}"));
                                                ui.label(RichText::new(format!("{kind}{v}")).color(theme::MUTED).small());
                                            });
                                        });
                                        if let Some(d) = crate::liber::describe::of(&r.ins.name) {
                                            ui.add(egui::Label::new(RichText::new(d).color(theme::TEXT).size(13.5)).wrap());
                                        }
                                        let (t, c) = Self::status_text(r);
                                        let resp = ui.add(egui::Label::new(RichText::new(t.clone()).color(c).small().strong()).truncate());
                                        if matches!(r.status, IStatus::Error(_)) {
                                            resp.on_hover_text(t);
                                        }
                                        ui.horizontal(|ui| {
                                            let b = egui::Button::new(RichText::new("Installa").strong().color(theme::ON_COLOR)).fill(theme::DOWNLOAD);
                                            if ui.add(b).clicked() {
                                                install = Some(i);
                                            }
                                            if let IStatus::Newer(_) = r.status {
                                                if ui.add_enabled(!self.busy, egui::Button::new("Aggiorna")).clicked() {
                                                    upd = Some(i);
                                                }
                                            }
                                            let bin = egui::Button::new(RichText::new("Cestino").color(theme::ON_COLOR)).fill(theme::INSTALL);
                                            if ui.add(bin).on_hover_text("Sposta l'installer nel Cestino").clicked() {
                                                trash = Some(i);
                                            }
                                            if r.entry.winget_id.is_none() && ui.button("Cerca aggiornamento").clicked() {
                                                search = Some(i);
                                            }
                                        });
                                    });
                                });
                            tile.response.context_menu(|ui| {
                                if ui.button("Associa a winget…").clicked() {
                                    assoc = Some(i);
                                    ui.close();
                                }
                                if ui.button("Parametri silenziosi…").clicked() {
                                    sw = Some(i);
                                    ui.close();
                                }
                                if r.entry.winget_id.is_some() && ui.button("Togli l'associazione winget").clicked() {
                                    assoc = Some(usize::MAX - i);
                                    ui.close();
                                }
                            });
                        }
                    });
                }
                ui.add_space(8.0);
            }
        });
        if let Some(i) = install {
            self.run_script(i);
        }
        if let Some(i) = trash {
            self.confirm_trash = Some(i);
        }
        if let Some(i) = upd {
            self.check(ctx, Some(i), true);
        }
        if let Some(i) = search {
            self.search_web(i);
        }
        if let Some(a) = assoc {
            if a > usize::MAX / 2 {
                self.set_entry(usize::MAX - a, |e| e.winget_id = None);
            } else if let Some(r) = self.rows.get(a) {
                let q = crate::liber::search::query(&r.ins.name);
                self.assoc = Some(Assoc {
                    idx: a,
                    query: q,
                    results: Vec::new(),
                    busy: false,
                    error: None,
                });
            }
        }
        if let Some(i) = sw {
            if let Some(r) = self.rows.get(i) {
                let text = r
                    .entry
                    .switches
                    .clone()
                    .or_else(|| r.ins.kind.switches().map(Into::into))
                    .unwrap_or_default();
                self.switches = Some(Switches { idx: i, text });
            }
        }
    }

    /// Conferma e spostamento di un installer (con il suo script) nel Cestino.
    fn trash_window(&mut self, ctx: &egui::Context) {
        let Some(i) = self.confirm_trash else {
            return;
        };
        let Some(ins) = self.rows.get(i).map(|r| r.ins.clone()) else {
            self.confirm_trash = None;
            return;
        };
        let mut yes = false;
        let mut no = false;
        egui::Window::new("Spostare nel Cestino?")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(format!(
                    "L'installer «{}» e il suo script vanno nel Cestino.",
                    ins.name
                ));
                ui.label(
                    RichText::new(ins.file.display().to_string())
                        .color(theme::MUTED)
                        .small(),
                );
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("Sposta nel Cestino")
                                    .strong()
                                    .color(theme::ON_COLOR),
                            )
                            .fill(theme::INSTALL),
                        )
                        .clicked()
                    {
                        yes = true;
                    }
                    if ui.button("Annulla").clicked() {
                        no = true;
                    }
                });
            });
        if yes {
            let root = self.root.clone();
            match crate::liber::trash::to_recycle_bin(&root, &ins.file) {
                Ok(()) => {
                    if let Some(dir) = ins.file.parent() {
                        // Lo script silenzioso accanto all'installer non serve più.
                        let script = dir.join(installers::script_name(&ins));
                        if script.is_file() {
                            let _ = crate::liber::trash::to_recycle_bin(&root, &script);
                        }
                    }
                    self.store.items.remove(&ins.rel);
                    if let Err(e) = installers::save_store(&root, &self.store) {
                        self.say(Kind::Err, e);
                    }
                    self.say(Kind::Ok, format!("{}: spostato nel Cestino", ins.name));
                    self.load(ctx);
                }
                Err(e) => self.say(Kind::Err, format!("{}: {e}", ins.name)),
            }
        }
        if yes || no {
            self.confirm_trash = None;
        }
    }

    /// Finestre di dialogo (associazione winget e parametri).
    pub fn windows(&mut self, ctx: &egui::Context) {
        self.trash_window(ctx);
        let mut do_search: Option<String> = None;
        let mut chosen: Option<(usize, String)> = None;
        let mut close = false;
        if let Some(a) = &mut self.assoc {
            let name = self
                .rows
                .get(a.idx)
                .map(|r| r.ins.name.clone())
                .unwrap_or_default();
            match assoc_dialog(ctx, a, &name) {
                AssocAct::Search(q) => do_search = Some(q),
                AssocAct::Choose(id) => chosen = Some((a.idx, id)),
                AssocAct::Close => close = true,
                AssocAct::None => {}
            }
        }
        if let Some(q) = do_search {
            let runner = Arc::clone(&self.runner);
            let tx = self.tx.clone();
            let c = ctx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(Ev::Found(winget::search(&*runner, &q)));
                c.request_repaint();
            });
        }
        if let Some((i, id)) = chosen {
            self.set_entry(i, |e| e.winget_id = Some(id.clone()));
            self.say(Kind::Ok, format!("Associato a winget: {id}"));
            close = true;
        }
        if close {
            self.assoc = None;
        }
        let mut save: Option<(usize, String)> = None;
        let mut close_sw = false;
        if let Some(s) = &mut self.switches {
            let name = self
                .rows
                .get(s.idx)
                .map(|r| r.ins.name.clone())
                .unwrap_or_default();
            egui::Window::new(format!("Parametri silenziosi di «{name}»")).collapsible(false).default_width(460.0).show(ctx, |ui| {
                ui.label("Parametri passati all'installer (vuoto = quelli riconosciuti automaticamente).");
                ui.add(egui::TextEdit::singleline(&mut s.text).desired_width(420.0));
                ui.horizontal(|ui| {
                    if ui.button("Salva").clicked() {
                        save = Some((s.idx, s.text.clone()));
                    }
                    if ui.button("Annulla").clicked() {
                        close_sw = true;
                    }
                });
            });
        }
        if let Some((i, t)) = save {
            let t = t.trim().to_string();
            let auto = self.rows.get(i).and_then(|r| r.ins.kind.switches());
            self.set_entry(i, |e| {
                e.switches = (!t.is_empty() && Some(t.as_str()) != auto).then(|| t.clone())
            });
            self.create_scripts();
            close_sw = true;
        }
        if close_sw {
            self.switches = None;
        }
    }
}
