use crate::prelude::*;

/// One real second shows this many simulated seconds: radio waves are far too fast to watch.
const SIM_SECONDS_PER_SECOND: f64 = 0.25e-9;
const SPAN_M: f64 = 1.0;
const PROBE_M: f64 = 0.5;
const HISTORY_S: f64 = 5e-9;

pub struct WaveBasics {
    f_hz: f64,
    amplitude: f64,
    phase_deg: f64,
    show_reference: bool,
    clock: Clock,
}

impl Default for WaveBasics {
    fn default() -> Self {
        Self { f_hz: 2.4e9, amplitude: 1.0, phase_deg: 0.0, show_reference: false, clock: Clock::default() }
    }
}

impl WaveBasics {
    fn value(&self, x_m: f64, t_s: f64, amplitude: f64, phase_deg: f64) -> f64 {
        let lambda = C / self.f_hz;
        amplitude * (std::f64::consts::TAU * (x_m / lambda - self.f_hz * t_s) + phase_deg.to_radians()).sin()
    }
}

impl Demo for WaveBasics {
    fn ui(&mut self, ui: &mut Ui, cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        self.clock.tick(ui, cx.dt);
        let t = self.clock.t * SIM_SECONDS_PER_SECOND;
        let lambda = C / self.f_hz;

        widgets::group(ui, "The wave", |ui| {
            ui.add(widgets::si_slider(&mut self.f_hz, 0.3e9..=6e9, "frequency", "Hz").logarithmic(true))
                .on_hover_text("How many times per second the wave repeats. Radio frequencies are in the millions (MHz) and billions (GHz).");
            ui.horizontal_wrapped(|ui| {
                ui.label("Typical:");
                for (name, f) in [("433 MHz remote control", 433e6), ("868 MHz sensor", 868e6), ("2.4 GHz Bluetooth / Wi-Fi", 2.4e9), ("5.8 GHz motion sensor", 5.8e9)] {
                    if ui.button(name).clicked() {
                        self.f_hz = f;
                    }
                }
            });
            ui.add(egui::Slider::new(&mut self.amplitude, 0.1..=1.5).text("amplitude").fixed_decimals(2))
                .on_hover_text("How strong the wave is: the height of its peaks.");
            ui.add(egui::Slider::new(&mut self.phase_deg, -180.0..=180.0).text("phase").suffix(" deg").fixed_decimals(0))
                .on_hover_text("Shifts the wave along without changing its shape. 360 degrees is one whole cycle.");
            ui.checkbox(&mut self.show_reference, "Show a reference wave (amplitude 1, phase 0)")
                .on_hover_text("A second, unchanged wave to compare against.");
        });
        self.clock.controls(ui);
        ui.label(RichText::new("Slowed down 4 000 000 000 times: one second on screen is a quarter of a nanosecond.").small().color(pal.dim));

        widgets::formula(
            ui,
            format!(
                "wavelength = speed of light / frequency = 299 792 458 m/s / {} = {}\nperiod     = 1 / frequency = {}",
                si(self.f_hz, "Hz"),
                si(lambda, "m"),
                si(1.0 / self.f_hz, "s")
            ),
        );

        ui.label(RichText::new("The wave in space, at this instant").strong());
        let n = 700;
        let wave: Vec<[f64; 2]> = (0..=n).map(|i| i as f64 * SPAN_M / n as f64).map(|x| [x, self.value(x, t, self.amplitude, self.phase_deg)]).collect();
        plots::plot("space", 230.0)
            .x_axis_label("distance from the antenna [m]")
            .y_axis_label("field strength")
            .include_y(-1.8)
            .include_y(1.8)
            .include_x(0.0)
            .include_x(SPAN_M)
            .show(ui, |p| {
                if self.show_reference {
                    let reference: Vec<[f64; 2]> = (0..=n).map(|i| i as f64 * SPAN_M / n as f64).map(|x| [x, self.value(x, t, 1.0, 0.0)]).collect();
                    p.line(plots::line("reference", reference, pal.dim).style(egui_plot::LineStyle::dashed_dense()));
                }
                p.line(plots::line("wave", wave, pal.trace[0]));
                // A ruler exactly one wavelength long.
                p.line(plots::line("one wavelength", vec![[0.1, 1.65], [0.1 + lambda, 1.65]], pal.trace[1]).width(3.0));
                let probe = self.value(PROBE_M, t, self.amplitude, self.phase_deg);
                p.points(egui_plot::Points::new("receiver", vec![[PROBE_M, probe]]).radius(6.0).color(pal.trace[3]));
            });

        ui.label(RichText::new("What the receiver at 0.5 m measures over time").strong());
        let m = 600;
        let history: Vec<[f64; 2]> = (0..=m)
            .map(|i| -HISTORY_S + i as f64 * HISTORY_S / m as f64)
            .map(|dt| [dt * 1e9, self.value(PROBE_M, t + dt, self.amplitude, self.phase_deg)])
            .collect();
        plots::plot("time", 190.0)
            .x_axis_label("time [ns]  (0 = now)")
            .y_axis_label("field strength")
            .include_y(-1.8)
            .include_y(1.8)
            .show(ui, |p| {
                p.line(plots::line("at the receiver", history, pal.trace[3]));
            });
    }

