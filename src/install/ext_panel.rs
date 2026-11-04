//! Scheda «Estensioni Chrome»: estrae le estensioni e le reinstalla.

use crate::install::chrome::{self, Ext};
use crate::install::extensions::{self as install, ExtList};
use crate::theme;
use eframe::egui::{self, Color32, RichText};
use std::collections::BTreeSet;
use std::path::PathBuf;

const BATCH: usize = 8;

struct Row {
    ext: Ext,
    selected: bool,
    /// Già installata in questo Chrome.
    present: bool,
    /// Viene da un elenco caricato e non da questo PC.
    imported: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Store,
    Policy,
}

pub struct ExtPanel {
    user_data: Option<PathBuf>,
    out_dir: PathBuf,
    /// Messaggi da mostrare nel registro dell'app.
    pub msgs: Vec<(Color32, String)>,
    rows: Vec<Row>,
    filter: String,
    install_open: bool,
    mode: Mode,
    /// Quanti gruppi di schede sono già stati aperti.
    opened_batches: usize,
}

impl ExtPanel {
    pub fn new(app_dir: &std::path::Path) -> Self {
        let mut p = Self {
            user_data: chrome::default_user_data(),
            out_dir: app_dir.join("Estensioni"),
            msgs: Vec::new(),
            rows: Vec::new(),
            filter: String::new(),
            install_open: false,
            mode: Mode::Store,
            opened_batches: 0,
        };
        p.extract();
        p
    }

    fn say(&mut self, color: Color32, msg: String) {
        self.msgs.push((color, msg));
    }

    fn local(&self) -> Vec<Ext> {
        self.user_data
            .as_deref()
            .map(chrome::scan)
            .unwrap_or_default()
    }

    /// Passo 1: legge le estensioni di Chrome di questo PC.
    fn extract(&mut self) {
        let Some(ud) = self.user_data.clone() else {
            self.say(
                theme::LOG_ERR,
                "Cartella di Chrome non trovata: scegline una.".into(),
            );
            return;
        };
        let list = chrome::scan(&ud);
        if list.is_empty() {
            self.say(
                theme::LOG_WARN,
                format!(
                    "Nessuna estensione in {} (Chrome è installato? Usa «Cartella Chrome…»).",
                    ud.display()
                ),
            );
        } else {
            self.say(
                theme::LOG_OK,
                format!("Trovate {} estensioni in {}", list.len(), ud.display()),
            );
        }
        self.rows = list
            .into_iter()
            .map(|ext| Row {
                selected: ext.store && !ext.theme,
                ext,
                present: true,
                imported: false,
            })
            .collect();
        self.opened_batches = 0;
    }

    fn choose_chrome_folder(&mut self) {
        if let Some(d) = rfd::FileDialog::new()
            .set_title("Cartella «User Data» di Chrome")
            .pick_folder()
        {
            self.user_data = Some(d);
            self.extract();
        }
    }

    fn selected_exts(&self) -> Vec<&Ext> {
        self.rows
            .iter()
            .filter(|r| r.selected)
            .map(|r| &r.ext)
            .collect()
    }

    /// Passo 2: salva l'elenco (JSON) e una pagina HTML con i collegamenti.
    fn save_list(&mut self) {
        let exts = self.selected_exts();
        if exts.is_empty() {
            self.say(theme::LOG_WARN, "Nessuna estensione selezionata.".into());
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .set_title("Salva l'elenco delle estensioni")
            .set_file_name("Estensioni.extdesk.json")
            .add_filter("Elenco ExtDesk", &["json"])
            .save_file()
        else {
            return;
        };
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
        let pc = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "questo PC".into());
        let list = ExtList::from_exts(&exts, &pc, &now);
        let n = list.extensions.len();
        let html = install::html_page(&list);
        let res = list.save(&path).and_then(|_| {
            std::fs::write(path.with_extension("html"), html)
                .map_err(|e| format!("pagina HTML: {e}"))
        });
        match res {
            Ok(()) => self.say(theme::LOG_OK, format!("Elenco di {n} estensioni salvato in {} (più la pagina .html con i collegamenti).", path.display())),
            Err(e) => self.say(theme::LOG_ERR, format!("Errore: {e}")),
        }
    }

