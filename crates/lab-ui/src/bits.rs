//! Showing and editing packets: a strip of clickable bits coloured by field,
//! a field legend, and a hex editor.

use crate::theme;
use egui::{Align2, Color32, FontId, Rect, RichText, Sense, StrokeKind, Ui, pos2, vec2};

/// A named run of bits in a packet.
#[derive(Clone, Debug)]
pub struct FieldSpan {
    pub name: String,
    pub bits: usize,
    pub help: String,
}

impl FieldSpan {
    pub fn new(name: &str, bits: usize, help: &str) -> Self {
        Self { name: name.to_owned(), bits, help: help.to_owned() }
    }
}

/// Which field (index into `fields`) a bit position belongs to.
pub fn field_of(fields: &[FieldSpan], bit: usize) -> Option<usize> {
    let mut start = 0;
    for (i, f) in fields.iter().enumerate() {
        if bit < start + f.bits {
            return Some(i);
        }
        start += f.bits;
    }
    None
}

#[derive(Default)]
pub struct StripResponse {
    /// A bit the user clicked this frame.
    pub clicked: Option<usize>,
    /// The field under the pointer.
    pub hovered: Option<usize>,
}

/// Draw bits as small cells, in the order given (for Bluetooth LE: the order on air),
/// coloured by field. Clicking a cell reports it so the caller can flip that bit.
pub fn bit_strip(ui: &mut Ui, bits: &[bool], fields: &[FieldSpan], highlight: Option<usize>) -> StripResponse {
    let pal = theme::palette(ui);
    let (cw, ch, gap, row_gap) = (11.0f32, 20.0f32, 5.0f32, 5.0f32);
    let byte_w = 8.0 * cw + gap;
    let bytes_per_row = ((ui.available_width() / byte_w).floor() as usize).clamp(2, 16);
    let per_row = bytes_per_row * 8;
    let rows = bits.len().div_ceil(per_row).max(1);
    let size = vec2(bytes_per_row as f32 * byte_w, rows as f32 * (ch + row_gap));
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter_at(rect);
    let cell = |i: usize| {
        let (row, col) = (i / per_row, i % per_row);
        let x = rect.left() + col as f32 * cw + (col / 8) as f32 * gap;
        let y = rect.top() + row as f32 * (ch + row_gap);
        Rect::from_min_size(pos2(x, y), vec2(cw - 1.0, ch))
    };
    let mut out = StripResponse::default();
    let pointer = response.hover_pos();
    for (i, &bit) in bits.iter().enumerate() {
        let r = cell(i);
        let field = field_of(fields, i);
        let base = field.map_or(pal.grid, |f| theme::field_color(f, pal.dark));
        let fill = if bit { base } else { base.gamma_multiply(0.55) };
        painter.rect_filled(r, 2.0, fill);
        painter.text(r.center(), Align2::CENTER_CENTER, if bit { "1" } else { "0" }, FontId::monospace(11.5), pal.text);
        if field.is_some() && field == highlight {
            painter.rect_stroke(r, 2.0, (1.5, pal.text), StrokeKind::Inside);
        }
        if pointer.is_some_and(|p| r.expand(0.5).contains(p)) {
            out.hovered = field;
            painter.rect_stroke(r, 2.0, (2.0, pal.warn), StrokeKind::Outside);
            if response.clicked() {
                out.clicked = Some(i);
            }
            let name = field.map_or("(outside any field)", |f| fields[f].name.as_str());
            response.clone().on_hover_text_at_pointer(format!("bit {i} (byte {}, bit {}) - {name}\nClick to flip it.", i / 8, i % 8));
        }
    }
    out
}

/// Coloured chips naming the fields. Click one to highlight it in the bit strip.
pub fn field_legend(ui: &mut Ui, fields: &[FieldSpan], selected: &mut Option<usize>) {
    let pal = theme::palette(ui);
    ui.horizontal_wrapped(|ui| {
        for (i, f) in fields.iter().enumerate() {
            let text = RichText::new(format!("{} ({} bits)", f.name, f.bits)).color(pal.text);
            let button = egui::Button::selectable(*selected == Some(i), text).fill(theme::field_color(i, pal.dark));
            if ui.add(button).on_hover_text(&f.help).clicked() {
                *selected = if *selected == Some(i) { None } else { Some(i) };
            }
        }
    });
}

/// A one-line hex editor. `text` is what the user types; when it parses, `bytes` is
/// updated and true is returned. When it does not parse, the error is shown and
/// `bytes` keeps its last good value.
pub fn hex_edit(ui: &mut Ui, label: &str, text: &mut String, bytes: &mut Vec<u8>, max_len: usize) -> bool {
    let pal = theme::palette(ui);
    let mut changed = false;
    let label_response = ui.label(label);
    let response = ui
        .add(egui::TextEdit::singleline(text).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY))
        .labelled_by(label_response.id);
    let parsed = sim_core::units::unhex(text);
    if response.changed()
        && let Some(b) = &parsed
            && b.len() <= max_len {
                *bytes = b.clone();
                changed = true;
            }
    match parsed {
        None => {
            ui.label(RichText::new("Not valid hex: use pairs of 0-9 and a-f, e.g. 02 01 06. Showing the last valid value.").color(pal.bad).small());
        }
        Some(b) if b.len() > max_len => {
            ui.label(RichText::new(format!("{} bytes is too long: at most {max_len} fit here.", b.len())).color(pal.bad).small());
        }
        _ => {}
    }
    changed
}

/// Render bytes as text like `02 01 06`.
pub fn hex_text(bytes: &[u8]) -> String {
    sim_core::units::hex(bytes)
}

/// A read-only row of hex bytes with a label, monospace.
pub fn hex_row(ui: &mut Ui, label: &str, bytes: &[u8], color: Option<Color32>) {
    ui.horizontal_wrapped(|ui| {
        ui.label(label);
        let mut text = RichText::new(hex_text(bytes)).monospace();
        if let Some(c) = color {
            text = text.color(c);
        }
        ui.label(text);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_bits_to_fields() {
        let f = [FieldSpan::new("a", 8, ""), FieldSpan::new("b", 32, "")];
        assert_eq!(field_of(&f, 0), Some(0));
        assert_eq!(field_of(&f, 7), Some(0));
        assert_eq!(field_of(&f, 8), Some(1));
        assert_eq!(field_of(&f, 39), Some(1));
        assert_eq!(field_of(&f, 40), None);
    }
}
