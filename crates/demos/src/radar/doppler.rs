use crate::prelude::*;
use sim_core::radar::{Target, cw_iq, doppler_hz};
use sim_core::{dsp, rng::Rng};

const FS: f64 = 4000.0;
const N: usize = 1024;
const ROOM_M: f32 = 10.0;

pub struct RadarDoppler {
    f0_hz: f64,
    /// Speed towards the radar, m/s. Negative = walking away.
    approach_mps: f64,
    range_m: f64,
    noise: f64,
    clock: Clock,
}

impl Default for RadarDoppler {
    fn default() -> Self {
        Self { f0_hz: 24.125e9, approach_mps: 1.2, range_m: 7.0, noise: 0.05, clock: Clock::default() }
    }
}

impl Demo for RadarDoppler {
    fn ui(&mut self, ui: &mut Ui, cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        // Move the person; turn around at the ends of the room.
        let dt = self.clock.tick(ui, cx.dt);
        self.range_m -= self.approach_mps * dt;
        if self.range_m < 1.0 || self.range_m > 9.5 {
            self.range_m = self.range_m.clamp(1.0, 9.5);
            self.approach_mps = -self.approach_mps;
        }

        widgets::group(ui, "The radar and the person", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Radar frequency:");
                for (name, f) in [("5.8 GHz", 5.8e9), ("10.525 GHz", 10.525e9), ("24.125 GHz", 24.125e9), ("60 GHz", 60e9)] {
                    ui.selectable_value(&mut self.f0_hz, f, name).on_hover_text("Common frequencies for motion sensors. Higher frequency = shorter wavelength = bigger Doppler shift for the same speed.");
                }
            });
            ui.add(egui::Slider::new(&mut self.approach_mps, -3.0..=3.0).text("speed towards the radar").suffix(" m/s").fixed_decimals(2))
                .on_hover_text("Positive: walking towards the radar. Negative: walking away. A person walks at about 1.4 m/s.");
            ui.add(egui::Slider::new(&mut self.noise, 0.0..=1.0).text("receiver noise").fixed_decimals(2))
                .on_hover_text("Random noise added to the received signal.");
        });
        self.clock.controls(ui);

        let world = egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(ROOM_M, 2.2));
        scene::scene(ui, "room", world, 130.0, |s| {
            let radar = s.pos(pos2(0.35, 1.1));
            s.painter.rect_filled(egui::Rect::from_center_size(radar, vec2(14.0, 34.0)), 3.0, pal.trace[0]);
            s.painter.text(radar + vec2(12.0, -24.0), egui::Align2::LEFT_BOTTOM, "radar", egui::FontId::proportional(13.0), pal.text);
            let mut p = pos2(self.range_m as f32 + 0.35, 1.1);
            let r = s.handle("person", &mut p, 12.0);
            if r.dragged() {
                self.range_m = (p.x - 0.35).clamp(1.0, 9.5) as f64;
            }
            let c = s.pos(pos2(self.range_m as f32 + 0.35, 1.1));
            s.painter.circle_filled(c, 11.0, pal.trace[3]);
            s.painter.text(c + vec2(0.0, -16.0), egui::Align2::CENTER_BOTTOM, format!("person, {:.1} m away", self.range_m), egui::FontId::proportional(13.0), pal.text);
            if self.approach_mps.abs() > 0.02 {
                s.painter.arrow(c, vec2(-(self.approach_mps as f32) * 28.0, 0.0), (2.5, pal.text));
            }
        });

        let fd = doppler_hz(self.approach_mps, self.f0_hz);
        let lambda = C / self.f0_hz;
        widgets::formula(
            ui,
            format!(
                "Doppler shift = 2 x speed x frequency / c = 2 x {:.2} m/s x {} / c = {:.0} Hz\n\
                 wavelength = {}   ->   the echo's phase goes round once for every {} the person moves",
                self.approach_mps,
                si(self.f0_hz, "Hz"),
                fd,
                si(lambda, "m"),
                si(lambda / 2.0, "m")
            ),
        );

        // The receiver output over the last quarter second.
        let window_s = N as f64 / FS;
        let target = Target { range_m: self.range_m + self.approach_mps * window_s, approach_mps: self.approach_mps, amplitude: 1.0 };
        let iq = cw_iq(&[target], self.f0_hz, FS, N, 0.0, self.noise, &mut Rng::new(self.clock.t.to_bits()));

        ui.columns(2, |cols| {
            cols[0].label(RichText::new("Receiver output over 50 ms").strong());
            let shown = 200;
            let i: Vec<f64> = iq[N - shown..].iter().map(|c| c.re).collect();
            let q: Vec<f64> = iq[N - shown..].iter().map(|c| c.im).collect();
            plots::plot("iq", 220.0).x_axis_label("time [ms]").y_axis_label("level").include_y(-1.6).include_y(1.6).show(&mut cols[0], |p| {
                p.line(plots::line("I", plots::series(&i, 0.0, 1000.0 / FS), pal.trace[0]));
                p.line(plots::line("Q", plots::series(&q, 0.0, 1000.0 / FS), pal.trace[1]));
            });
            cols[1].label(RichText::new("The same as a rotating arrow (I across, Q up)").strong());
            let trail: Vec<[f64; 2]> = iq[N - 40..].iter().map(|c| [c.re, c.im]).collect();
            let tip = iq[N - 1];
            plots::plot("arrow", 220.0).data_aspect(1.0).include_x(-1.6).include_x(1.6).include_y(-1.6).include_y(1.6).show(&mut cols[1], |p| {
                p.line(plots::line("recent path", trail, pal.dim));
                p.line(plots::line("now", vec![[0.0, 0.0], [tip.re, tip.im]], pal.trace[3]).width(3.0));
            });
        });

        ui.label(RichText::new("Spectrum of the receiver output").strong());
        let spec = dsp::spectrum_iq(&iq, FS);
        let limit = (doppler_hz(3.0, self.f0_hz) * 1.25).clamp(150.0, FS / 2.0);
        let pts: Vec<[f64; 2]> = spec.iter().filter(|p| p[0].abs() <= limit).map(|p| [p[0], p[1].max(-70.0)]).collect();
        plots::plot("doppler", 210.0).x_axis_label("Doppler frequency [Hz]   (right: approaching, left: moving away)").y_axis_label("level [dB]").include_y(-60.0).include_y(5.0).show(ui, |p| {
            p.line(plots::line("spectrum", pts, pal.trace[2]).fill(-70.0).fill_alpha(0.15));
            p.vline(egui_plot::VLine::new("predicted by the formula", fd).color(pal.trace[3]).style(egui_plot::LineStyle::dashed_dense()));
        });
        let peak = dsp::peak(&spec);
        let measured_speed = peak[0] * C / (2.0 * self.f0_hz);
        widgets::formula(ui, format!("measured peak: {:.0} Hz   ->   speed = shift x c / (2 x frequency) = {:.2} m/s", peak[0], measured_speed));
    }

    fn apply_preset(&mut self, preset: &str) {
        match preset {
            "away" => self.approach_mps = -1.2,
            "low-band" => self.f0_hz = 5.8e9,
            "still" => self.approach_mps = 0.0,
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "radar-doppler",
        title: "Doppler radar: sensing motion",
        blurb: "A moving person changes the frequency of the echo. That tiny shift is how motion sensors work.",
        path: &["Radar"],
        make: || Box::new(RadarDoppler::default()),
        explain: Explanation {
            what: "A radar on the left sends a steady [[carrier]] into a 10 m room. A person walks back and forth; you can drag \
                them, or set their speed. The radar receives the echo and compares it with what it is sending.\n\n\
                The result of that comparison is the pair of slow waves in the left graph, called I and Q. The right graph shows \
                the same two numbers as an arrow that turns. At the bottom is the [[spectrum]] of that signal, with the dashed \
                line marking where the formula says the peak should be.",
            why: "The echo is the radar's own wave, delayed by the trip to the person and back. If the person stands still, the \
                delay is constant and the echo has exactly the transmitted [[frequency]]. But if the person walks towards the radar, \
                each wave peak has a slightly shorter trip than the one before, so the peaks arrive a little closer together: the \
                echo's frequency is slightly higher. Walking away makes it slightly lower. This is the [[Doppler shift]], the same \
                effect that changes the pitch of a passing siren.\n\n\
                The shift is tiny - about 190 Hz on a 24 billion Hz carrier at walking speed - far too small to measure directly. \
                So the radar mixes the echo with its own transmission, and what comes out is only the difference: a slow wave whose \
                frequency is the Doppler shift. Every half [[wavelength]] the person moves, that wave completes one cycle.\n\n\
                Why two outputs? A single output could not tell approaching from receding - both give the same slow wave. With two \
                mixers a quarter cycle apart ([[IQ]]), the pair behaves like an arrow that turns one way for approaching and the \
                other way for receding.\n\n\
                This is the whole principle of the microwave motion sensors in automatic lights and door openers: any movement \
                produces a signal; a still room produces none.",
            formulas: &[
                ("Doppler shift = 2 x speed x frequency / c", "The factor 2 is there because the wave travels to the person and back. At 24.125 GHz: about 161 Hz for every metre per second."),
                ("one cycle per half wavelength of movement", "At 24 GHz the wavelength is 12.4 mm, so 6.2 mm of movement is one full cycle. That is why radar can even sense breathing."),
            ],
            try_this: &[
                Experiment {
                    action: "Set the speed to a negative value, so the person walks away.",
                    watch: "the spectrum peak moving to the left of zero, and the arrow turning the other way. I and Q swap which one leads.",
                    preset: Some("away"),
                },
                Experiment {
                    action: "Switch the radar from 24.125 GHz to 5.8 GHz while the person keeps walking.",
                    watch: "the waves slowing down to about a quarter of the frequency. Same speed, longer wavelength, smaller shift.",
                    preset: Some("low-band"),
                },
                Experiment {
                    action: "Set the speed to exactly zero.",
                    watch: "I and Q becoming flat lines and the arrow standing still. A plain Doppler sensor cannot see a person who does not move.",
                    preset: Some("still"),
                },
            ],
            leaves_out: "The person is a single reflecting point moving straight along the line to the radar. A real body has arms \
                and legs moving at different speeds, and moving sideways gives almost no shift. The echo's strength is kept constant \
                here; in reality it falls off steeply with distance. Reflections from walls, which do not move and add a constant \
                offset, are left out.",
            refs: &["Doppler formula checked against the HB100 sensor datasheet: 19.49 Hz per km/h at 10.525 GHz."],
        },
    }
}
