//! Colours and text sizes shared by all demos, for light and dark mode.
//!
//! The trace colours follow the Okabe-Ito palette, which stays distinguishable for
//! people with the common forms of colour blindness.

use egui::Color32;

#[derive(Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    /// Colours for plot traces and things that must be told apart.
    pub trace: [Color32; 6],
    pub good: Color32,
    pub bad: Color32,
    pub warn: Color32,
    pub dim: Color32,
    pub text: Color32,
    pub canvas: Color32,
    pub grid: Color32,
}

pub fn palette(ui: &egui::Ui) -> Palette {
    let v = ui.visuals();
    if v.dark_mode {
        Palette {
            dark: true,
            trace: [
                Color32::from_rgb(0x56, 0xB4, 0xE9),
                Color32::from_rgb(0xE6, 0x9F, 0x00),
                Color32::from_rgb(0x2F, 0xC7, 0x9A),
                Color32::from_rgb(0xF0, 0x6A, 0x3A),
                Color32::from_rgb(0xD8, 0x8F, 0xC0),
                Color32::from_rgb(0xF0, 0xE4, 0x42),
            ],
            good: Color32::from_rgb(0x2F, 0xC7, 0x9A),
            bad: Color32::from_rgb(0xF0, 0x6A, 0x3A),
            warn: Color32::from_rgb(0xE6, 0x9F, 0x00),
            dim: Color32::from_gray(120),
            text: v.text_color(),
            canvas: Color32::from_rgb(0x12, 0x15, 0x1B),
            grid: Color32::from_gray(48),
        }
    } else {
        Palette {
            dark: false,
            trace: [
                Color32::from_rgb(0x00, 0x72, 0xB2),
                Color32::from_rgb(0xB8, 0x7A, 0x00),
                Color32::from_rgb(0x00, 0x8A, 0x66),
                Color32::from_rgb(0xD5, 0x5E, 0x00),
                Color32::from_rgb(0xA8, 0x4F, 0x8A),
                Color32::from_rgb(0x7A, 0x70, 0x00),
            ],
            good: Color32::from_rgb(0x00, 0x8A, 0x66),
            bad: Color32::from_rgb(0xD5, 0x5E, 0x00),
            warn: Color32::from_rgb(0xB8, 0x7A, 0x00),
            dim: Color32::from_gray(140),
            text: v.text_color(),
            canvas: Color32::from_rgb(0xF4, 0xF5, 0xF7),
            grid: Color32::from_gray(210),
        }
    }
}

/// Background colours for packet fields. Chosen to keep black or white text readable.
pub fn field_color(index: usize, dark: bool) -> Color32 {
    const DARK: [Color32; 8] = [
        Color32::from_rgb(0x1F, 0x5F, 0x8B),
        Color32::from_rgb(0x8A, 0x5A, 0x00),
        Color32::from_rgb(0x12, 0x6E, 0x55),
        Color32::from_rgb(0x8F, 0x3A, 0x1C),
        Color32::from_rgb(0x7A, 0x3F, 0x6A),
        Color32::from_rgb(0x55, 0x5A, 0x12),
        Color32::from_rgb(0x3A, 0x48, 0x8F),
        Color32::from_rgb(0x5A, 0x5A, 0x5A),
    ];
    const LIGHT: [Color32; 8] = [
        Color32::from_rgb(0xA9, 0xD6, 0xF2),
        Color32::from_rgb(0xF5, 0xD4, 0x8A),
        Color32::from_rgb(0x9F, 0xE0, 0xCC),
        Color32::from_rgb(0xF4, 0xB3, 0x9A),
        Color32::from_rgb(0xE6, 0xBD, 0xDA),
        Color32::from_rgb(0xE4, 0xE0, 0x8C),
        Color32::from_rgb(0xBF, 0xC8, 0xF2),
        Color32::from_rgb(0xD4, 0xD4, 0xD4),
    ];
    if dark { DARK[index % 8] } else { LIGHT[index % 8] }
}

/// Apply the app-wide text sizes and spacing. Slightly larger than egui's defaults:
/// this is a tool for reading and thinking, not for packing in controls.
pub fn install(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        use egui::{FontFamily::Monospace, FontFamily::Proportional, FontId, TextStyle};
        style.text_styles = [
            (TextStyle::Heading, FontId::new(21.0, Proportional)),
            (TextStyle::Body, FontId::new(15.0, Proportional)),
            (TextStyle::Button, FontId::new(15.0, Proportional)),
            (TextStyle::Monospace, FontId::new(14.0, Monospace)),
            (TextStyle::Small, FontId::new(12.5, Proportional)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.slider_width = 190.0;
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
    });
}
