use crate::prelude::*;
use sim_core::radar::{Fmcw, Target};
use sim_core::rng::Rng;

pub struct RadarFmcw {
    radar: Fmcw,
    ranges: [f64; 2],
    second_target: bool,
    noise: f64,
}

impl Default for RadarFmcw {
    fn default() -> Self {
        Self { radar: Fmcw { f0_hz: 60e9, bandwidth_hz: 1e9, chirp_s: 128e-6, fs: 1e6 }, ranges: [4.0, 9.0], second_target: false, noise: 0.05 }
    }
}

impl RadarFmcw {
    fn targets(&self) -> Vec<Target> {
        let n = if self.second_target { 2 } else { 1 };
        self.ranges[..n].iter().map(|r| Target { range_m: *r, approach_mps: 0.0, amplitude: 1.0 }).collect()
    }
}

impl Demo for RadarFmcw {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        let r = self.radar;
        let max_range = r.max_range();

        widgets::group(ui, "The chirp", |ui| {
            ui.add(widgets::si_slider(&mut self.radar.bandwidth_hz, 100e6..=4e9, "sweep bandwidth", "Hz").logarithmic(true))
                .on_hover_text("How far the frequency climbs during one chirp. 24 GHz sensors may use 250 MHz; 60 GHz sensors several GHz.");
            ui.add(widgets::si_slider(&mut self.radar.chirp_s, 32e-6..=512e-6, "chirp duration", "s").logarithmic(true))
                .on_hover_text("How long one sweep takes.");
            ui.add(egui::Slider::new(&mut self.noise, 0.0..=1.0).text("receiver noise").fixed_decimals(2));
        });
        widgets::group(ui, "The objects (drag them in the picture)", |ui| {
            ui.checkbox(&mut self.second_target, "second object").on_hover_text("Add a second reflecting object to see whether the radar can tell them apart.");
        });

