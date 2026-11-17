use crate::prelude::*;
use egui_plot::{Points, VLine};
use sim_core::signals::{lowpass, lowpass_alpha, lowpass_gain};
use std::f64::consts::TAU;

const FS: f64 = 1000.0;
const WANTED_HZ: f64 = 2.0;
const HUM_HZ: f64 = 50.0;

pub struct Filter {
    fc_hz: f64,
    hum: f64,
}

impl Default for Filter {
    fn default() -> Self {
        Self { fc_hz: 5.0, hum: 0.4 }
    }
}

impl Demo for Filter {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "A slow signal with mains hum on it, and a low-pass filter", |ui| {
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "fc", |ui| ui.add(widgets::si_slider(&mut self.fc_hz, 0.2..=300.0, "cutoff frequency", "Hz").logarithmic(true)).on_hover_text("Where the filter starts to block: slower signals pass, faster ones are weakened."));
                widgets::wrap(ui, "hum", |ui| ui.add(egui::Slider::new(&mut self.hum, 0.0..=1.0).text("strength of the 50 Hz hum").fixed_decimals(2)).on_hover_text("Interference from the mains, added to the wanted 2 Hz signal."));
            });
        });
        let alpha = lowpass_alpha(self.fc_hz, FS);
        let n = 1000;
        let wanted: Vec<f64> = (0..n).map(|i| (TAU * WANTED_HZ * i as f64 / FS).sin()).collect();
        let input: Vec<f64> = (0..n).map(|i| wanted[i] + self.hum * (TAU * HUM_HZ * i as f64 / FS).sin()).collect();
        let output = lowpass(&input, alpha);

        ui.label(RichText::new("Over time: what goes in, and what comes out").strong());
        plots::plot_bare("time", 240.0).x_axis_label("time [ms]").y_axis_label("value").show(ui, |p| {
            p.line(plots::line("in", plots::series(&input, 0.0, 1.0), pal.dim));
            p.line(plots::line("wanted", plots::series(&wanted, 0.0, 1.0), pal.text).style(egui_plot::LineStyle::dashed_dense()));
            p.line(plots::line("out", plots::series(&output, 0.0, 1.0), pal.trace[1]).width(2.5));
        });
        widgets::key(ui, &[(pal.dim, "input: the wanted signal plus hum"), (pal.text, "dashed: the wanted signal alone"), (pal.trace[1], "output of the filter")]);

        ui.label(RichText::new("By frequency: how much of each frequency gets through").strong());
        let curve: Vec<[f64; 2]> = (0..=200).map(|i| -1.0 + 3.7 * i as f64 / 200.0).map(|x| [x, 100.0 * lowpass_gain(10f64.powf(x), FS, alpha)]).collect();
        let (g_wanted, g_hum) = (lowpass_gain(WANTED_HZ, FS, alpha), lowpass_gain(HUM_HZ, FS, alpha));
        plots::plot_bare("gain", 200.0)
            .x_axis_label("frequency")
            .y_axis_label("gets through [%]")
            .include_y(0.0)
            .include_y(105.0)
            .x_grid_spacer(egui_plot::uniform_grid_spacer(|_| [1.0, 1.0, 1.0]))
            .x_axis_formatter(|mark, _| {
                let i = mark.value.round();
                if (mark.value - i).abs() < 0.01 { si(10f64.powf(i), "Hz") } else { String::new() }
            })
            .show(ui, |p| {
                p.line(plots::line("gain", curve, pal.trace[1]).width(2.0));
                p.vline(VLine::new("cutoff", self.fc_hz.log10()).color(pal.warn));
                p.points(Points::new("wanted", vec![[WANTED_HZ.log10(), 100.0 * g_wanted]]).radius(6.0).color(pal.good));
                p.points(Points::new("hum", vec![[HUM_HZ.log10(), 100.0 * g_hum]]).radius(6.0).color(pal.bad));
            });
        widgets::key(ui, &[(pal.good, "the wanted 2 Hz signal"), (pal.bad, "the 50 Hz hum"), (pal.warn, "the cutoff frequency")]);

        let shift = (alpha.recip()).log2().round() as i32;
        widgets::formula(
            ui,
            format!(
                "for every new reading x:    y = y + alpha x (x - y)\n\
                 alpha = 1 - e^(-2 pi x cutoff / sample rate) = 1 - e^(-2 pi x {} / {}) = {:.4}\n\
                 gets through:  2 Hz signal {:.0} %        50 Hz hum {:.1} %\n\n\
                 in C, with whole numbers only:    y += (x - y) >> {shift};        /* alpha = 1/{} */",
                si(self.fc_hz, "Hz"),
                si(FS, "Hz"),
                alpha,
                g_wanted * 100.0,
                g_hum * 100.0,
                1 << shift.clamp(0, 20),
            ),
        );
        widgets::status(
            ui,
            g_wanted > 0.9 && g_hum * self.hum < 0.05,
            if g_wanted <= 0.9 {
                format!("the cutoff is too low: the filter also flattens the wanted signal, to {:.0} %, and delays it", g_wanted * 100.0)
            } else if g_hum * self.hum >= 0.05 {
                format!("the cutoff is too high: {:.0} % of the hum still gets through", g_hum * 100.0)
            } else {
                "the wanted signal passes almost untouched and the hum is nearly gone".to_owned()
            },
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "too-high" => self.fc_hz = 200.0,
            "too-low" => self.fc_hz = 0.5,
            "strong-hum" => self.hum = 1.0,
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "signals-filter",
        title: "Digital filters: one line that removes the noise",
        blurb: "The most used filter in firmware is a single line of code. See what it lets through and what it costs.",
        path: &["Signals and analog"],
        make: || Box::new(Filter::default()),
        explain: Explanation {
            what: "The grey curve is a sensor signal as it arrives: a slow swing that we want, with a fast 50 Hz ripple from the \
                mains riding on it. The orange curve is the same signal after a [[low-pass filter]]. The dashed line is the \
                wanted signal alone, for comparison.\n\n\
                The lower graph shows the filter's character: for each frequency, how much of it gets through. The two dots \
                are the wanted signal and the hum.",
            why: "Often the interesting part of a signal is slow - a temperature, a position, a light level - and the \
                disturbance is fast. Separating them by speed is what a low-pass filter does: slow changes pass, fast ones \
                are held back.\n\n\
                The simplest digital one needs a single stored number and a single line of code. With every new reading, \
                move the stored value a fixed fraction of the way towards the reading. If the fraction is small, one odd \
                reading barely moves the value, so quick wiggles cancel out before they have any effect; a lasting change \
                pulls the value along, step by step. That fraction, usually called alpha, is the only setting.\n\n\
                Compared with averaging the last N readings it needs no memory of old readings, and it reacts in a smooth \
                curve rather than a ramp. It behaves exactly like the classic analog filter made of one resistor and one \
                capacitor.\n\n\
                The graph by frequency shows its limits. The transition from \"passes\" to \"blocked\" is gentle: at the \
                cutoff frequency 71 % still gets through, at ten times the cutoff about 10 %. To remove the hum well, the \
                cutoff has to sit far below 50 Hz - and then it must still be far above the wanted signal, or that gets \
                flattened and delayed too. When wanted and unwanted are closer together than this filter can separate, a \
                steeper, more elaborate filter is needed.",
            formulas: &[
                ("y = y + alpha x (x - y)", "With alpha = 1/16: each reading moves the output one sixteenth of the way towards it."),
                ("alpha = 1 - e^(-2 pi x cutoff / sample rate)", "Cutoff 5 Hz at 1000 readings per second: alpha = 0.031, about 1/32."),
            ],
            try_this: &[
                Experiment {
                    action: "Raise the cutoff to 200 Hz.",
                    watch: "the orange curve following every ripple of the input: 97 % of the hum gets through. A filter whose cutoff lies above the disturbance does nothing.",
                    preset: Some("too-high"),
                },
                Experiment {
                    action: "Lower the cutoff to 0.5 Hz.",
                    watch: "the hum gone completely - and the wanted signal with it: shrunk to a quarter and lagging far behind the dashed line. This is the same smooth-or-fast trade as with averaging.",
                    preset: Some("too-low"),
                },
                Experiment {
                    action: "Make the hum as strong as the signal itself.",
                    watch: "the filter still recovering a recognisable signal from an input in which you can hardly see it. Then look for the cutoff that gives the cleanest result.",
                    preset: Some("strong-hum"),
                },
            ],
            leaves_out: "One filter type out of many. Filters with several stages fall off more steeply; \"notch\" filters remove \
                one frequency such as 50 Hz and nothing else; high-pass filters do the opposite of this one. The integer \
                version in the last line of the formula box only allows alpha to be 1/2, 1/4, 1/8 and so on, and loses a \
                little accuracy to rounding. A digital filter cannot repair aliasing: whatever is above half the sample \
                rate has to be removed by an analog filter before the converter.",
            refs: &["First-order IIR low-pass (exponential moving average), the discrete equivalent of an RC low-pass: gain |H(f)| = alpha / sqrt(1 - 2(1 - alpha) cos(2 pi f / fs) + (1 - alpha)^2); e.g. S. W. Smith, \"The Scientist and Engineer's Guide to Digital Signal Processing\", chapter 19."],
        },
    }
}
