//! Scheda «Configurazione Windows»: lancia gli script di configurazione.

use crate::install::setup::{self, Impact, SCRIPTS};
use crate::theme;
use eframe::egui::{self, Color32, RichText};
use std::path::PathBuf;

pub struct SetupPanel {
    dir: PathBuf,
    admin: bool,
    confirm: Option<usize>,
    /// Messaggi da mostrare nel registro dell'app.
    pub msgs: Vec<(Color32, String)>,
}

impl SetupPanel {
    pub fn new(app_dir: &std::path::Path) -> Self {
        Self {
            dir: app_dir.join("Script"),
            admin: setup::is_admin(),
            confirm: None,
            msgs: Vec::new(),
        }
    }

    fn say(&mut self, c: Color32, m: String) {
        self.msgs.push((c, m));
    }

    fn run(&mut self, i: usize) {
        let Some(s) = SCRIPTS.get(i) else { return };
        let res = setup::prepare(s, &self.dir)
            .and_then(|p| setup::launch(s.kind, &p, self.admin).map(|_| p));
        match res {
            Ok(_) => self.say(
                theme::LOG_OK,
                format!("{}: avviato in una finestra a parte.", s.label),
            ),
            Err(e) => self.say(theme::LOG_ERR, format!("{}: {e}", s.label)),
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let note = if self.admin {
            "InstallDesk è in esecuzione come amministratore: gli script partono senza altre richieste."
        } else {
            "Ogni script chiede l'autorizzazione di amministratore. Per una sola richiesta, avvia InstallDesk come amministratore."
        };
        ui.label(RichText::new(note).color(theme::MUTED));
        ui.add_space(4.0);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut group = "";
                for (i, s) in SCRIPTS.iter().enumerate() {
                    if s.group != group {
                        group = s.group;
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(group)
                                .size(16.0)
                                .strong()
                                .color(theme::PRIMARY),
                        );
                    }
                    ui.horizontal(|ui| {
                        let color = if s.impact == Impact::High {
                            theme::INSTALL
                        } else {
                            theme::SCAN
                        };
                        let b = egui::Button::new(
                            RichText::new(s.label).strong().color(theme::ON_COLOR),
                        )
                        .fill(color)
                        .min_size(egui::vec2(280.0, 30.0));
                        if ui.add(b).clicked() {
                            if s.impact == Impact::High {
                                self.confirm = Some(i);
                            } else {
                                self.run(i);
                            }
                        }
                        ui.label(RichText::new(s.desc).color(theme::MUTED));
                    });
                }
            });
        if let Some(i) = self.confirm {
            let Some(s) = SCRIPTS.get(i) else {
                self.confirm = None;
                return;
            };
            egui::Window::new("Conferma")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    ui.label(RichText::new(s.label).strong());
                    ui.label(s.desc);
                    ui.label(
                        RichText::new("Ha un impatto rilevante sul sistema e non sempre si annulla. Continuare?")
                            .color(theme::WARN),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Esegui").clicked() {
                            self.confirm = None;
                            self.run(i);
                        }
                        if ui.button("Annulla").clicked() {
                            self.confirm = None;
                        }
                    });
                });
        }
    }
}