        let n_targets = if self.second_target { 2 } else { 1 };
        let world = egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(21.0, 2.4));
        scene::scene(ui, "range", world, 110.0, |s| {
            let radar = s.pos(pos2(0.5, 1.2));
            s.painter.rect_filled(egui::Rect::from_center_size(radar, vec2(14.0, 34.0)), 3.0, pal.trace[0]);
            let limit = s.pos(pos2(0.5 + max_range.min(20.0) as f32, 1.2));
            s.painter.line_segment([limit + vec2(0.0, -30.0), limit + vec2(0.0, 30.0)], (1.0, pal.dim));
            s.painter.text(limit + vec2(-4.0, 30.0), egui::Align2::RIGHT_BOTTOM, format!("max range {:.1} m", max_range), egui::FontId::proportional(12.0), pal.dim);
            for i in 0..n_targets {
                let mut p = pos2(self.ranges[i] as f32 + 0.5, 1.2);
                let resp = s.handle(("target", i), &mut p, 11.0);
                if resp.dragged() {
                    self.ranges[i] = (p.x - 0.5).clamp(0.3, 20.0) as f64;
                }
                let c = s.pos(pos2(self.ranges[i] as f32 + 0.5, 1.2));
                s.painter.circle_filled(c, 10.0, pal.trace[3 + i]);
                s.painter.text(c + vec2(0.0, -14.0), egui::Align2::CENTER_BOTTOM, format!("{:.2} m", self.ranges[i]), egui::FontId::proportional(13.0), pal.text);
            }
        });

        let targets = self.targets();
        let slope = r.slope();
        widgets::formula(
            ui,
            format!(
                "slope = bandwidth / duration = {}/µs        beat frequency = 2 x distance x slope / c = {}\n\
                 range resolution = c / (2 x bandwidth) = {}        max range (set by the receiver's {} sample rate) = {}",
                si(slope * 1e-6, "Hz"),
                targets.iter().map(|t| si(r.beat_hz(t.range_m), "Hz")).collect::<Vec<_>>().join("  and  "),
                si(r.range_resolution(), "m"),
                si(r.fs, "Hz"),
                si(max_range, "m"),
            ),
        );

        ui.label(RichText::new("1. Frequency over time, zoomed in to the first microsecond of a chirp").strong());
        plots::plot("chirp", 230.0).x_axis_label("time [ns]").y_axis_label("frequency above start [MHz]").include_y(0.0).show(ui, |p| {
            let t_end = 1e-6;
            let ramp = |delay: f64| -> Vec<[f64; 2]> { vec![[delay * 1e9, 0.0], [t_end * 1e9, slope * (t_end - delay) / 1e6]] };
            p.line(plots::line("transmitted", ramp(0.0), pal.trace[0]).width(2.5));
            for (i, t) in targets.iter().enumerate() {
                p.line(plots::line(&format!("echo from {:.2} m", t.range_m), ramp(2.0 * t.range_m / C), pal.trace[3 + i]));
            }
        });
        ui.label(RichText::new("The echo is the same ramp, arriving late. At any instant it is a fixed number of hertz below the transmitted ramp: that gap is the beat frequency.").small().color(pal.dim));

        let beat = r.beat(&targets, 0, self.noise, &mut Rng::new(11));
        ui.label(RichText::new("2. Receiver output during one whole chirp (transmitted and echo mixed together)").strong());
        let re: Vec<f64> = beat.iter().map(|c| c.re).collect();
        plots::plot("beat", 170.0).x_axis_label("time [µs]").y_axis_label("level").show(ui, |p| {
            p.line(plots::line("beat signal", plots::series(&re, 0.0, 1e6 / r.fs), pal.trace[1]));
        });

        ui.label(RichText::new("3. Its spectrum, with the frequency axis relabelled as distance").strong());
        let profile = r.range_profile(&beat);
        let pts: Vec<[f64; 2]> = profile.iter().enumerate().map(|(k, v)| [r.range_of_bin(k), v.max(-70.0)]).collect();
        plots::plot("profile", 220.0).x_axis_label("distance [m]").y_axis_label("level [dB]").include_y(-60.0).include_y(5.0).show(ui, |p| {
            p.line(plots::line("range profile", pts, pal.trace[2]).fill(-70.0).fill_alpha(0.15));
            for (i, t) in targets.iter().enumerate() {
                p.vline(egui_plot::VLine::new(format!("true distance {:.2} m", t.range_m), t.range_m).color(pal.trace[3 + i]).style(egui_plot::LineStyle::dashed_dense()));
            }
        });
        if self.second_target {
            let gap = (self.ranges[0] - self.ranges[1]).abs();
            let res = r.range_resolution();
            widgets::status(
                ui,
                gap >= 2.0 * res,
                format!("the objects are {} apart; the resolution is {} - {}", si(gap, "m"), si(res, "m"), if gap >= 2.0 * res { "two clear peaks" } else { "too close: the peaks merge into one" }),
            );
        }
    }

    fn apply_preset(&mut self, preset: &str) {
        match preset {
            "two-close" => {
                self.second_target = true;
                self.ranges = [4.0, 4.25];
            }
            "narrow" => {
                self.second_target = true;
                self.ranges = [4.0, 4.6];
                self.radar.bandwidth_hz = 250e6;
            }
            "far" => self.ranges[0] = 18.0,
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "radar-fmcw",
        title: "FMCW radar: measuring distance",
        blurb: "Sweep the frequency, and the delay of an echo turns into a tone. The pitch of the tone is the distance.",
        path: &["Radar"],
        make: || Box::new(RadarFmcw::default()),
        explain: Explanation {
            what: "The strip at the top is a 20 m range with one or two reflecting objects you can drag. Below it are the three \
                stages of an FMCW radar measurement. Graph 1: the transmitted frequency climbing, and each echo climbing the same \
                way a moment later. Graph 2: what the receiver outputs during one sweep. Graph 3: the [[spectrum]] of graph 2 - \
                each peak is an object, and its position on the axis is its distance.",
            why: "A Doppler radar only notices movement. To measure distance, the radar needs to know how long an echo took - \
                but radio waves travel 30 cm per nanosecond, so timing an echo directly needs extremely fast electronics.\n\n\
                FMCW (frequency-modulated continuous wave) radar uses a trick instead. It does not send a constant frequency: \
                it sweeps the frequency steadily upwards. One such sweep is a [[chirp]]. While an echo is on its way out and back, \
                the transmitter has already moved on to a higher frequency. So at every instant, the echo is a little lower in \
                frequency than what is being transmitted, and the further away the object, the bigger the gap.\n\n\
                Mixing the echo with the transmitted signal gives the difference between the two: a steady tone called the \
                [[beat frequency]]. A delay of a few nanoseconds, which is hard to measure, has become a tone of a few hundred \
                kilohertz, which is easy. An [[FFT]] sorts the receiver output by frequency, and each object shows up as its own peak.\n\n\
                How close can two objects be and still give two peaks? That depends on one thing only: how wide the sweep is. \
                This is the [[range resolution]], and it is why 60 GHz radars, which are allowed to sweep several gigahertz, can \
                resolve centimetres while 24 GHz sensors with 250 MHz manage about 60 cm.",
            formulas: &[
                ("beat frequency = 2 x distance x slope / c", "Slope is how fast the frequency climbs: bandwidth divided by chirp duration."),
                ("range resolution = c / (2 x bandwidth)", "1 GHz of sweep gives 15 cm. 4 GHz gives 3.75 cm. 250 MHz gives 60 cm."),
                ("max range = sample rate x c / (2 x slope)", "Far objects give high beat frequencies; the receiver can only sample so fast."),
            ],
            try_this: &[
                Experiment {
                    action: "Add the second object and drag it to about 25 cm from the first.",
                    watch: "graph 3: still two separate peaks with a 1 GHz sweep (resolution 15 cm) - only just.",
                    preset: Some("two-close"),
                },
                Experiment {
                    action: "With the objects 60 cm apart, reduce the sweep bandwidth to 250 MHz.",
                    watch: "the two peaks melting into one wide lump. The radar now reports a single object.",
                    preset: Some("narrow"),
                },
                Experiment {
                    action: "Drag an object out towards the maximum range.",
                    watch: "graph 2 wiggling faster and faster: more distance, higher beat frequency, until the receiver cannot keep up.",
                    preset: Some("far"),
                },
            ],
            leaves_out: "The objects do not move here; movement adds a Doppler shift on top of the beat frequency (next demo). \
                Every echo has the same strength regardless of distance, whereas a real echo from twice as far is sixteen times \
                weaker. The sweep is perfectly straight and there is no pause between chirps.",
            refs: &["Texas Instruments, \"The fundamentals of millimeter wave radar sensors\" (SPYY005): beat frequency, range resolution, maximum range."],
        },
    }
}
