//! Palette pastello (niente bianco) e stile dell'interfaccia.

use eframe::egui::{self, Color32, CornerRadius, Stroke};

/// Sfondo: celeste pastello.
pub const BG: Color32 = Color32::from_rgb(0xC4, 0xE6, 0xF9);
/// Pannelli, finestre e campi di testo: celeste un po' più chiaro (mai bianco).
pub const SURFACE: Color32 = Color32::from_rgb(0xD3, 0xEE, 0xFB);
/// Riga alternata delle tabelle: lilla.
pub const STRIPE: Color32 = Color32::from_rgb(0xBA, 0xDE, 0xF4);
pub const PRIMARY: Color32 = Color32::from_rgb(0x4F, 0x7C, 0xD6);
pub const PRIMARY_LIGHT: Color32 = Color32::from_rgb(0xAC, 0xCF, 0xF1);
/// Testo: blu notte.
pub const TEXT: Color32 = Color32::from_rgb(0x22, 0x31, 0x5E);
/// Testo secondario: blu polvere.
pub const MUTED: Color32 = Color32::from_rgb(0x58, 0x68, 0x98);
pub const BORDER: Color32 = Color32::from_rgb(0x8F, 0xBC, 0xE0);
pub const OK: Color32 = Color32::from_rgb(0x1F, 0x86, 0x5C);
pub const WARN: Color32 = Color32::from_rgb(0xB0, 0x62, 0x18);
pub const ERR: Color32 = Color32::from_rgb(0xBC, 0x3C, 0x58);
/// Pulsanti principali: blu, menta e pesca pastello.
pub const SCAN: Color32 = Color32::from_rgb(0x9B, 0xBF, 0xF4);
pub const DOWNLOAD: Color32 = Color32::from_rgb(0x9C, 0xDD, 0xC1);
pub const INSTALL: Color32 = Color32::from_rgb(0xF6, 0xBF, 0x9C);
/// Colori delle schede in alto: pastello, tutti diversi e mai bianchi.
pub const TAB_BLUE: Color32 = Color32::from_rgb(0x9B, 0xBF, 0xF4);
pub const TAB_MINT: Color32 = Color32::from_rgb(0x9C, 0xDD, 0xC1);
pub const TAB_YELLOW: Color32 = Color32::from_rgb(0xF3, 0xE0, 0x9A);
pub const TAB_PEACH: Color32 = Color32::from_rgb(0xF6, 0xBF, 0x9C);
pub const TAB_LILAC: Color32 = Color32::from_rgb(0xCF, 0xBF, 0xF0);
pub const TAB_PINK: Color32 = Color32::from_rgb(0xF2, 0xBB, 0xD0);
/// Testo sui pulsanti pastello: blu notte.
pub const ON_COLOR: Color32 = Color32::from_rgb(0x1B, 0x28, 0x55);
pub const LOG_BG: Color32 = Color32::from_rgb(0x34, 0x41, 0x72);
pub const LOG_INFO: Color32 = Color32::from_rgb(0xB5, 0xD2, 0xFF);
pub const LOG_OK: Color32 = Color32::from_rgb(0xA8, 0xEC, 0xCC);
pub const LOG_WARN: Color32 = Color32::from_rgb(0xFF, 0xE2, 0xA0);
pub const LOG_ERR: Color32 = Color32::from_rgb(0xFF, 0xB4, 0xC2);

/// Colori dell'interfaccia: mai bianco né grigio, nemmeno nei dettagli (testo, ombre, bordi).
pub fn visuals() -> egui::Visuals {
    let mut v = egui::Visuals::light();
    v.panel_fill = BG;
    v.window_fill = SURFACE;
    v.extreme_bg_color = SURFACE;
    v.faint_bg_color = STRIPE;
    v.code_bg_color = SURFACE;
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = PRIMARY_LIGHT;
    v.selection.stroke = Stroke::new(1.0, PRIMARY);
    v.hyperlink_color = PRIMARY;
    let r = CornerRadius::same(9);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = r;
    }
    v.widgets.noninteractive.bg_fill = SURFACE;
    v.widgets.noninteractive.weak_bg_fill = SURFACE;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.inactive.bg_fill = PRIMARY_LIGHT;
    v.widgets.inactive.weak_bg_fill = PRIMARY_LIGHT;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.hovered.bg_fill = Color32::from_rgb(0xB4, 0xC8, 0xF2);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0xB4, 0xC8, 0xF2);
    v.widgets.hovered.bg_stroke = Stroke::new(1.5, PRIMARY);
    v.widgets.active.bg_fill = Color32::from_rgb(0xA3, 0xBB, 0xEE);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(0xA3, 0xBB, 0xEE);
    v.widgets.open.bg_fill = PRIMARY_LIGHT;
    v.widgets.open.weak_bg_fill = PRIMARY_LIGHT;
    // Sempre chiaro, qualunque sia il tema di Windows (altrimenti egui ripiega sui suoi colori).
    harmonize(&mut v);
    v
}

