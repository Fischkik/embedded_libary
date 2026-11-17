use crate::prelude::*;
use egui_plot::{Points, VLine};
use sim_core::signals::{alias_frequency, alias_signed};
use std::f64::consts::TAU;


pub struct Sampling {
    f_hz: f64,
    fs_hz: f64,
}

impl Default for Sampling {
    fn default() -> Self {
        Self { f_hz: 150.0, fs_hz: 1000.0 }
    }
}

impl Demo for Sampling {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "A signal and a sampler", |ui| {
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "f", |ui| ui.add(widgets::si_slider(&mut self.f_hz, 20.0..=2500.0, "signal frequency", "Hz").logarithmic(true)).on_hover_text("The real signal: a pure tone of this frequency."));
                widgets::wrap(ui, "fs", |ui| ui.add(widgets::si_slider(&mut self.fs_hz, 20.0..=5000.0, "sample rate", "Hz").logarithmic(true)).on_hover_text("How many times per second the ADC measures the signal."));
            });
        });
        let (f, fs) = (self.f_hz, self.fs_hz);
        let apparent = alias_frequency(f, fs);
        let signed = alias_signed(f, fs);
        let ok = f < fs / 2.0;
        // Long enough to show two periods of whatever the samples seem to show.
        let window_s = if apparent > 0.0 { (2.0 / apparent).clamp(0.02, 0.5) } else { 0.02 };

        ui.label(RichText::new("The real signal, the samples, and what the samples seem to show").strong());
        let dense = 4000;
        let real: Vec<[f64; 2]> = (0..=dense).map(|i| i as f64 / dense as f64 * window_s).map(|t| [t * 1e3, (TAU * f * t).sin()]).collect();
        let seems: Vec<[f64; 2]> = (0..=dense).map(|i| i as f64 / dense as f64 * window_s).map(|t| [t * 1e3, (TAU * signed * t).sin()]).collect();
        let dots: Vec<[f64; 2]> = (0..=(window_s * fs) as usize).map(|n| n as f64 / fs).map(|t| [t * 1e3, (TAU * f * t).sin()]).collect();
        plots::plot_bare("time", 240.0).x_axis_label("time [ms]").y_axis_label("value").include_y(-1.2).include_y(1.2).show(ui, |p| {
            p.line(plots::line("real", real, pal.dim));
            if !ok {
                p.line(plots::line("seems", seems, pal.bad).width(2.0));
            }
            p.points(Points::new("samples", dots).radius(4.0).color(pal.trace[1]));
        });
        widgets::key(ui, &[(pal.dim, "the real signal"), (pal.trace[1], "the samples: all the microcontroller ever sees"), (pal.bad, "the slowest wave that fits the samples (shown when it is not the real one)")]);

        ui.label(RichText::new("On the frequency scale").strong());
        let top = fs.max(f) * 1.1;
        plots::plot_bare("freq", 110.0).x_axis_label("frequency [Hz]").include_x(0.0).include_x(top).include_y(0.0).include_y(1.0).show_axes([true, false]).show(ui, |p| {
            p.vline(VLine::new("half the sample rate", fs / 2.0).color(pal.warn).width(2.0));
            p.vline(VLine::new("sample rate", fs).color(pal.dim));
            p.points(Points::new("real", vec![[f, 0.65]]).radius(7.0).color(pal.text));
            p.points(Points::new("apparent", vec![[apparent, 0.3]]).radius(7.0).color(if ok { pal.good } else { pal.bad }));
        });
        widgets::key(ui, &[(pal.text, "upper dot: the real frequency"), (if ok { pal.good } else { pal.bad }, "lower dot: the frequency the samples show"), (pal.warn, "half the sample rate: the limit"), (pal.dim, "the sample rate")]);

        widgets::status(
            ui,
            ok,
            if ok {
                format!("the signal is below half the sample rate: the samples show its true frequency, {}", si(f, "Hz"))
            } else {
                format!("the signal is above half the sample rate: the samples look like a {} signal that is not there", si(apparent, "Hz"))
            },
        );
        widgets::formula(
            ui,
            format!(
                "highest frequency that can be sampled = sample rate / 2 = {} / 2 = {}\n\
                 samples per period of the signal      = sample rate / signal frequency = {:.2}\n\
                 apparent frequency = distance to the nearest multiple of the sample rate = |{} - {} x {}| = {}",
                si(fs, "Hz"),
                si(fs / 2.0, "Hz"),
                fs / f,
                si(f, "Hz"),
                (f / fs).round(),
                si(fs, "Hz"),
                si(apparent, "Hz"),
            ),
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "alias" => self.f_hz = 900.0,
            "equal" => self.f_hz = 1000.0,
            "hum" => (self.f_hz, self.fs_hz) = (50.0, 60.0),
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "signals-sampling",
        title: "Sampling and aliasing",
        blurb: "Measure a fast signal too rarely, and the numbers describe a slow signal that does not exist.",
        path: &["Signals and analog"],
        make: || Box::new(Sampling::default()),
        explain: Explanation {
            what: "The grey curve is a real signal: a voltage that swings up and down smoothly. The dots are the moments an \
                [[ADC]] measures it. The microcontroller never sees the curve, only the dots.\n\n\
                The lower strip shows the same thing as frequencies: where the real signal is, and where the samples make it \
                appear to be. In the opening state the two agree.",
            why: "A microcontroller cannot watch a signal continuously. It takes a [[sample]] now and then and has to fill in \
                the rest in its head. If the signal changes slowly compared with the sampling, that is no problem: the dots \
                trace out the curve.\n\n\
                But when the signal is fast, a whole swing or more happens between two samples, and the dots land at \
                scattered points of different swings. Join them up, and a much slower wave appears. It is not a blurred \
                version of the real signal; it is a different signal, with a frequency of its own, and nothing in the \
                numbers reveals that it is false. This is [[aliasing]]. Wagon wheels that seem to turn slowly backwards in a \
                film are the same effect: the camera samples the wheel 24 times a second.\n\n\
                The rule is strict and simple. A signal is captured faithfully only if it is sampled more than twice per \
                period, so the highest frequency a sampler can handle is half its sample rate. Anything above is not \
                lost - it is folded down and lands somewhere between zero and that limit, on top of the signals that \
                really belong there.\n\n\
                Because the damage cannot be undone afterwards, it has to be prevented before sampling. Every serious \
                measuring input has an analog low-pass filter in front of the converter, which removes everything above \
                half the sample rate before it can fold. Sampling faster than strictly needed makes that filter easy to \
                build.",
            formulas: &[
                ("highest usable frequency = sample rate / 2", "Sampling 1000 times per second captures signals up to 500 Hz."),
                ("apparent frequency = distance from the signal frequency to the nearest multiple of the sample rate", "A 900 Hz signal sampled at 1000 Hz appears as |900 - 1000| = 100 Hz."),
            ],
            try_this: &[
                Experiment {
                    action: "Raise the signal to 900 Hz while sampling at 1 kHz.",
                    watch: "the dots lining up along a slow red wave of 100 Hz. Look only at the dots and ask yourself whether you could tell.",
                    preset: Some("alias"),
                },
                Experiment {
                    action: "Make the signal frequency exactly equal to the sample rate.",
                    watch: "every sample landing on the same point of the wave: the samples say \"nothing is moving at all\".",
                    preset: Some("equal"),
                },
                Experiment {
                    action: "Sample 50 Hz mains hum at 60 Hz.",
                    watch: "a 10 Hz wobble in the readings. This is how hum from the mains turns up as a slow drift in a sensor that is read too slowly.",
                    preset: Some("hum"),
                },
            ],
            leaves_out: "A single pure tone; real signals are mixtures of many frequencies, and each one folds by the same rule. \
                Sampling here is instantaneous and exactly regular; a real converter takes a short time per sample and its \
                clock jitters. Exactly at half the sample rate the result depends on where in the wave the samples happen to \
                fall. Aliasing is sometimes used on purpose, to bring a high-frequency radio signal down to a low one.",
            refs: &["Nyquist-Shannon sampling theorem: C. E. Shannon, \"Communication in the Presence of Noise\", Proceedings of the IRE, 1949. Folding of frequencies above half the sample rate: any signal-processing textbook, e.g. Oppenheim and Schafer, \"Discrete-Time Signal Processing\"."],
        },
    }
}
