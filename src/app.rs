//! Finestra principale di TweakDesk: impostazioni di Windows e installazione.

use crate::clean::panel::CleanPanel;
use crate::install::app::InstallPanel;
use crate::theme;
use crate::tweak_panel::TweakPanel;
use eframe::egui::{self, RichText};
use std::path::PathBuf;

pub const APP_TITLE: &str = "TweakDesk";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Copy, PartialEq)]
enum Section {
    Tweaks,
    Install,
    Clean,
}

pub struct App {
    section: Section,
    tweak: TweakPanel,
    install: InstallPanel,
    clean: CleanPanel,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, open: Option<PathBuf>) -> Self {
        theme::apply(&cc.egui_ctx);
        Self {
            section: Section::Tweaks,
            tweak: TweakPanel::new(open),
            install: InstallPanel::new(),
            clean: CleanPanel::new(),
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("sections").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(APP_TITLE)
                        .size(24.0)
                        .strong()
                        .color(theme::PRIMARY),
                );
                ui.label(RichText::new(format!("v{APP_VERSION}")).color(theme::MUTED));
                ui.add_space(16.0);
                ui.selectable_value(
                    &mut self.section,
                    Section::Tweaks,
                    RichText::new("Impostazioni di Windows").size(16.0),
                );
                ui.selectable_value(
                    &mut self.section,
                    Section::Install,
                    RichText::new("Programmi, estensioni e script").size(16.0),
                );
                ui.selectable_value(
                    &mut self.section,
                    Section::Clean,
                    RichText::new("Pulizia e avvio").size(16.0),
                );
            });
            ui.add_space(4.0);
        });
        match self.section {
            Section::Tweaks => self.tweak.show(ui),
            Section::Install => self.install.show(ui),
            Section::Clean => self.clean.show(ui),
        }
    }
}