/// Sostituisce i grigi e i bianchi di egui (testo dei pulsanti, ombre, cursore, bordi).
fn harmonize(v: &mut egui::Visuals) {
    v.weak_text_color = Some(MUTED);
    v.code_bg_color = SURFACE;
    v.faint_bg_color = STRIPE;
    v.extreme_bg_color = SURFACE;
    v.window_stroke = Stroke::new(1.0, BORDER);
    let shadow = Color32::from_rgba_unmultiplied(0x2B, 0x4A, 0x7A, 60);
    v.window_shadow.color = shadow;
    v.popup_shadow.color = shadow;
    v.text_cursor.stroke.color = PRIMARY;
    v.widgets.noninteractive.weak_bg_fill = SURFACE;
    v.widgets.open.bg_fill = PRIMARY_LIGHT;
    v.widgets.open.weak_bg_fill = PRIMARY_LIGHT;
    v.widgets.open.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.hovered.bg_fill = v.widgets.hovered.weak_bg_fill;
    v.widgets.active.bg_fill = v.widgets.active.weak_bg_fill;
    v.widgets.active.bg_stroke = Stroke::new(1.5, PRIMARY);
    v.text_edit_bg_color = Some(SURFACE);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.fg_stroke.color = TEXT;
    }
}

pub fn apply(ctx: &egui::Context) {
    let v = visuals();
    ctx.set_theme(egui::ThemePreference::Light);
    ctx.set_visuals_of(egui::Theme::Light, v.clone());
    ctx.set_visuals_of(egui::Theme::Dark, v);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(12.0, 6.0);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn neutral(c: Color32) -> bool {
        let [r, g, b, _] = c.to_srgba_unmultiplied().map(i32::from);
        r.max(g).max(b) - r.min(g).min(b) < 24
    }

    #[test]
    fn no_white_or_gray_anywhere_in_the_visuals() {
        let v = visuals();
        let mut named: Vec<(&str, Color32)> = vec![
            ("panel", v.panel_fill),
            ("window", v.window_fill),
            ("faint", v.faint_bg_color),
            ("extreme", v.extreme_bg_color),
            ("code", v.code_bg_color),
            ("window_stroke", v.window_stroke.color),
            ("window_shadow", v.window_shadow.color),
            ("popup_shadow", v.popup_shadow.color),
            ("selection", v.selection.bg_fill),
            ("selection_stroke", v.selection.stroke.color),
            ("hyperlink", v.hyperlink_color),
            ("cursor", v.text_cursor.stroke.color),
            ("warn", v.warn_fg_color),
            ("error", v.error_fg_color),
        ];
        if let Some(c) = v.override_text_color {
            named.push(("text", c));
        }
        if let Some(c) = v.weak_text_color {
            named.push(("weak_text", c));
        }
        if let Some(c) = v.text_edit_bg_color {
            named.push(("text_edit", c));
        }
        for (n, w) in [
            ("noninteractive", v.widgets.noninteractive),
            ("inactive", v.widgets.inactive),
            ("hovered", v.widgets.hovered),
            ("active", v.widgets.active),
            ("open", v.widgets.open),
        ] {
            named.push((n, w.bg_fill));
            named.push((n, w.weak_bg_fill));
            named.push((n, w.bg_stroke.color));
            named.push((n, w.fg_stroke.color));
        }
        for (n, c) in named {
            assert!(!neutral(c), "{n}: {c:?} è bianco o grigio");
        }
    }
}
