//! Interfaccia: elenco delle app, controllo e aggiornamento all'avvio, registro.

use crate::liber::config::{self, Config};
use crate::liber::inst_ui::{assoc_dialog, Assoc, AssocAct, InstApp};
use crate::liber::net::{Http, RealHttp};
use crate::liber::scan::{self, App};
use crate::liber::state;
use crate::liber::update::{self, Check, Kind, NoProbe, Probe, RealProbe};
use crate::liber::winget::{self, Pkg, RealRunner, Runner};
use crate::theme;
use eframe::egui::{self, Color32, RichText};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

/// Categorie disponibili (il catalogo usa le stesse), nell'ordine in cui si mostrano.
pub const CATEGORIES: [&str; 10] = [
    "Internet",
    "Ufficio",
    "Multimedia",
    "Immagini",
    "Sistema",
    "Sicurezza",
    "Sviluppo",
    "Utilità",
    "Giochi",
    "Altro",
];

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Unchecked,
    Checking,
    Updating,
    Done(String),
    Result(Check),
}

enum Event {
    Log(Kind, String),
    Status(PathBuf, Status),
    Rescan,
    Loaded(Vec<(App, state::State)>),
    Found(Result<Vec<Pkg>, String>),
    Finished,
}

struct Row {
    app: App,
    category: String,
    local: Option<String>,
    winget: Option<String>,
    status: Status,
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Portable,
    Installers,
}

pub struct LiberPanel {
    mode: Mode,
    runner: Arc<dyn Runner>,
    assoc: Option<Assoc>,
    confirm_trash: Option<usize>,
    inst: InstApp,
    icons: crate::liber::icons::Icons,
    inst_pending: bool,
    base: PathBuf,
    cfg_path: PathBuf,
    cfg: Config,
    rows: Vec<Row>,
    filter: String,
    cat: Option<String>,
    log: Vec<(Color32, String)>,
    busy: bool,
    started: bool,
    loading: bool,
    pending_check: bool,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    http: Arc<dyn Http>,
    probe: Arc<dyn Probe>,
}

fn r_has_winget(rows: &[Row], i: usize) -> bool {
    rows.get(i).is_some_and(|r| r.winget.is_some())
}

pub(crate) fn kind_color(k: Kind) -> Color32 {
    match k {
        Kind::Info => theme::LOG_INFO,
        Kind::Ok => theme::LOG_OK,
        Kind::Warn => theme::LOG_WARN,
        Kind::Err => theme::LOG_ERR,
    }
}

/// Colore pastello dell'iniziale, scelto dal nome.
/// Disegna l'icona di un programma dentro il riquadro.
pub(crate) fn paint_icon(ui: &egui::Ui, rect: egui::Rect, tex: &egui::TextureHandle) {
    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    // Il bianco qui è solo il "nessun filtro" dell'immagine, non un colore visibile.
    ui.painter()
        .image(tex.id(), rect.shrink(5.0), uv, egui::Color32::WHITE);
}

pub(crate) fn avatar_color(name: &str) -> Color32 {
    const COLORS: [Color32; 6] = [
        Color32::from_rgb(0xFF, 0xB3, 0xDE),
        Color32::from_rgb(0xB7, 0xC4, 0xFF),
        Color32::from_rgb(0xC4, 0xB5, 0xFD),
        Color32::from_rgb(0xFF, 0xB7, 0xC5),
        Color32::from_rgb(0xA5, 0xD8, 0xFF),
        Color32::from_rgb(0xD9, 0xC2, 0xFF),
    ];
    let h = name.bytes().fold(7usize, |a, b| {
        a.wrapping_mul(31).wrapping_add(usize::from(b))
    });
    COLORS[h % COLORS.len()]
}

/// Categoria: quella scelta dall'utente, poi la cartella in cui sta l'app, poi il catalogo,
/// poi «Altro».
pub fn category_of(app: &App, st: &state::State) -> String {
    st.category
        .clone()
        .filter(|c| !c.trim().is_empty())
        .or_else(|| app.folder_category.clone())
        .or_else(|| app.entry.map(|e| e.category.to_string()))
        .unwrap_or_else(|| "Altro".to_string())
}

/// Categoria principale (`Video › Editor` -> `Video`).
pub(crate) fn main_of(c: &str) -> &str {
    c.split(crate::liber::scan::SEPARATOR).next().unwrap_or(c)
}

