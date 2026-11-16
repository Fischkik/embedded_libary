//! Small shared controls: unit-aware sliders, a simulation clock, titled groups.

use egui::{RichText, Ui};
use std::ops::RangeInclusive;

/// Parse text like `"2.4 GHz"`, `"250k"` or `"12.5m"` into a plain number.
pub fn parse_si(text: &str) -> Option<f64> {
    let t = text.trim();
    let split = t.find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E'))).unwrap_or(t.len());
    // Guard against treating an exponent-less trailing 'e' as part of the number.
    let (num, rest) = t.split_at(split);
    let value: f64 = num.trim().parse().ok()?;
    let scale = match rest.trim().chars().next() {
        Some('T') => 1e12,
        Some('G') => 1e9,
        Some('M') => 1e6,
        Some('k') | Some('K') => 1e3,
        Some('m') => 1e-3,
        Some('u') | Some('µ') => 1e-6,
        Some('n') => 1e-9,
        Some('p') => 1e-12,
        _ => 1.0,
    };
    Some(value * scale)
}

/// A slider that shows its value with an SI prefix and unit, e.g. "2.402 GHz".
/// Chain `.logarithmic(true)` for values that span several orders of magnitude.
pub fn si_slider<'a>(value: &'a mut f64, range: RangeInclusive<f64>, label: &'a str, unit: &'static str) -> egui::Slider<'a> {
    egui::Slider::new(value, range)
        .text(label)
        .custom_formatter(move |v, _| sim_core::units::si(v, unit))
        .custom_parser(parse_si)
}

/// A group box with a small title.
pub fn group<R>(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::group(ui.style())
        .show(ui, |ui| {
            ui.label(RichText::new(title).strong());
            add(ui)
        })
        .inner
}

/// A formula with the current numbers filled in, set apart in monospace.
pub fn formula(ui: &mut Ui, text: impl Into<String>) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.label(RichText::new(text.into()).monospace());
    });
}

/// "label  value" on one line, the value emphasised.
pub fn readout(ui: &mut Ui, label: &str, value: impl Into<String>) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.label(RichText::new(value.into()).strong().monospace());
    });
}

/// A coloured status line, e.g. "CRC OK" in green.
pub fn status(ui: &mut Ui, ok: bool, text: impl Into<String>) {
    let pal = crate::theme::palette(ui);
    let (mark, color) = if ok { ("OK", pal.good) } else { ("FAIL", pal.bad) };
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(mark).strong().color(color));
        ui.label(RichText::new(text.into()).color(color));
    });
}

/// Play / pause / speed for an animated simulation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clock {
    pub running: bool,
    /// Simulated seconds per real second. What "1x" means is up to the demo.
    pub speed: f64,
    /// Simulated time, in the demo's own unit.
    pub t: f64,
}

impl Default for Clock {
    fn default() -> Self {
        Self { running: true, speed: 1.0, t: 0.0 }
    }
}

impl Clock {
    /// Advance by real time `dt` and keep the screen repainting while running.
    /// Returns the simulated time that passed.
    pub fn tick(&mut self, ui: &Ui, dt: f64) -> f64 {
        if !self.running {
            return 0.0;
        }
        ui.ctx().request_repaint();
        let step = dt * self.speed;
        self.t += step;
        step
    }

    /// Draw the controls. Returns true if "Restart" was clicked.
    pub fn controls(&mut self, ui: &mut Ui) -> bool {
        let mut restart = false;
        ui.horizontal_wrapped(|ui| {
            let label = if self.running { "Pause" } else { "Play" };
            if ui.button(label).on_hover_text("Stop or continue the animation").clicked() {
                self.running = !self.running;
            }
            if ui.button("Restart").on_hover_text("Start the animation again from time zero").clicked() {
                self.t = 0.0;
                restart = true;
            }
            ui.add(egui::Slider::new(&mut self.speed, 0.1..=4.0).logarithmic(true).text("speed").fixed_decimals(1).suffix("x"))
                .on_hover_text("How fast the animation runs. This only changes how fast you watch, not the physics.");
        });
        restart
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_si_text() {
        assert_eq!(parse_si("2.4 GHz"), Some(2.4e9));
        assert_eq!(parse_si("250k"), Some(250e3));
        assert_eq!(parse_si("12.5 mm"), Some(12.5e-3));
        assert_eq!(parse_si("-3"), Some(-3.0));
        assert_eq!(parse_si("7 Hz"), Some(7.0));
        assert_eq!(parse_si("abc"), None);
    }
}