    fn load_list(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Apri un elenco di estensioni")
            .add_filter("Elenco ExtDesk", &["json"])
            .pick_file()
        else {
            return;
        };
        match ExtList::load(&path) {
            Err(e) => self.say(theme::LOG_ERR, e),
            Ok(list) => {
                let have: BTreeSet<String> = self.local().into_iter().map(|e| e.id).collect();
                self.rows = list
                    .extensions
                    .into_iter()
                    .map(|e| {
                        let present = have.contains(&e.id);
                        Row {
                            ext: Ext {
                                id: e.id,
                                name: e.name,
                                version: e.version,
                                profile: "Elenco".into(),
                                store: true,
                                theme: false,
                            },
                            selected: !present,
                            present,
                            imported: true,
                        }
                    })
                    .collect();
                self.opened_batches = 0;
                let n = self.rows.iter().filter(|r| !r.present).count();
                self.say(
                    theme::LOG_OK,
                    format!(
                        "Elenco caricato da {}: {n} da installare, le altre ci sono già.",
                        path.display()
                    ),
                );
            }
        }
    }

    /// Id da installare: selezionati e non ancora presenti.
    fn to_install(&self) -> Vec<String> {
        let mut seen = BTreeSet::new();
        self.rows
            .iter()
            .filter(|r| r.selected && !r.present && seen.insert(r.ext.id.clone()))
            .map(|r| r.ext.id.clone())
            .collect()
    }

    fn open_next_batch(&mut self) {
        let batches = install::batches(&self.to_install(), BATCH);
        let Some(b) = batches.get(self.opened_batches) else {
            self.say(theme::LOG_OK, "Tutte le schede sono state aperte.".into());
            return;
        };
        match install::open_in_chrome(b) {
            Ok(()) => {
                self.opened_batches += 1;
                self.say(
                    theme::LOG_OK,
                    format!(
                        "Aperte {} schede del Web Store: premi «Aggiungi a Chrome» in ognuna.",
                        b.len()
                    ),
                );
            }
            Err(e) => self.say(theme::LOG_ERR, e),
        }
    }

