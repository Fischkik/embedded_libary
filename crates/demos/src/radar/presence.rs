use crate::prelude::*;
use egui_plot::{HLine, Polygon};
use sim_core::radar::{BREATHING_BAND_PER_MIN, PRESENCE_FS, PRESENCE_N, PRESENCE_THRESHOLD_DB, Presence, doppler_hz};

pub struct RadarPresence {
    p: Presence,
}

impl Default for RadarPresence {
    fn default() -> Self {
        Self { p: Presence { f0_hz: 24.125e9, person: true, breaths_per_min: 15.0, chest_mm: 4.0, beats_per_min: 72.0, heart_mm: 0.0, noise: 0.02 } }
    }
}

impl Demo for RadarPresence {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        let p = &mut self.p;
        widgets::group(ui, "Someone sits still in front of the radar", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut p.person, "someone is there").on_hover_text("Untick for an empty chair: the radar then sees only things that do not move.");
                ui.label("radar:");
                ui.selectable_value(&mut p.f0_hz, 24.125e9, "24 GHz").on_hover_text("A common frequency for motion and presence sensors. Wavelength 12.4 mm.");
                ui.selectable_value(&mut p.f0_hz, 60.0e9, "60 GHz").on_hover_text("Used by newer presence sensors. Wavelength 5 mm: the same movement turns the phase 2.5 times further.");
            });
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "rate", |ui| ui.add(egui::Slider::new(&mut p.breaths_per_min, 6.0..=30.0).text("breaths per minute").fixed_decimals(0)).on_hover_text("An adult at rest breathes about 12 to 20 times a minute."));
                widgets::wrap(ui, "chest", |ui| {
                    ui.add(egui::Slider::new(&mut p.chest_mm, 0.1..=12.0).logarithmic(true).text("chest moves").suffix(" mm").fixed_decimals(1))
                        .on_hover_text("How far the chest wall moves in and out with each breath, peak to peak. Typically 1 to 12 mm.")
                });
            });
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "heart", |ui| ui.add(egui::Slider::new(&mut p.heart_mm, 0.0..=0.5).text("heartbeat moves").suffix(" mm").fixed_decimals(2)).on_hover_text("The heartbeat moves the chest wall too, by only 0.1 to 0.5 mm."));
                widgets::wrap(ui, "noise", |ui| ui.add(egui::Slider::new(&mut p.noise, 0.005..=0.3).logarithmic(true).text("receiver noise").fixed_decimals(3)).on_hover_text("Noise compared with the echo. Farther away or behind a wall, the echo is weaker and the noise counts more."));
            });
        });
        let p = self.p;
        let tr = p.simulate(1);
        let lambda = C / p.f0_hz;
        let span = PRESENCE_N as f64 / PRESENCE_FS;

        ui.label(RichText::new("The chest, and the phase of its echo, over 32 seconds").strong());
        plots::plot_bare("chest", 120.0).x_axis_label("time [s]").y_axis_label("chest [mm]").include_x(0.0).include_x(span).include_y(-1.0).include_y(1.0).show(ui, |pl| {
            pl.line(plots::line("chest", tr.t.iter().zip(&tr.displacement_mm).map(|(t, x)| [*t, *x]).collect(), pal.trace[3]).width(2.0));
        });
        plots::plot_bare("phase", 150.0).x_axis_label("time [s]").y_axis_label("phase [rad]").include_x(0.0).include_x(span).include_y(-1.0).include_y(1.0).show(ui, |pl| {
            pl.line(plots::line("phase", tr.t.iter().zip(&tr.phase_rad).map(|(t, x)| [*t, *x]).collect(), pal.trace[0]).width(2.0));
        });
        ui.label(RichText::new("Spectrum of the phase: which rhythms does it contain?").strong());
        plots::plot_bare("spectrum", 180.0).x_axis_label("rhythm [per minute]").y_axis_label("level [dB]").include_x(0.0).include_x(120.0).include_y(-70.0).include_y(15.0).show(ui, |pl| {
            let band = pal.good.gamma_multiply(0.15);
            pl.polygon(Polygon::new("breathing band", plots::bar(BREATHING_BAND_PER_MIN.0, BREATHING_BAND_PER_MIN.1, -70.0, 15.0)).fill_color(band).stroke((0.0, band)));
            pl.hline(HLine::new("threshold", PRESENCE_THRESHOLD_DB).color(pal.warn).style(egui_plot::LineStyle::dashed_dense()));
            pl.line(plots::line("spectrum", tr.spectrum.iter().copied().filter(|q| q[0] <= 120.0).collect(), pal.trace[0]).width(1.8));
        });
        widgets::key(ui, &[(pal.trace[3], "chest movement"), (pal.trace[0], "phase of the echo and its spectrum"), (pal.good, "shaded: breathing rates searched"), (pal.warn, "dashed: \"someone is there\" threshold")]);

        let phase_pp = 4.0 * std::f64::consts::PI * p.chest_mm * 1e-3 / lambda;
        let speed = std::f64::consts::PI * p.breaths_per_min / 60.0 * p.chest_mm * 1e-3;
        widgets::formula(
            ui,
            format!(
                "wavelength = c / f = {}\n\
                 phase change = 4 pi x movement / wavelength = 4 pi x {} / {} = {:.2} rad\n\
                 fastest chest speed = pi x rate x movement = {}  ->  Doppler 2 v f / c = {:.2} Hz\n\
                 strongest rhythm in the breathing band: {:.1} per minute at {:.0} dB (threshold {:.0} dB)",
                si(lambda, "m"),
                si(p.chest_mm * 1e-3, "m"),
                si(lambda, "m"),
                phase_pp,
                si(speed, "m/s"),
                doppler_hz(speed, p.f0_hz),
                tr.breathing_peak[0],
                tr.breathing_peak[1],
                PRESENCE_THRESHOLD_DB,
            ),
        );
        let found = tr.breathing_peak[1] > PRESENCE_THRESHOLD_DB;
        widgets::status(
            ui,
            found == p.person,
            match (found, p.person) {
                (true, true) => format!("someone is there: breathing found at {:.0} per minute", tr.breathing_peak[0]),
                (false, false) => "nobody there, and the radar agrees".to_owned(),
                (false, true) => "missed: someone is there, but the breathing is lost in the noise".to_owned(),
                (true, false) => "false alarm: a rhythm was found in an empty room".to_owned(),
            },
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "empty" => self.p.person = false,
            "shallow" => self.p.chest_mm = 0.3,
            "shallow-60" => {
                self.p.chest_mm = 0.3;
                self.p.f0_hz = 60.0e9;
            }
            "heart" => self.p.heart_mm = 0.4,
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "radar-presence",
        title: "Presence detection: breathing and micro-motion",
        blurb: "A person reading a book hardly moves - but they breathe. A radar can see a chest move by a millimetre.",
        path: &["Radar"],
        make: || Box::new(RadarPresence::default()),
        explain: Explanation {
            what: "A person sits still in front of a radar. The top graph is the movement of their chest as they breathe. The \
                second graph is what the radar measures: the [[phase]] of the echo, which follows the chest exactly. The \
                bottom graph is the [[spectrum]] of that phase over 32 seconds: a peak shows a rhythm, and a peak in the \
                shaded band of breathing rates above the dashed line means \"someone is there\".",
            why: "Lights switching off on someone who is sitting still at a desk is the classic failure of motion sensors. A \
                motion sensor looks for the [[Doppler shift]] of a moving body: a walking person at 1 m/s shifts a 24 GHz \
                echo by 161 Hz. A breathing chest moves at a few millimetres per second, a shift below 1 Hz, which the \
                sensor has to filter out together with the echoes of walls and furniture.\n\n\
                [[Presence detection]] uses the phase instead. The echo travels to the person and back, so when the chest \
                moves 1 mm closer, the path gets 2 mm shorter. Measured in wavelengths, that is a turn of the phase by \
                4 pi x movement / [[wavelength]]. At 24 GHz the wavelength is 12.4 mm, so a 4 mm breath turns the phase \
                by about 4 radians - more than half a full turn, easy to measure. The phase plot is therefore a copy of the \
                chest movement.\n\n\
                A wall or a table also reflects, but its phase never changes. A rhythmic change in a narrow range of rates, \
                about 6 to 40 per minute, is very unlikely to come from anything but a living person. The radar finds it \
                with an [[FFT]] over half a minute or so: that is why presence sensors take a while to decide.\n\n\
                The heartbeat moves the chest too, by a few tenths of a millimetre at about 1 beat per second. It shows as a \
                second, smaller peak. Measuring breathing and heart rate this way is the basis of contactless vital-sign \
                monitoring.",
            formulas: &[
                ("phase change = 4 pi x movement / wavelength", "4 mm at 24 GHz (wavelength 12.4 mm): 4 pi x 4 / 12.4 = 4.04 rad."),
                ("Doppler shift = 2 x speed x f / c", "A chest moving at 3 mm/s: 0.5 Hz - too slow for a motion detector."),
            ],
            try_this: &[
                Experiment { action: "Empty the chair.", watch: "the phase becoming flat noise and the spectrum dropping below the threshold.", preset: Some("empty") },
                Experiment {
                    action: "Make the breathing very shallow: 0.3 mm.",
                    watch: "the phase swinging by only 0.3 rad, and the peak about 4 dB above the threshold at 24 GHz.",
                    preset: Some("shallow"),
                },
                Experiment {
                    action: "The same shallow breathing at 60 GHz.",
                    watch: "the phase swinging 2.5 times as far, and the peak 8 dB higher: a shorter wavelength makes the radar more sensitive to small movements.",
                    preset: Some("shallow-60"),
                },
                Experiment { action: "Add a heartbeat of 0.4 mm.", watch: "a second peak at 72 per minute, much smaller than the breathing peak.", preset: Some("heart") },
            ],
            leaves_out: "Only one reflecting point is simulated. In reality the radar also receives the walls, which add a constant \
                offset that must be removed before the phase is taken, and it first picks the right distance from an FMCW \
                range profile (see the FMCW demo) so that only the person's distance is analysed. Breathing is shown as a \
                perfect sine; real breathing is irregular, and a person who shifts in the chair causes large jumps. The \
                threshold and noise levels are illustrative. Heartbeat measurement in practice needs careful filtering, \
                because the harmonics of breathing are often stronger than the heartbeat itself.",
            refs: &[
                "Phase of a reflected wave: 4 pi R / lambda for a round trip (standard radar result).",
                "Chest movement: breathing 1-12 mm at 0.1-0.5 Hz, heartbeat 0.1-0.5 mm at 0.8-2 Hz (TI mmWave Vital Signs lab, developer's guide for xWR1443).",
            ],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The experiments quote numbers; check them.
    #[test]
    fn experiments_show_what_they_promise() {
        let peak = |preset: &str| {
            let mut d = RadarPresence::default();
            d.apply_preset(preset);
            d.p.simulate(1).breathing_peak[1]
        };
        let (shallow, shallow_60) = (peak("shallow"), peak("shallow-60"));
        assert!((shallow - PRESENCE_THRESHOLD_DB - 4.0).abs() < 1.0, "{shallow}");
        assert!((shallow_60 - shallow - 8.0).abs() < 1.0, "{shallow_60} {shallow}");
        assert!(peak("empty") < PRESENCE_THRESHOLD_DB);
    }
}
