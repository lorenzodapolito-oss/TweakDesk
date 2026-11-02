#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod catalog;
mod engine;
mod model;
mod regfile;
mod registry;
mod theme;

fn main() -> eframe::Result {
    // Un file passato come argomento (doppio clic o trascinamento sull'exe) viene aperto subito.
    let open = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(format!("{} v{}", app::APP_TITLE, app::APP_VERSION))
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([800.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        app::APP_TITLE,
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, open.clone())))),
    )
}