    fn policy_files(&mut self, apply: bool) {
        let ids = self.to_install();
        if ids.is_empty() {
            self.say(theme::LOG_WARN, "Niente da installare.".into());
            return;
        }
        let res = std::fs::create_dir_all(&self.out_dir)
            .map_err(|e| format!("{}: {e}", self.out_dir.display()))
            .and_then(|_| {
                let add = self.out_dir.join("Installa-estensioni.reg");
                let del = self.out_dir.join("Rimuovi-installazione-forzata.reg");
                std::fs::write(&add, install::forcelist_reg(&ids)).map_err(|e| e.to_string())?;
                std::fs::write(&del, install::forcelist_remove_reg(&ids))
                    .map_err(|e| e.to_string())?;
                Ok(add)
            });
        match res {
            Err(e) => self.say(theme::LOG_ERR, format!("Errore: {e}")),
            Ok(add) => {
                self.say(
                    theme::LOG_OK,
                    format!(
                        "Creati i file .reg in {} ({} estensioni).",
                        self.out_dir.display(),
                        ids.len()
                    ),
                );
                if apply {
                    match install::import_reg_as_admin(&add) {
                        Ok(()) => self.say(
                            theme::LOG_INFO,
                            "Conferma la richiesta di amministratore, poi riavvia Chrome.".into(),
                        ),
                        Err(e) => self.say(theme::LOG_ERR, e),
                    }
                }
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

    fn table(&mut self, ui: &mut egui::Ui) {
        if self.rows.is_empty() {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Nessuna estensione").size(18.0).strong());
                ui.label(RichText::new("Premi «1 Estrai da Chrome», oppure carica un elenco salvato su un altro PC.").color(theme::MUTED));
            });
            return;
        }
        ui.horizontal(|ui| {
            ui.label("Filtra:");
            ui.text_edit_singleline(&mut self.filter);
            if ui.button("Seleziona tutte").clicked() {
                self.rows.iter_mut().for_each(|r| r.selected = true);
            }
            if ui.button("Nessuna").clicked() {
                self.rows.iter_mut().for_each(|r| r.selected = false);
            }
            let n = self.rows.iter().filter(|r| r.selected).count();
            ui.label(
                RichText::new(format!("{n} di {} selezionate", self.rows.len()))
                    .color(theme::MUTED),
            );
        });
        let f = self.filter.to_lowercase();
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("ext")
                    .striped(true)
                    .num_columns(5)
                    .spacing([14.0, 8.0])
                    .show(ui, |ui| {
                        for h in ["", "Estensione", "Versione", "Profilo", "Note"] {
                            ui.label(RichText::new(h).strong().color(theme::MUTED));
                        }
                        ui.end_row();
                        for r in self
                            .rows
                            .iter_mut()
                            .filter(|r| f.is_empty() || r.ext.name.to_lowercase().contains(&f))
                        {
                            ui.checkbox(&mut r.selected, "");
                            ui.label(RichText::new(&r.ext.name).strong());
                            ui.label(&r.ext.version);
                            ui.label(&r.ext.profile);
                            let note = if r.imported && r.present {
                                RichText::new("Già in Chrome").color(theme::OK)
                            } else if r.ext.theme {
                                RichText::new("Tema").color(theme::MUTED)
                            } else if !r.ext.store {
                                RichText::new("Non dal Web Store: non reinstallabile")
                                    .color(theme::WARN)
                            } else {
                                RichText::new("")
                            };
                            ui.label(note);
                            ui.end_row();
                        }
                    });
            });
    }

    fn install_window(&mut self, ctx: &egui::Context) {
        let mut open = self.install_open;
        let ids = self.to_install();
        egui::Window::new("Installa le estensioni")
            .open(&mut open)
            .default_width(520.0)
            .show(ctx, |ui| {
                ui.label(format!("{} estensioni da installare (selezionate e non ancora presenti).", ids.len()));
                ui.add_space(6.0);
                ui.radio_value(&mut self.mode, Mode::Store, "Web Store: apro le pagine, tu premi «Aggiungi a Chrome» (consigliato)");
                ui.radio_value(&mut self.mode, Mode::Policy, "Installazione forzata con criteri di Chrome (amministratore)");
                ui.separator();
                match self.mode {
                    Mode::Store => {
                        let total = install::batches(&ids, BATCH).len();
                        ui.label(RichText::new("Chrome non permette di installare estensioni di nascosto: ogni pagina va confermata. Le apro a gruppi di 8.").color(theme::MUTED));
                        let label = if self.opened_batches == 0 { "Apri le prime 8 schede".to_string() } else { format!("Apri le prossime 8 ({} di {total} fatti)", self.opened_batches) };
                        if Self::big_button(ui, &label, theme::DOWNLOAD, self.opened_batches < total) {
                            self.open_next_batch();
                        }
                    }
                    Mode::Policy => {
                        ui.label(RichText::new("Chrome installa da solo tutto al riavvio, ma le estensioni risultano «gestite dall'organizzazione» e non si possono rimuovere finché non togli i criteri (file «Rimuovi-installazione-forzata.reg»).").color(theme::WARN));
                        ui.horizontal(|ui| {
                            if Self::big_button(ui, "Applica ora (admin)", theme::INSTALL, !ids.is_empty()) {
                                self.policy_files(true);
                            }
                            if ui.button("Crea solo i file .reg").clicked() {
                                self.policy_files(false);
                            }
                        });
                    }
                }
            });
        self.install_open = open;
    }
}

impl ExtPanel {
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.horizontal_wrapped(|ui| {
            if Self::big_button(ui, "Estrai da Chrome", theme::SCAN, true) {
                self.extract();
            }
            if Self::big_button(ui, "Salva elenco", theme::DOWNLOAD, !self.rows.is_empty()) {
                self.save_list();
            }
            if Self::big_button(
                ui,
                "Installa estensioni",
                theme::INSTALL,
                !self.to_install().is_empty(),
            ) {
                self.install_open = true;
            }
            if ui.button("Carica elenco…").clicked() {
                self.load_list();
            }
            if ui.button("Cartella Chrome…").clicked() {
                self.choose_chrome_folder();
            }
        });
        ui.add_space(4.0);
        self.table(ui);
        if self.install_open {
            self.install_window(&ctx);
        }
    }
}
