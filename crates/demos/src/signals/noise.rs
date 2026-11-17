use crate::prelude::*;
use egui_plot::Points;
use sim_core::signals::{noisy_readings, std_dev};

const COUNT: usize = 600;
const STEP_AT: usize = 350;

pub struct Noise {
    sigma_mv: f64,
    n: usize,
    step: bool,
}

impl Default for Noise {
    fn default() -> Self {
        Self { sigma_mv: 20.0, n: 16, step: true }
    }
}

impl Demo for Noise {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "A sensor reading with noise on it", |ui| {
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "sigma", |ui| ui.add(egui::Slider::new(&mut self.sigma_mv, 0.0..=100.0).text("noise").suffix(" mV").fixed_decimals(0)).on_hover_text("How much each single reading typically deviates from the true value (the standard deviation)."));
                widgets::wrap(ui, "n", |ui| ui.add(egui::Slider::new(&mut self.n, 1..=256).logarithmic(true).text("average over").suffix(" readings")).on_hover_text("Each output value is the mean of this many of the most recent readings."));
                ui.checkbox(&mut self.step, "the true value jumps by 100 mV").on_hover_text("Something real happens: the quantity being measured changes suddenly.");
            });
        });
        let sigma = self.sigma_mv / 1000.0;
        let r = noisy_readings(COUNT, 1.0, if self.step { 1.1 } else { 1.0 }, STEP_AT, sigma, self.n, 11);
        let steady = self.n.min(STEP_AT - 20)..STEP_AT;
        let (raw_sd, avg_sd) = (std_dev(&r.raw[steady.clone()]), std_dev(&r.averaged[steady]));

        ui.label(RichText::new("Single readings and their running average").strong());
        plots::plot_bare("readings", 300.0).x_axis_label("reading number").y_axis_label("volts").show(ui, |p| {
            p.points(Points::new("single readings", plots::series(&r.raw, 0.0, 1.0)).radius(1.8).color(pal.dim));
            p.line(plots::line("truth", plots::series(&r.truth, 0.0, 1.0), pal.text).style(egui_plot::LineStyle::dashed_dense()));
            p.line(plots::line("average", plots::series(&r.averaged, 0.0, 1.0), pal.trace[1]).width(2.5));
        });
        widgets::key(ui, &[(pal.dim, "single readings"), (pal.trace[1], "average of the last readings"), (pal.text, "dashed: the true value")]);

        widgets::formula(
            ui,
            format!(
                "noise after averaging = noise / square root of N = {:.1} mV / sqrt({}) = {:.2} mV\n\
                 measured in this run  : single readings {:.1} mV, averaged {:.2} mV\n\
                 time to follow a jump = N readings = {}        (halfway after {})",
                self.sigma_mv,
                self.n,
                self.sigma_mv / (self.n as f64).sqrt(),
                raw_sd * 1000.0,
                avg_sd * 1000.0,
                self.n,
                self.n / 2,
            ),
        );
        let visible = self.sigma_mv / (self.n as f64).sqrt() < 100.0 / 5.0;
        widgets::status(
            ui,
            visible,
            if visible {
                format!("the 100 mV jump stands out clearly from the remaining noise - {} readings after it happened", self.n)
            } else {
                "the remaining noise is still large compared with the 100 mV jump: a single averaged value cannot be trusted to show it".to_owned()
            },
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "none" => self.n = 1,
            "heavy" => self.n = 256,
            "noisy" => (self.sigma_mv, self.n) = (100.0, 4),
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "signals-noise",
        title: "Noise and averaging",
        blurb: "Every measurement jitters. Averaging calms it down - at a price you can see.",
        path: &["Signals and analog"],
        make: || Box::new(Noise::default()),
        explain: Explanation {
            what: "Each grey dot is one reading of a sensor. The dashed line is the true value, which the readings scatter \
                around because of [[noise]]. The orange line is what you get by averaging the most recent readings.\n\n\
                Two thirds of the way along, the true value jumps up by 100 mV. Watch how the orange line follows.",
            why: "No measurement is perfectly steady. The electrons in every resistor jiggle with heat, the supply voltage \
                ripples, neighbouring circuits couple in. On top of the value you want sits a random error that is different \
                at every reading.\n\n\
                Random errors have a useful property: they are as often too high as too low. Add up several readings and the \
                true value adds up in full, while the errors partly cancel. Divide by the number of readings, and you have a \
                value with less noise than any single one.\n\n\
                How much less follows a law that is worth knowing by heart: the noise shrinks with the square root of the \
                number of readings. Four readings halve it. To halve it again takes sixteen. To get ten times less noise you \
                need a hundred readings. The first few readings help a lot, and every further improvement costs four times \
                as much as the last.\n\n\
                And the cost is time. An average over N readings only shows a change in full when all N readings were taken \
                after the change. Until then it is somewhere in between. So the same setting that makes the line smooth \
                makes it slow. A thermometer can afford to average for a second; a motor controller cannot.\n\n\
                Averaging only removes errors that are random. If every reading is 5 mV too high because of an offset in \
                the circuit, the average is 5 mV too high as well, with great precision.",
            formulas: &[
                ("noise after averaging N readings = noise of one reading / square root of N", "20 mV of noise, 16 readings averaged: 20 / 4 = 5 mV."),
                ("each 4 times more readings = half the noise = about 1 more bit of resolution", "This is how a 12-bit converter is made to deliver 14 useful bits: by averaging 16 readings."),
            ],
            try_this: &[
                Experiment { action: "Average over 1 reading - that is, do not average.", watch: "the orange line jumping about as wildly as the dots. The 100 mV step is visible, but any single value could be 40 mV off.", preset: Some("none") },
                Experiment {
                    action: "Average over 256 readings.",
                    watch: "a beautifully calm line - which needs 256 readings to climb to the new value after the jump. Smooth and sluggish are the same setting.",
                    preset: Some("heavy"),
                },
                Experiment {
                    action: "Set the noise to 100 mV and average over 4.",
                    watch: "the jump almost disappearing in the remaining noise of 50 mV. Then raise the averaging until it is clearly visible again, and note what it costs in delay.",
                    preset: Some("noisy"),
                },
            ],
            leaves_out: "The noise here is of the simplest kind: independent from reading to reading (\"white\") and with a \
                bell-shaped distribution. Real noise often contains slow drifts, which averaging does not remove, and \
                interference at fixed frequencies such as mains hum, which is better removed by averaging over exactly one \
                mains period. A moving average is one filter among many; the next demo shows one that needs no memory of \
                old readings. Gaining resolution by averaging also needs the noise to be at least about one converter step \
                large.",
            refs: &["Standard error of the mean: sigma / sqrt(N) for N independent readings (any statistics textbook). Resolution gain by oversampling and averaging: e.g. Silicon Labs application note AN118, \"Improving ADC Resolution by Oversampling and Averaging\"."],
        },
    }
}