/// Posizione di una categoria: quelle note nel loro ordine, le altre in ordine alfabetico,
/// «Altro» per ultima.
pub(crate) fn rank(c: &str) -> (u8, usize, String) {
    if c == "Altro" {
        return (2, 0, String::new());
    }
    match CATEGORIES.iter().position(|k| *k == c) {
        Some(p) => (0, p, String::new()),
        None => (1, 0, c.to_lowercase()),
    }
}

/// Indici delle righe per categoria; salta quelle vuote.
pub(crate) fn group(rows: &[(String, String)], filter: &str) -> Vec<(String, Vec<usize>)> {
    let f = filter.trim().to_lowercase();
    let mut cats: Vec<&String> = rows.iter().map(|(c, _)| c).collect();
    cats.sort_by_key(|c| (rank(main_of(c)), c.to_lowercase()));
    cats.dedup();
    cats.into_iter()
        .filter_map(|c| {
            let idx: Vec<usize> = rows
                .iter()
                .enumerate()
                .filter(|(_, (cat, name))| {
                    cat == c && (f.is_empty() || name.to_lowercase().contains(&f))
                })
                .map(|(i, _)| i)
                .collect();
            (!idx.is_empty()).then(|| (c.clone(), idx))
        })
        .collect()
}

impl LiberPanel {
    pub fn new() -> Self {
        let base = config::base_dir();
        let cfg_path = base.join("liberdesk.json");
        let cfg = config::load(&cfg_path);
        let (tx, rx) = channel();
        let inst = InstApp::new(cfg.effective_installers_dir(&base));
        let s = Self {
            mode: Mode::Portable,
            runner: Arc::new(RealRunner),
            assoc: None,
            confirm_trash: None,
            inst,
            icons: crate::liber::icons::Icons::default(),
            inst_pending: true,
            base,
            cfg_path,
            cfg,
            rows: Vec::new(),
            filter: String::new(),
            cat: None,
            log: Vec::new(),
            busy: false,
            started: false,
            loading: false,
            pending_check: false,
            tx,
            rx,
            http: Arc::new(RealHttp::new()),
            probe: Arc::new(RealProbe),
        };
        let dir = s.apps_dir();
        let _ = std::fs::create_dir_all(&dir);
        s
    }

    fn apps_dir(&self) -> PathBuf {
        self.cfg.effective_dir(&self.base)
    }

    fn say(&mut self, k: Kind, m: impl Into<String>) {
        self.log.push((kind_color(k), m.into()));
        if self.log.len() > 500 {
            self.log.remove(0);
        }
    }

    fn save_cfg(&mut self) {
        if let Err(e) = config::save(&self.cfg_path, &self.cfg) {
            self.say(Kind::Err, format!("Impostazioni non salvate: {e}"));
        }
    }

    /// Rilegge la cartella delle app, tenendo gli stati già noti.
    fn rescan(&mut self) {
        let apps = scan::scan(&self.apps_dir());
        let loaded = apps
            .into_iter()
            .map(|a| {
                let st = state::load(&a.dir);
                (a, st)
            })
            .collect();
        self.build_rows(loaded);
    }

    fn build_rows(&mut self, loaded: Vec<(App, state::State)>) {
        let old: Vec<(PathBuf, Status)> = self
            .rows
            .iter()
            .map(|r| (r.app.dir.clone(), r.status.clone()))
            .collect();
        self.rows = loaded
            .into_iter()
            .map(|(app, st)| {
                let local = update::local_version(&app, &st, &NoProbe);
                let status = old
                    .iter()
                    .find(|(d, _)| *d == app.dir)
                    .map_or(Status::Unchecked, |(_, s)| s.clone());
                let category = category_of(&app, &st);
                Row {
                    winget: st.winget_id.clone(),
                    app,
                    category,
                    local,
                    status,
                }
            })
            .collect();
    }

