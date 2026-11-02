//! Palette verde acqua e stile dell'interfaccia.

use eframe::egui::{self, Color32, CornerRadius, Stroke};

pub const BG: Color32 = Color32::from_rgb(0xF3, 0xF7, 0xF7);
pub const SURFACE: Color32 = Color32::WHITE;
pub const PRIMARY: Color32 = Color32::from_rgb(0x1F, 0x8A, 0x8A);
pub const PRIMARY_LIGHT: Color32 = Color32::from_rgb(0xE3, 0xF3, 0xF2);
pub const TEXT: Color32 = Color32::from_rgb(0x22, 0x3B, 0x3F);
pub const MUTED: Color32 = Color32::from_rgb(0x66, 0x7C, 0x80);
pub const BORDER: Color32 = Color32::from_rgb(0xD8, 0xE3, 0xE4);
pub const OK: Color32 = Color32::from_rgb(0x2E, 0x9E, 0x6B);
pub const WARN: Color32 = Color32::from_rgb(0xC7, 0x7D, 0x0A);
pub const ERR: Color32 = Color32::from_rgb(0xD0, 0x45, 0x45);
pub const LOG_BG: Color32 = Color32::from_rgb(0x1B, 0x2B, 0x2E);
pub const LOG_INFO: Color32 = Color32::from_rgb(0x8F, 0xD3, 0xD3);
pub const LOG_OK: Color32 = Color32::from_rgb(0x6B, 0xCB, 0x97);
pub const LOG_WARN: Color32 = Color32::from_rgb(0xFF, 0xD9, 0x7A);
pub const LOG_ERR: Color32 = Color32::from_rgb(0xFF, 0x8A, 0x8A);

pub fn apply(ctx: &egui::Context) {
    let mut v = egui::Visuals::light();
    v.panel_fill = BG;
    v.window_fill = SURFACE;
    v.extreme_bg_color = SURFACE;
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = PRIMARY_LIGHT;
    v.selection.stroke = Stroke::new(1.0, PRIMARY);
    v.hyperlink_color = PRIMARY;
    let r = CornerRadius::same(6);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
    ] {
        w.corner_radius = r;
    }
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, PRIMARY);
    v.widgets.hovered.weak_bg_fill = PRIMARY_LIGHT;
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(12.0, 6.0);
    });
}
