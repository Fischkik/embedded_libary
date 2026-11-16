//! Renders the explanation panel of a demo.

use crate::{DemoEntry, glossary, theme};
use egui::{RichText, Ui};

/// All `[[term]]` marks in a text.
pub fn terms_in(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(a) = rest.find("[[") {
        let Some(len) = rest[a + 2..].find("]]") else { break };
        out.push(&rest[a + 2..a + 2 + len]);
        rest = &rest[a + 4 + len..];
    }
    out
}

/// Draw text with paragraphs (blank line) and glossary terms (`[[term]]`).
pub fn rich(ui: &mut Ui, text: &str) {
    for para in text.split("\n\n") {
        let para = para.replace('\n', " ");
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let mut rest = para.as_str();
            while let Some(a) = rest.find("[[") {
                let Some(len) = rest[a + 2..].find("]]") else { break };
                if a > 0 {
                    ui.label(&rest[..a]);
                }
                let term = &rest[a + 2..a + 2 + len];
                match glossary::lookup(term) {
                    Some(def) => {
                        ui.label(RichText::new(term).underline()).on_hover_text(def);
                    }
                    None => {
                        ui.label(term);
                    }
                }
                rest = &rest[a + 4 + len..];
            }
            if !rest.is_empty() {
                ui.label(rest);
            }
        });
        ui.add_space(4.0);
    }
}

fn section(ui: &mut Ui, title: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(title).strong().size(16.0));
}

/// Draw the whole explanation. Returns the preset of a "Show me" button if one was clicked.
pub fn show(ui: &mut Ui, entry: &DemoEntry) -> Option<&'static str> {
    let pal = theme::palette(ui);
    let e = &entry.explain;
    let mut clicked = None;

    ui.heading(entry.title);
    ui.label(RichText::new(entry.blurb).italics().color(pal.dim));

    section(ui, "What you see");
    rich(ui, e.what);

    section(ui, "Why it works");
    rich(ui, e.why);

    if !e.formulas.is_empty() {
        section(ui, "In numbers");
        for (formula, words) in e.formulas {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label(RichText::new(*formula).monospace().strong());
                ui.label(*words);
            });
        }
    }

    section(ui, "Try this");
    for (i, x) in e.try_this.iter().enumerate() {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.label(RichText::new(format!("{}. ", i + 1)).strong());
                ui.label(x.action);
            });
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.label(RichText::new("Watch for: ").color(pal.good).strong());
                ui.label(x.watch);
            });
            if let Some(preset) = x.preset {
                if ui.small_button(format!("Show me  (experiment {})", i + 1)).clicked() {
                    clicked = Some(preset);
                }
            }
        });
    }

    section(ui, "What this leaves out");
    rich(ui, e.leaves_out);

    let mut terms: Vec<&str> = [e.what, e.why, e.leaves_out].iter().flat_map(|t| terms_in(t)).collect();
    terms.sort_by_key(|t| t.to_lowercase());
    terms.dedup_by_key(|t| t.to_lowercase());
    if !terms.is_empty() {
        section(ui, "Words used here");
        for term in terms {
            if let Some(def) = glossary::lookup(term) {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.label(RichText::new(format!("{term}: ")).strong());
                    ui.label(def);
                });
            }
        }
    }

    if !e.refs.is_empty() {
        section(ui, "Where this comes from");
        for r in e.refs {
            ui.label(RichText::new(format!("- {r}")).small());
        }
    }
    ui.add_space(12.0);
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_terms() {
        assert_eq!(terms_in("a [[carrier]] and a [[CRC]]."), ["carrier", "CRC"]);
        assert!(terms_in("no terms, [[unclosed").is_empty());
    }
}