    /// Legge la cartella delle app in un altro thread, così la finestra si apre subito.
    fn load_async(&mut self, ctx: &egui::Context) {
        self.loading = true;
        let dir = self.apps_dir();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let loaded = scan::scan(&dir)
                .into_iter()
                .map(|a| {
                    let st = state::load(&a.dir);
                    (a, st)
                })
                .collect();
            let _ = tx.send(Event::Loaded(loaded));
            ctx.request_repaint();
        });
    }

    fn pump(&mut self) {
        let mut rescan = false;
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                Event::Log(k, m) => self.say(k, m),
                Event::Status(dir, st) => {
                    if let Some(r) = self.rows.iter_mut().find(|r| r.app.dir == dir) {
                        r.status = st;
                    }
                }
                Event::Rescan => rescan = true,
                Event::Found(res) => {
                    if let Some(a) = &mut self.assoc {
                        a.found(res);
                    }
                }
                Event::Loaded(l) => {
                    self.build_rows(l);
                    self.loading = false;
                }
                Event::Finished => self.busy = false,
            }
        }
        if rescan {
            self.rescan();
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

    /// Controlla le app indicate; se `install`, scarica e installa quelle con una versione nuova.
    fn check(&mut self, ctx: &egui::Context, only: Option<usize>, install: bool) {
        let targets: Vec<(App, Option<String>, Option<String>)> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(i, r)| {
                only.is_none_or(|o| o == *i) && (r.app.entry.is_some() || r.winget.is_some())
            })
            .map(|(_, r)| (r.app.clone(), r.local.clone(), r.winget.clone()))
            .collect();
        if targets.is_empty() {
            self.say(Kind::Info, "Nessuna app con aggiornamenti automatici.");
            return;
        }
        let total = self.rows.len();
        self.say(
            Kind::Info,
            format!(
                "{} app trovate, {} con aggiornamenti automatici",
                total,
                targets.len()
            ),
        );
        for r in &mut self.rows {
            if targets.iter().any(|(a, _, _)| a.dir == r.app.dir) {
                r.status = Status::Checking;
            }
        }
        let http = Arc::clone(&self.http);
        let probe = Arc::clone(&self.probe);
        let runner = Arc::clone(&self.runner);
        let explicit = only.is_some();
        self.spawn(ctx, move |tx| {
            let log = |k: Kind, m: String| {
                let _ = tx.send(Event::Log(k, m));
            };
            let mut changed = false;
            let mut wg_ok: Option<bool> = None;
            for (app, local, wid) in targets {
                // La versione dall'eseguibile si legge qui, fuori dall'interfaccia.
                let local = local.or_else(|| {
                    app.exe.as_ref().and_then(|e| probe.file_version(e))
                });
                let res = match (&app.entry, &wid) {
                    (None, Some(id)) => {
                        let ok = *wg_ok.get_or_insert_with(|| winget::available(&*runner));
                        if ok {
                            update::winget_check(&*runner, id, local.as_deref())
                        } else {
                            Check::Error("winget non disponibile su questo PC".into())
                        }
                    }
                    _ => update::check(&*http, &app, local.as_deref()),
                };
                match &res {
                    Check::Current(v) => log(Kind::Info, format!("{}: già aggiornata ({v})", app.name)),
                    Check::Available { latest, .. } => log(
                        Kind::Info,
                        format!(
                            "{}: nuova versione {latest} (hai {})",
                            app.name,
                            local.as_deref().unwrap_or("?")
                        ),
                    ),
                    Check::Winget { latest, .. } => log(
                        Kind::Info,
                        format!(
                            "{}: nuova versione {latest} su winget (hai {})",
                            app.name,
                            local.as_deref().unwrap_or("?")
                        ),
                    ),
                    Check::Unknown { latest, .. } => log(
                        Kind::Warn,
                        format!(
                            "{}: versione installata non nota, ultima {latest}: usa «Aggiorna» se vuoi sostituirla",
                            app.name
                        ),
                    ),
                    _ => {}
                }
                let st = Status::Result(res.clone());
                let _ = tx.send(Event::Status(app.dir.clone(), st));
                let (latest, asset, ok) = match res {
                    Check::Winget { latest, known } => {
                        if !(install && (known || explicit)) {
                            continue;
                        }
                        let id = wid.clone().unwrap_or_default();
                        let _ = tx.send(Event::Status(app.dir.clone(), Status::Updating));
                        let mut l = |k: Kind, m: &str| {
                            let _ = tx.send(Event::Log(k, m.to_string()));
                        };
                        match update::winget_apply(&*runner, &app, local.as_deref(), &latest, &id, &mut l) {
                            Ok(()) => {
                                changed = true;
                                let _ = tx.send(Event::Status(app.dir.clone(), Status::Done(latest)));
                            }
                            Err(e) => {
                                log(Kind::Err, format!("{}: {e}", app.name));
                                let _ = tx.send(Event::Status(app.dir.clone(), Status::Result(Check::Error(e))));
                            }
                        }
                        continue;
                    }
                    Check::Available { latest, asset } => (latest, asset, true),
                    // Con la versione sconosciuta si installa solo se l'utente lo chiede.
                    Check::Unknown { latest, asset } => (latest, asset, explicit),
                    Check::Error(e) => {
                        log(Kind::Warn, format!("{}: {e}", app.name));
                        continue;
                    }
                    _ => continue,
                };
                if !(install && ok) {
                    continue;
                }
                let _ = tx.send(Event::Status(app.dir.clone(), Status::Updating));
                let mut l = |k: Kind, m: &str| {
                    let _ = tx.send(Event::Log(k, m.to_string()));
                };
                match update::apply(&*http, &app, local.as_deref(), &latest, &asset, &mut l) {
                    Ok(()) => {
                        changed = true;
                        let _ = tx.send(Event::Status(app.dir.clone(), Status::Done(latest)));
                    }
                    Err(e) => {
                        log(Kind::Err, format!("{}: {e}", app.name));
                        let _ = tx.send(Event::Status(
                            app.dir.clone(),
                            Status::Result(Check::Error(e)),
                        ));
                    }
                }
            }
            if changed {
                let _ = tx.send(Event::Rescan);
            }
            log(Kind::Info, "Controllo finito.".into());
        });
    }

    fn launch(&mut self, idx: usize) {
        let Some(exe) = self.rows.get(idx).and_then(|r| r.app.exe.clone()) else {
            return;
        };
        let dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
        match std::process::Command::new(&exe).current_dir(dir).spawn() {
            Ok(_) => {}
            Err(e) => self.say(Kind::Err, format!("{}: {e}", exe.display())),
        }
    }

    fn search_update(&mut self, idx: usize) {
        let Some(name) = self.rows.get(idx).map(|r| {
            r.app
                .dir
                .file_name()
                .map_or(r.app.name.clone(), |n| n.to_string_lossy().to_string())
        }) else {
            return;
        };
        let url = crate::liber::search::url(&name);
        // `explorer <indirizzo>` apre Documenti: per i siti serve il gestore dei collegamenti.
        if let Err(e) = std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url.as_str()])
            .spawn()
        {
            self.say(Kind::Err, format!("Ricerca non aperta: {e}"));
        } else {
            self.say(
                Kind::Info,
                format!("Cerco: {}", crate::liber::search::query(&name)),
            );
        }
    }

    fn open_folder(&mut self) {
        let dir = self.apps_dir();
        if let Err(e) = std::process::Command::new("explorer").arg(&dir).spawn() {
            self.say(Kind::Err, format!("{}: {e}", dir.display()));
        }
    }

    fn status_text(r: &Row) -> (String, Color32) {
        match &r.status {
            Status::Unchecked => {
                if r.app.entry.is_some() || r.winget.is_some() {
                    ("Da controllare".into(), theme::MUTED)
                } else {
                    ("Senza aggiornamenti automatici".into(), theme::MUTED)
                }
            }
            Status::Checking => ("Controllo…".into(), theme::SCAN),
            Status::Updating => ("Aggiorno…".into(), theme::INSTALL),
            Status::Done(v) => (format!("Aggiornata alla {v}"), theme::OK),
            Status::Result(c) => match c {
                Check::NoSource => ("Senza aggiornamenti automatici".into(), theme::MUTED),
                Check::Current(_) => ("Aggiornata".into(), theme::OK),
                Check::Available { latest, .. } => {
                    (format!("Nuova versione {latest}"), theme::WARN)
                }
                Check::Winget { latest, known } => (
                    if *known {
                        format!("Nuova versione {latest} (winget)")
                    } else {
                        format!("Versione {latest} disponibile (la tua non è nota)")
                    },
                    theme::WARN,
                ),
                Check::Unknown { latest, .. } => (
                    format!("Versione {latest} disponibile (la tua non è nota)"),
                    theme::WARN,
                ),
                Check::Error(_) => ("Controllo non riuscito".into(), theme::ERR),
            },
        }
    }

    fn can_update(r: &Row) -> bool {
        matches!(
            r.status,
            Status::Result(Check::Available { .. } | Check::Unknown { .. } | Check::Winget { .. })
        )
    }

    fn set_winget(&mut self, idx: usize, id: Option<String>) {
        let Some(r) = self.rows.get_mut(idx) else {
            return;
        };
        let mut st = state::load(&r.app.dir);
        st.winget_id = id.clone();
        r.winget = id;
        let dir = r.app.dir.clone();
        if let Err(e) = state::save(&dir, &st) {
            self.say(Kind::Err, format!("Associazione non salvata: {e}"));
        }
    }

    /// Conferma e spostamento di un'app nel Cestino.
    fn trash_window(&mut self, ctx: &egui::Context) {
        let Some(i) = self.confirm_trash else {
            return;
        };
        let Some((name, dir)) = self
            .rows
            .get(i)
            .map(|r| (r.app.name.clone(), r.app.dir.clone()))
        else {
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
                    "«{name}» e tutti i suoi file (anche Data e impostazioni) vanno nel Cestino."
                ));
                ui.label(
                    RichText::new(dir.display().to_string())
                        .color(theme::MUTED)
                        .small(),
                );
                ui.label("Puoi recuperarli dal Cestino di Windows.");
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
            let root = self.apps_dir();
            match crate::liber::trash::to_recycle_bin(&root, &dir) {
                Ok(()) => {
                    self.say(Kind::Ok, format!("{name}: spostato nel Cestino"));
                    let _ = self.tx.send(Event::Rescan);
                }
                Err(e) => self.say(Kind::Err, format!("{name}: {e}")),
            }
        }
        if yes || no {
            self.confirm_trash = None;
        }
    }

    /// Finestra «Associa a winget» delle app portabili.
    fn assoc_window(&mut self, ctx: &egui::Context) {
        let Some(a) = &mut self.assoc else {
            return;
        };
        let name = self
            .rows
            .get(a.idx)
            .map(|r| r.app.name.clone())
            .unwrap_or_default();
        let idx = a.idx;
        match assoc_dialog(ctx, a, &name) {
            AssocAct::Search(q) => {
                let runner = Arc::clone(&self.runner);
                let tx = self.tx.clone();
                let c = ctx.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(Event::Found(winget::search(&*runner, &q)));
                    c.request_repaint();
                });
            }
            AssocAct::Choose(id) => {
                self.set_winget(idx, Some(id.clone()));
                self.say(Kind::Ok, format!("{name}: associato a winget ({id})"));
                self.assoc = None;
            }
            AssocAct::Close => self.assoc = None,
            AssocAct::None => {}
        }
    }

    fn set_category(&mut self, idx: usize, cat: &str) {
        let Some(r) = self.rows.get_mut(idx) else {
            return;
        };
        let mut st = state::load(&r.app.dir);
        st.category = Some(cat.to_string());
        let dir = r.app.dir.clone();
        r.category = cat.to_string();
        if let Err(e) = state::save(&dir, &st) {
            self.say(Kind::Err, format!("Categoria non salvata: {e}"));
        }
    }

    fn side(&mut self, ui: &mut egui::Ui) {
        let pairs: Vec<(String, String)> = self
            .rows
            .iter()
            .map(|r| (r.category.clone(), r.app.name.clone()))
            .collect();
        let mut mains: Vec<(String, usize)> = Vec::new();
        for (c, _) in &pairs {
            let m = main_of(c).to_string();
            match mains.iter_mut().find(|(x, _)| *x == m) {
                Some((_, n)) => *n += 1,
                None => mains.push((m, 1)),
            }
        }
        mains.sort_by_key(|(m, _)| rank(m));
        ui.add_space(6.0);
        ui.label(RichText::new("Categorie").strong().color(theme::MUTED));
        if ui
            .selectable_label(self.cat.is_none(), format!("Tutte ({})", self.rows.len()))
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

    fn list(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let pairs: Vec<(String, String)> = self
            .rows
            .iter()
            .map(|r| (r.category.clone(), r.app.name.clone()))
            .collect();
        let groups = group(&pairs, &self.filter);
        let mut all_cats: Vec<String> = CATEGORIES.iter().map(|c| (*c).to_string()).collect();
        for (c, _) in &pairs {
            if !all_cats.contains(c) {
                all_cats.push(c.clone());
            }
        }
        let mut launch = None;
        let mut trash: Option<usize> = None;
        let mut upd = None;
        let mut search = None;
        let mut moved: Option<(usize, String)> = None;
        let mut assoc: Option<(usize, bool)> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if self.rows.is_empty() && self.loading {
                ui.add_space(20.0);
                ui.label(RichText::new("Leggo la cartella delle app…").size(16.0).color(theme::MUTED));
            } else if self.rows.is_empty() {
                ui.add_space(20.0);
                ui.label(
                    RichText::new(format!(
                        "Nessuna app trovata in {}.\nMetti qui le app portabili, una cartella ciascuna: compariranno da sole.",
                        self.apps_dir().display()
                    ))
                    .size(16.0)
                    .color(theme::MUTED),
                );
            }
            for (cat, idx) in &groups {
                if self.cat.as_deref().is_some_and(|c| c != main_of(cat)) {
                    continue;
                }
                ui.add_space(4.0);
                ui.label(RichText::new(cat).size(18.0).strong().color(theme::PRIMARY));
                ui.separator();
                // Colonne calcolate a mano: le tessere riempiono la riga e non escono mai a destra.
                let gap = 10.0;
                let avail = ui.available_width() - 18.0;
                let cols = (((avail + gap) / (230.0 + gap)).floor() as usize).max(1);
                let tile_w = ((avail - gap * (cols as f32 - 1.0)) / cols as f32 - 24.0).max(150.0);
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
                                    ui.painter().rect_filled(rect, 12.0, avatar_color(&r.app.name));
                                    // L'icona del programma; se non c'è (ancora), la lettera.
                                    if let Some(tex) = r.app.exe.as_deref().and_then(|e| self.icons.get(ctx, e)) {
                                        paint_icon(ui, rect, tex);
                                    } else {
                                        let initial = r.app.name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
                                        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, initial, egui::FontId::proportional(24.0), theme::TEXT);
                                    }
                                    ui.vertical(|ui| {
                                        ui.add(egui::Label::new(RichText::new(&r.app.name).strong().size(15.0)).truncate());
                                        let ver = r.local.as_deref().map_or("versione non nota".to_string(), |v| format!("versione {v}"));
                                        ui.label(RichText::new(ver).color(theme::MUTED).small());
                                    });
                                });
                                if let Some(d) = crate::liber::describe::of(&r.app.name) {
                                    ui.add(egui::Label::new(RichText::new(d).color(theme::TEXT).size(13.5)).wrap());
                                }
                                let (t, c) = Self::status_text(r);
                                let resp = ui.add(egui::Label::new(RichText::new(t).color(c).small().strong()).truncate());
                                if let Status::Result(Check::Error(e)) = &r.status {
                                    resp.on_hover_text(e);
                                }
                                ui.horizontal(|ui| {
                                    let start = egui::Button::new(RichText::new("Avvia").strong().color(theme::ON_COLOR)).fill(theme::DOWNLOAD);
                                    if ui.add(start).clicked() {
                                        launch = Some(i);
                                    }
                                    let bin = egui::Button::new(RichText::new("Cestino").color(theme::ON_COLOR)).fill(theme::INSTALL);
                                    if ui.add(bin).on_hover_text("Sposta la cartella del programma nel Cestino").clicked() {
                                        trash = Some(i);
                                    }
                                    if r.app.entry.is_none()
                                        && r.winget.is_none()
                                        && ui
                                            .button("Cerca aggiornamento")
                                            .on_hover_text(crate::liber::search::query(&r.app.name))
                                            .clicked()
                                    {
                                        search = Some(i);
                                    }
                                    if Self::can_update(r)
                                        && ui.add_enabled(!self.busy, egui::Button::new("Aggiorna")).clicked()
                                    {
                                        upd = Some(i);
                                    }
                                });
                            });
                            });
                        tile.response.context_menu(|ui| {
                            if ui.button("Associa a winget…").clicked() {
                                assoc = Some((i, true));
                                ui.close();
                            }
                            if r_has_winget(&self.rows, i) && ui.button("Togli l'associazione winget").clicked() {
                                assoc = Some((i, false));
                                ui.close();
                            }
                            ui.menu_button("Sposta in…", |ui| {
                                for c in &all_cats {
                                    if ui.button(c).clicked() {
                                        moved = Some((i, c.clone()));
                                        ui.close();
                                    }
                                }
                            });
                        });
                    }
                  });
                }
                ui.add_space(8.0);
            }
        });
        if let Some(i) = launch {
            self.launch(i);
        }
        if let Some(i) = trash {
            self.confirm_trash = Some(i);
        }
        if let Some(i) = upd {
            self.check(ctx, Some(i), true);
        }
        if let Some(i) = search {
            self.search_update(i);
        }
        if let Some((i, c)) = moved {
            self.set_category(i, &c);
        }
        if let Some((i, add)) = assoc {
            if add {
                if let Some(r) = self.rows.get(i) {
                    let q = crate::liber::search::query(&r.app.name);
                    self.assoc = Some(Assoc::new(i, q));
                }
            } else {
                self.set_winget(i, None);
            }
        }
    }
}