    fn apply_preset(&mut self, preset: &str) {
        match preset {
            "low" => self.f_hz = 433e6,
            "high" => self.f_hz = 5.8e9,
            "phase" => {
                self.phase_deg = 180.0;
                self.show_reference = true;
            }
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "wave-basics",
        title: "What is a radio wave?",
        blurb: "Frequency, wavelength, amplitude and phase - the four words everything else is built on.",
        path: &["Wireless", "Radio basics"],
        make: || Box::new(WaveBasics::default()),
        explain: Explanation {
            what: "The top graph is a snapshot of a radio wave in space: how strong the field is at each distance from the \
                [[antenna]], frozen at one instant and then released in extreme slow motion. The wave slides to the right at the \
                speed of light.\n\n\
                The red dot is a receiver standing still at 0.5 m. It does not see a shape moving past: all it can measure is the \
                field at its own position going up and down. The bottom graph is that measurement over the last few nanoseconds.\n\n\
                The orange bar is exactly one [[wavelength]] long.",
            why: "A radio wave is an electric and magnetic field that keeps swinging back and forth while it travels. Four numbers \
                describe it. [[Frequency]] is how often it swings per second, counted in [[hertz]]. [[Wavelength]] is the distance \
                from one peak to the next. [[Amplitude]] is how high the peaks are, which is the wave's strength. [[Phase]] is where \
                in its swing it is at a given moment.\n\n\
                Frequency and wavelength are tied together because every radio wave travels at the same speed, the speed of light. \
                If it swings more often, it gets less far during each swing, so the peaks sit closer together. That is why a \
                2.4 GHz Bluetooth antenna is a few centimetres long while an FM radio antenna is most of a metre.\n\n\
                A wave like this, steady and unchanging, is called a [[carrier]]. It carries no information yet. Sending data means \
                changing the amplitude, the frequency or the phase a little - the next demos show how.",
            formulas: &[
                ("wavelength = c / frequency", "Speed of light divided by frequency. At 2.4 GHz: 12.5 cm."),
                ("period = 1 / frequency", "How long one swing takes. At 2.4 GHz: 0.417 nanoseconds."),
            ],
            try_this: &[
                Experiment {
                    action: "Drag the frequency down to 433 MHz, then up to 5.8 GHz.",
                    watch: "the orange one-wavelength bar. Low frequency: long wavelength, slow swings at the receiver. High frequency: peaks crowd together.",
                    preset: Some("low"),
                },
                Experiment {
                    action: "Switch on the reference wave and set the phase to 180 degrees.",
                    watch: "the two waves being exact mirror images. Added together they would cancel completely.",
                    preset: Some("phase"),
                },
                Experiment {
                    action: "Pause the animation and change only the amplitude.",
                    watch: "the peaks getting taller or shorter while their positions stay put. Amplitude does not change frequency or wavelength.",
                    preset: None,
                },
            ],
            leaves_out: "A real wave gets weaker as it spreads out from the antenna; here the strength stays the same over the whole \
                metre. Only the electric field along one line is shown - the real wave spreads in all directions (see the next demo) \
                and has a magnetic field at right angles to the electric one.",
            refs: &["Speed of light c = 299 792 458 m/s (exact, SI definition)."],
        },
    }
}
