use crate::prelude::*;
use sim_core::{dsp, rng::Rng};

const FS: f64 = 100_000.0;
const N: usize = 2048;

#[derive(Clone, Copy)]
struct Tone {
    on: bool,
    f_hz: f64,
    amplitude: f64,
}

pub struct Spectrum {
    tones: [Tone; 3],
    square: bool,
    noise: f64,
}

impl Default for Spectrum {
    fn default() -> Self {
        Self {
            tones: [
                Tone { on: true, f_hz: 2_000.0, amplitude: 1.0 },
                Tone { on: false, f_hz: 7_000.0, amplitude: 0.5 },
                Tone { on: false, f_hz: 15_000.0, amplitude: 0.25 },
            ],
            square: false,
            noise: 0.0,
        }
    }
}

impl Spectrum {
    fn signal(&self) -> Vec<f64> {
        let mut rng = Rng::new(42);
        (0..N)
            .map(|n| {
                let t = n as f64 / FS;
                let mut y = rng.gauss() * self.noise;
                for (i, tone) in self.tones.iter().enumerate().filter(|(_, t)| t.on) {
                    let s = (std::f64::consts::TAU * tone.f_hz * t).sin();
                    y += tone.amplitude * if self.square && i == 0 { s.signum() } else { s };
                }
                y
            })
            .collect()
    }
}

impl Demo for Spectrum {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "Build a signal from pure tones", |ui| {
            for (i, tone) in self.tones.iter_mut().enumerate() {
                ui.horizontal_wrapped(|ui| {
                    ui.checkbox(&mut tone.on, format!("tone {}", i + 1)).on_hover_text("Add this tone to the signal");
                    ui.add_enabled(tone.on, widgets::si_slider(&mut tone.f_hz, 500.0..=40_000.0, "frequency", "Hz").logarithmic(true));
                    ui.add_enabled(tone.on, egui::Slider::new(&mut tone.amplitude, 0.01..=1.0).logarithmic(true).text("amplitude").fixed_decimals(2));
                });
            }
            ui.checkbox(&mut self.square, "Make tone 1 a square wave instead of a smooth sine")
                .on_hover_text("A square wave has sharp edges. Sharp edges need many high frequencies.");
            ui.add(egui::Slider::new(&mut self.noise, 0.0..=0.5).text("noise").fixed_decimals(2))
                .on_hover_text("Random hiss added to the signal. Every real signal has some.");
        });

        let x = self.signal();
        ui.label(RichText::new("The signal over time").strong());
        plots::plot("time", 200.0).x_axis_label("time [ms]").y_axis_label("value").include_y(-2.0).include_y(2.0).show(ui, |p| {
            p.line(plots::line("signal", plots::series(&x[..400], 0.0, 1000.0 / FS), pal.trace[0]));
        });

        ui.label(RichText::new("The same signal sorted by frequency (its spectrum)").strong());
        let spec = dsp::spectrum_real(&x, FS);
        let pts: Vec<[f64; 2]> = spec.iter().map(|p| [p[0] / 1000.0, p[1].max(-100.0)]).collect();
        plots::plot("spectrum", 240.0).x_axis_label("frequency [kHz]").y_axis_label("level [dB]").include_y(-90.0).include_y(5.0).show(ui, |p| {
            p.line(plots::line("spectrum", pts, pal.trace[1]).fill(-100.0).fill_alpha(0.15));
        });
        let peak = dsp::peak(&spec);
        widgets::formula(
            ui,
            format!(
                "strongest component: {} at {:.1} dB      (0 dB = amplitude 1,  -20 dB = amplitude 0.1,  -40 dB = 0.01)",
                si(peak[0], "Hz"),
                peak[1]
            ),
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        match preset {
            "three" => self.tones.iter_mut().for_each(|t| t.on = true),
            "square" => self.square = true,
            "buried" => {
                self.tones[0].amplitude = 0.05;
                self.noise = 0.3;
            }
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "spectrum",
        title: "Spectrum: seeing frequencies",
        blurb: "Any signal is a mix of pure tones. The spectrum shows the recipe.",
        path: &["Wireless", "Radio basics"],
        make: || Box::new(Spectrum::default()),
        explain: Explanation {
            what: "The top graph shows a signal the usual way: its value as time passes. The bottom graph shows the very same \
                signal in a different way: sorted by [[frequency]]. Each spike says \"this signal contains a tone of this frequency, \
                this strong\". This picture is called the [[spectrum]].\n\n\
                The vertical scale is in [[dB]], which squeezes a huge range onto one axis: every 20 dB down is ten times less amplitude.",
            why: "Any signal, however complicated, can be built by adding up pure sine waves of different frequencies and strengths. \
                The [[FFT]] does the reverse: it takes a stretch of signal and works out which sine waves it is made of.\n\n\
                This matters for radio because the air is shared by frequency. Bluetooth, Wi-Fi and a microwave oven all transmit at \
                once, and receivers tell them apart only by frequency. So the questions that matter for a transmitter are: where is \
                my signal in the spectrum, and how wide is it? The width is its [[bandwidth]]. A transmitter that is too wide \
                spills into its neighbours' [[channels]], which is why radio regulations set limits, and why Bluetooth smooths its bits \
                before sending (next demo).\n\n\
                The rule of thumb to take away: the faster or sharper a signal changes, the more bandwidth it needs.",
            formulas: &[
                ("dB = 20 x log10(amplitude ratio)", "A convenient scale for ratios. -6 dB is half the amplitude, -20 dB a tenth, -40 dB a hundredth."),
            ],
            try_this: &[
                Experiment {
                    action: "Switch on tones 2 and 3.",
                    watch: "the time graph turning into an unreadable wiggle, while the spectrum stays simple: three clean spikes.",
                    preset: Some("three"),
                },
                Experiment {
                    action: "Turn tone 1 into a square wave.",
                    watch: "a whole row of new spikes at 3, 5, 7 ... times the tone's frequency. Sharp edges are made of high frequencies.",
                    preset: Some("square"),
                },
                Experiment {
                    action: "Make tone 1 very weak (amplitude 0.05) and turn the noise up to 0.3.",
                    watch: "the tone vanishing in the time graph but still standing out in the spectrum. Receivers use this to dig weak signals out of [[noise]].",
                    preset: Some("buried"),
                },
            ],
            leaves_out: "The frequencies here are audio-range (kilohertz) so the numbers stay friendly; radio works the same way a \
                million times higher. The FFT looks at a short stretch of signal through a smooth window, which makes every spike a \
                little wider than a perfect line.",
            refs: &["2048-point FFT with a Hann window, sample rate 100 kHz."],
        },
    }
}
