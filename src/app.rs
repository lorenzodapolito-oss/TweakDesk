//! Finestra principale di TweakDesk: schede colorate in alto.

use crate::clean::panel::{CleanPanel, Tab as CleanTab};
use crate::debloat::panel::DebloatPanel;
use crate::install::app::{InstallPanel, Tab as InstallTab};
use crate::liber::panel::LiberPanel;
use crate::theme;
use crate::tweak_panel::TweakPanel;
use eframe::egui::{self, Color32, RichText};
use std::path::PathBuf;

pub const APP_TITLE: &str = "TweakDesk";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Section {
    Settings,
    Programs,
    Extensions,
    Clean,
    Backup,
    Config,
    Debloat,
}

/// Scheda: titolo e colore pastello (sempre colorata, non solo quando è scelta).
pub const SECTIONS: [(Section, &str, Color32); 7] = [
    (Section::Settings, "Impostazioni", theme::TAB_BLUE),
    (Section::Programs, "Programmi", theme::TAB_MINT),
    (Section::Extensions, "Estensioni Chrome", theme::TAB_YELLOW),
    (Section::Clean, "Pulizia e avvio", theme::TAB_PEACH),
    (Section::Backup, "Backup di sistema", theme::TAB_LILAC),
    (
        Section::Config,
        "Configurazione di Windows",
        theme::TAB_AQUA,
    ),
    (
        Section::Debloat,
        "Debloat e ottimizzazione",
        theme::TAB_PINK,
    ),
];

pub struct App {
    section: Section,
    tweak: TweakPanel,
    install: InstallPanel,
    liber: LiberPanel,
    clean: CleanPanel,
    debloat: DebloatPanel,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, open: Option<PathBuf>) -> Self {
        theme::apply(&cc.egui_ctx);
        Self {
            section: Section::Settings,
            tweak: TweakPanel::new(open),
            install: InstallPanel::new(),
            liber: LiberPanel::new(),
            clean: CleanPanel::new(),
            debloat: DebloatPanel::new(),
        }
    }

    fn select(&mut self, s: Section) {
        self.section = s;
        match s {
            Section::Extensions => self.install.set_tab(InstallTab::Ext),
            Section::Config => self.install.set_tab(InstallTab::Setup),
            Section::Clean => self.clean.set_tab(CleanTab::Clean),
            Section::Backup => self.clean.set_tab(CleanTab::Backup),
            Section::Settings | Section::Programs | Section::Debloat => {}
        }
    }
}

/// Un pulsante-scheda colorato; quella scelta ha il bordo scuro e il testo in grassetto.
fn tab_button(ui: &mut egui::Ui, text: &str, color: Color32, selected: bool) -> bool {
    let rich = if selected {
        RichText::new(text).size(15.5).strong()
    } else {
        RichText::new(text).size(15.5)
    };
    let stroke = if selected {
        egui::Stroke::new(2.5, theme::TEXT)
    } else {
        egui::Stroke::new(1.0, theme::BORDER)
    };
    let b = egui::Button::new(rich.color(theme::ON_COLOR))
        .fill(color)
        .stroke(stroke)
        .min_size(egui::vec2(0.0, 34.0));
    ui.add(b).clicked()
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("sections").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{APP_TITLE} di Lorenzo D'Apolito"))
                        .size(24.0)
                        .strong()
                        .color(theme::PRIMARY),
                );
                ui.label(RichText::new(format!("v{APP_VERSION}")).color(theme::MUTED));
            });
            ui.horizontal_wrapped(|ui| {
                for (s, title, color) in SECTIONS {
                    if tab_button(ui, title, color, self.section == s) {
                        self.select(s);
                    }
                }
            });
            ui.add_space(4.0);
        });
        match self.section {
            Section::Settings => self.tweak.show(ui),
            Section::Programs => self.liber.show(ui),
            Section::Extensions | Section::Config => self.install.show(ui),
            Section::Clean | Section::Backup => self.clean.show(ui),
            Section::Debloat => self.debloat.show(ui),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn sections_are_unique_and_colored_differently() {
        let sections: HashSet<_> = SECTIONS.iter().map(|s| s.0).collect();
        assert_eq!(sections.len(), SECTIONS.len());
        let colors: HashSet<_> = SECTIONS.iter().map(|s| s.2).collect();
        assert_eq!(colors.len(), SECTIONS.len());
        assert!(SECTIONS.iter().all(|s| s.2 != Color32::WHITE));
    }
}
