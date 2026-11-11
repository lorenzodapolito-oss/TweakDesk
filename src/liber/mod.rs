//! Gestione delle app portabili (cartella `Apps`) e dei programmi non portabili (cartella
//! `Installers`) con aggiornamenti automatici: l'ex LiberDesk, ora scheda «Programmi».

pub mod archive;
pub mod catalog;
pub mod config;
pub mod describe;
pub mod github;
pub mod inst_ui;
pub mod inst_update;
pub mod installers;
pub mod net;
pub mod panel;
pub mod scan;
pub mod search;
pub mod state;
pub mod trash;
pub mod update;
pub mod version;
pub mod winget;