impl LiberPanel {
    pub fn show(&mut self, ui: &mut egui::Ui) {
        self.pump();
        let ctx = ui.ctx().clone();
        self.icons.tick(&ctx);
        self.inst.pump(&ctx);
        for (c, m) in self.inst.take_logs() {
            self.log.push((c, m));
        }
        if !self.started {
            self.started = true;
            self.pending_check = self.cfg.auto_check;
            self.load_async(&ctx);
        }
        if self.pending_check && !self.loading {
            self.pending_check = false;
            self.say(Kind::Info, "Controllo gli aggiornamenti…");
            let install = self.cfg.auto_install;
            self.check(&ctx, None, install);
        }
        self.inst.ensure_loaded(&ctx);
        if self.inst_pending && self.cfg.auto_check && self.started && !self.inst.busy() {
            self.inst_pending = false;
            if self.inst.has_ids() {
                self.say(
                    Kind::Info,
                    "Controllo i programmi non portabili con winget…",
                );
                let install = self.cfg.auto_install;
                self.inst.check(&ctx, None, install);
            }
        }
        egui::Panel::top("liber_top").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.mode, Mode::Portable, "Portabili");
                ui.selectable_value(&mut self.mode, Mode::Installers, "Non portabili");
            });
            if self.mode == Mode::Installers {
                ui.horizontal_wrapped(|ui| {
                    let busy = self.inst.busy();
                    let b = egui::Button::new(
                        RichText::new("Controlla e scarica aggiornamenti (winget)")
                            .strong()
                            .color(theme::ON_COLOR),
                    )
                    .fill(theme::SCAN);
                    if ui.add_enabled(!busy, b).clicked() {
                        self.inst.check(&ctx, None, true);
                    }
                    if ui
                        .add_enabled(!busy, egui::Button::new("Solo controlla"))
                        .clicked()
                    {
                        self.inst.check(&ctx, None, false);
                    }
                    if ui.button("Crea tutti gli script silenziosi").clicked() {
                        self.inst.create_scripts();
                    }
                    if ui.button("Rileggi la cartella").clicked() {
                        self.inst.load(&ctx);
                    }
                    if ui.button("Apri la cartella").clicked() {
                        self.inst.open_folder();
                    }
                    if ui.button("Cambia cartella…").clicked() {
                        if let Some(p) = rfd::FileDialog::new().pick_folder() {
                            self.cfg.installers_dir = p.to_string_lossy().to_string();
                            self.save_cfg();
                            self.inst.set_root(p);
                            self.inst.load(&ctx);
                        }
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("Cerca:");
                    ui.add(egui::TextEdit::singleline(&mut self.inst.filter).desired_width(160.0));
                    ui.label(
                        RichText::new(self.inst.root().display().to_string()).color(theme::MUTED),
                    );
                });
            }
            if self.mode == Mode::Portable {
                ui.horizontal_wrapped(|ui| {
                    let busy = self.busy;
                    if ui
                        .add_enabled(
                            !busy,
                            egui::Button::new(
                                RichText::new("Controlla e aggiorna tutto")
                                    .strong()
                                    .color(theme::ON_COLOR),
                            )
                            .fill(theme::SCAN),
                        )
                        .clicked()
                    {
                        self.check(&ctx, None, true);
                    }
                    if ui
                        .add_enabled(!busy, egui::Button::new("Solo controlla"))
                        .clicked()
                    {
                        self.check(&ctx, None, false);
                    }
                    if ui.button("Rileggi la cartella").clicked() {
                        self.rescan();
                    }
                    if ui.button("Apri la cartella delle app").clicked() {
                        self.open_folder();
                    }
                    if ui.button("Cambia cartella…").clicked() {
                        if let Some(p) = rfd::FileDialog::new().pick_folder() {
                            self.cfg.apps_dir = p.to_string_lossy().to_string();
                            self.save_cfg();
                            self.rows.clear();
                            self.rescan();
                        }
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    let mut changed = false;
                    changed |= ui
                        .checkbox(&mut self.cfg.auto_check, "Controlla all'avvio")
                        .changed();
                    changed |= ui
                        .checkbox(
                            &mut self.cfg.auto_install,
                            "Installa subito gli aggiornamenti trovati",
                        )
                        .changed();
                    if changed {
                        self.save_cfg();
                    }
                    ui.label("Cerca:");
                    ui.add(egui::TextEdit::singleline(&mut self.filter).desired_width(160.0));
                    ui.label(
                        RichText::new(self.apps_dir().display().to_string()).color(theme::MUTED),
                    );
                });
            }
            ui.add_space(4.0);
        });
        egui::Panel::bottom("liber_log").show(ui, |ui| {
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
        egui::Panel::left("liber_cats")
            .default_size(170.0)
            .show(ui, |ui| match self.mode {
                Mode::Portable => self.side(ui),
                Mode::Installers => self.inst.side(ui),
            });
        egui::CentralPanel::default().show(ui, |ui| match self.mode {
            Mode::Portable => self.list(ui, &ctx),
            Mode::Installers => self.inst.list(ui, &ctx),
        });
        self.inst.windows(&ctx);
        self.assoc_window(&ctx);
        self.trash_window(&ctx);
        if self.busy || self.loading || self.inst.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::liber::catalog;

    fn row(status: Status) -> Row {
        Row {
            winget: None,
            app: App {
                name: "X".into(),
                dir: PathBuf::from("X"),
                exe: None,
                entry: catalog::find_for_folder("Rufus"),
                root: PathBuf::from("."),
                folder_category: None,
            },
            category: "Sistema".into(),
            local: None,
            status,
        }
    }

    #[test]
    fn status_texts_and_update_button() {
        assert_eq!(
            LiberPanel::status_text(&row(Status::Unchecked)).0,
            "Da controllare"
        );
        let mut r = row(Status::Unchecked);
        r.app.entry = None;
        assert!(LiberPanel::status_text(&r)
            .0
            .contains("Senza aggiornamenti"));
        assert_eq!(
            LiberPanel::status_text(&row(Status::Result(Check::Current("1".into())))).0,
            "Aggiornata"
        );
        assert!(!LiberPanel::can_update(&row(Status::Unchecked)));
        assert!(!LiberPanel::can_update(&row(Status::Result(
            Check::Current("1".into())
        ))));
        let a = crate::liber::github::Asset {
            name: "a.zip".into(),
            browser_download_url: "https://x".into(),
            size: 1,
        };
        assert!(LiberPanel::can_update(&row(Status::Result(
            Check::Available {
                latest: "2".into(),
                asset: a.clone()
            }
        ))));
        assert!(LiberPanel::can_update(&row(Status::Result(
            Check::Unknown {
                latest: "2".into(),
                asset: a
            }
        ))));
    }

    #[test]
    fn groups_follow_the_fixed_order_and_filter() {
        let rows = vec![
            ("Sistema".to_string(), "Rufus".to_string()),
            ("Internet".to_string(), "Firefox".to_string()),
            ("Sistema".to_string(), "BCU".to_string()),
            ("Altro".to_string(), "Mio".to_string()),
        ];
        let g = group(&rows, "");
        let names: Vec<_> = g.iter().map(|(c, _)| c.as_str()).collect();
        assert_eq!(names, vec!["Internet", "Sistema", "Altro"]);
        assert_eq!(g[1].1, vec![0, 2]);
        let f = group(&rows, "ruf");
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].1, vec![0]);
        // Categorie delle cartelle dell'utente: dopo quelle note, «Altro» in fondo.
        let r2 = vec![
            ("Altro".to_string(), "A".to_string()),
            ("Zeta".to_string(), "B".to_string()),
            ("Beta".to_string(), "C".to_string()),
            ("Sistema".to_string(), "D".to_string()),
        ];
        let n: Vec<_> = group(&r2, "").into_iter().map(|(c, _)| c).collect();
        assert_eq!(n, vec!["Sistema", "Beta", "Zeta", "Altro"]);
        let r3 = vec![
            ("Video › Editor".to_string(), "A".to_string()),
            ("Audio".to_string(), "B".to_string()),
            ("Video".to_string(), "C".to_string()),
            ("Video › Riproduttore".to_string(), "D".to_string()),
        ];
        let n: Vec<_> = group(&r3, "").into_iter().map(|(c, _)| c).collect();
        assert_eq!(
            n,
            vec!["Audio", "Video", "Video › Editor", "Video › Riproduttore"]
        );
        assert_eq!(main_of("Video › Editor"), "Video");
        assert_eq!(main_of("Audio"), "Audio");
    }

    #[test]
    fn category_user_choice_wins() {
        let mut a = row(Status::Unchecked).app;
        let mut st = state::State::default();
        assert_eq!(category_of(&a, &st), "Sistema");
        st.category = Some("Giochi".into());
        assert_eq!(category_of(&a, &st), "Giochi");
        st.category = Some("  ".into());
        assert_eq!(category_of(&a, &st), "Sistema");
        a.folder_category = Some("Utility".into());
        assert_eq!(category_of(&a, &state::State::default()), "Utility");
        a.folder_category = None;
        a.entry = None;
        assert_eq!(category_of(&a, &state::State::default()), "Altro");
    }

    #[test]
    fn catalog_categories_are_all_known() {
        for e in crate::liber::catalog::CATALOG {
            assert!(CATEGORIES.contains(&e.category), "{}", e.category);
        }
    }

    #[test]
    fn avatar_color_is_stable() {
        assert_eq!(avatar_color("Notepad++"), avatar_color("Notepad++"));
    }
}
