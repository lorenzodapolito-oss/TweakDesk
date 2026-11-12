#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod catalog;
mod clean;
mod debloat;
mod engine;
mod install;
mod liber;
mod model;
mod regfile;
mod registry;
mod theme;
mod tweak_panel;

/// Icona della finestra: 128x128 RGBA incorporata.
fn app_icon() -> eframe::egui::IconData {
    eframe::egui::IconData {
        rgba: include_bytes!("../assets/icon.rgba").to_vec(),
        width: 128,
        height: 128,
    }
}

/// Testo del rapporto d'errore salvato accanto al programma e mostrato in una finestra.
fn error_report(what: &str, detail: &str) -> String {
    format!(
        "TweakDesk v{}\n{what}\n\n{detail}\n\nSe il problema persiste, manda questo testo a chi ti assiste.",
        app::APP_VERSION
    )
}

/// Un avvio fallito (finestra non creata, grafica non disponibile, errore imprevisto) non deve
/// sparire in silenzio: il programma non ha console, quindi si scrive un file e si mostra un avviso.
fn report_fatal(text: &str) {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let _ = std::fs::write(dir.join("TweakDesk-errore.txt"), text);
        }
    }
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("TweakDesk non riesce ad avviarsi")
        .set_description(text)
        .show();
}

/// Opzioni della finestra con il motore grafico scelto.
fn native_options(renderer: eframe::Renderer) -> eframe::NativeOptions {
    eframe::NativeOptions {
        renderer,
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(format!(
                "{} di Lorenzo D'Apolito v{}",
                app::APP_TITLE,
                app::APP_VERSION
            ))
            .with_inner_size([1200.0, 760.0])
            .with_min_inner_size([800.0, 480.0])
            .with_icon(app_icon()),
        ..Default::default()
    }
}

/// Prima DirectX/Vulkan (wgpu: funziona anche senza driver video, con il rendering software
/// di Windows), poi OpenGL. Così il programma parte anche su un Windows appena installato.
fn run(open: Option<std::path::PathBuf>) -> eframe::Result {
    let mut last = Ok(());
    for renderer in [eframe::Renderer::Wgpu, eframe::Renderer::Glow] {
        let open = open.clone();
        last = eframe::run_native(
            app::APP_TITLE,
            native_options(renderer),
            Box::new(move |cc| Ok(Box::new(app::App::new(cc, open.clone())))),
        );
        if last.is_ok() {
            break;
        }
    }
    last
}

fn main() {
    // Un errore imprevisto (panic) viene scritto in un file invece di chiudere tutto senza dire nulla.
    std::panic::set_hook(Box::new(|info| {
        report_fatal(&error_report("Errore imprevisto.", &info.to_string()));
    }));
    // Un file passato come argomento (doppio clic o trascinamento sull'exe) viene aperto subito.
    let open = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    if let Err(e) = run(open) {
        report_fatal(&error_report(
            "La finestra non si apre: non è disponibile né DirectX/Vulkan né OpenGL. Installa i driver video scaricandoli dal sito del produttore del PC o della scheda grafica (non serve Windows Update), poi riprova.",
            &e.to_string(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_report_has_version_and_details() {
        let t = error_report("Errore.", "dettaglio X");
        assert!(t.contains(app::APP_VERSION));
        assert!(t.contains("Errore.") && t.contains("dettaglio X"));
    }
}
