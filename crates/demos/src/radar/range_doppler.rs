use crate::prelude::*;
use egui_plot::{PlotImage, PlotPoint, Points};
use sim_core::radar::{Fmcw, Target};
use sim_core::rng::Rng;

struct Mover {
    key: u32,
    range_m: f64,
    approach_mps: f64,
}

pub struct RangeDopplerDemo {
    radar: Fmcw,
    chirps: usize,
    movers: Vec<Mover>,
    noise: f64,
    clock: Clock,
    heat: heat::Heat,
    next_key: u32,
}

impl Default for RangeDopplerDemo {
    fn default() -> Self {
        Self {
            radar: Fmcw { f0_hz: 60e9, bandwidth_hz: 1e9, chirp_s: 128e-6, fs: 1e6 },
            chirps: 64,
            movers: vec![Mover { key: 0, range_m: 5.0, approach_mps: 1.4 }, Mover { key: 1, range_m: 12.0, approach_mps: -3.0 }],
            noise: 0.3,
            clock: Clock::default(),
            heat: heat::Heat::new("range-doppler"),
            next_key: 2,
        }
    }
}

impl Demo for RangeDopplerDemo {
    fn ui(&mut self, ui: &mut Ui, cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        let r = self.radar;
        let (max_range, v_max) = (r.max_range(), r.max_velocity());

        // Move everything; turn around at the ends.
        let dt = self.clock.tick(ui, cx.dt);
        for m in &mut self.movers {
            m.range_m -= m.approach_mps * dt;
            if m.range_m < 0.8 || m.range_m > max_range * 0.95 {
                m.range_m = m.range_m.clamp(0.8, max_range * 0.95);
                m.approach_mps = -m.approach_mps;
            }
        }

        widgets::group(ui, "Moving objects", |ui| {
            let mut remove = None;
            for (i, m) in self.movers.iter_mut().enumerate() {
                ui.push_id(m.key, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(format!("object {}", i + 1)).color(pal.trace[i % 6]).strong());
                        widgets::wrap(ui, "distance", |ui| {
                            ui.add(egui::Slider::new(&mut m.range_m, 0.8..=max_range * 0.95).text("distance").suffix(" m").fixed_decimals(1))
                                .on_hover_text("How far the object is from the radar. It moves the object's spot left or right on the map.")
                        });
                        widgets::wrap(ui, "speed", |ui| {
                            ui.add(egui::Slider::new(&mut m.approach_mps, -8.0..=8.0).text("speed towards radar").suffix(" m/s").fixed_decimals(1))
                                .on_hover_text("Positive = approaching. Walking is about 1.4 m/s, running 3 to 5 m/s.")
                        });
                        if ui.small_button("Remove").clicked() {
                            remove = Some(i);
                        }
                    });
                });
            }
            if let Some(i) = remove {
                self.movers.remove(i);
            }
            if self.movers.len() < 5 && ui.button("Add object").clicked() {
                self.movers.push(Mover { key: self.next_key, range_m: 8.0, approach_mps: 0.0 });
                self.next_key += 1;
            }
        });
        widgets::group(ui, "The radar", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Chirps per measurement:");
                for n in [16usize, 32, 64, 128] {
                    ui.selectable_value(&mut self.chirps, n, format!("{n}")).on_hover_text("More chirps = longer measurement = finer speed resolution.");
                }
            });
            ui.add(egui::Slider::new(&mut self.noise, 0.0..=2.0).text("receiver noise").fixed_decimals(2))
                .on_hover_text("Random noise added to the received signal. It shows up as speckle across the whole map.");
        });
        self.clock.controls(ui);

        let targets: Vec<Target> = self.movers.iter().map(|m| Target { range_m: m.range_m, approach_mps: m.approach_mps, amplitude: 1.0 }).collect();
        let map = r.range_doppler(&targets, self.chirps, self.noise, &mut Rng::new(self.clock.t.to_bits()));
        // Texture rows run top to bottom; put the fastest approaching speed at the top.
        let (w, h) = (map.n_range, map.n_doppler);
        self.heat.update(ui.ctx(), w, h, false, |i| {
            let (row, col) = (h - 1 - i / w, i % w);
            heat::ramp((map.data[row * w + col] + 55.0) / 55.0)
        });

        ui.label(RichText::new("Range-Doppler map: every bright spot is one object").strong());
        plots::plot("map", 380.0)
            .x_axis_label("distance [m]")
            .y_axis_label("speed towards the radar [m/s]")
            .include_x(0.0)
            .include_x(max_range)
            .include_y(-v_max)
            .include_y(v_max)
            .show(ui, |p| {
                if let Some(id) = self.heat.id() {
                    p.image(PlotImage::new("echo strength", id, PlotPoint::new(max_range / 2.0, 0.0), vec2(max_range as f32, 2.0 * v_max as f32)));
                }
                for (i, m) in self.movers.iter().enumerate() {
                    p.points(
                        Points::new(format!("object {} really is here", i + 1), vec![[m.range_m, m.approach_mps]])
                            .shape(egui_plot::MarkerShape::Circle)
                            .filled(false)
                            .radius(11.0)
                            .color(pal.trace[i % 6]),
                    );
                }
            });
        widgets::formula(
            ui,
            format!(
                "range resolution = c / (2 x bandwidth) = {}        speed resolution = wavelength / (2 x chirps x chirp time) = {}\n\
                 fastest unambiguous speed = wavelength / (4 x chirp time) = {}        one measurement takes {}",
                si(r.range_resolution(), "m"),
                si(r.velocity_resolution(self.chirps), "m/s"),
                si(v_max, "m/s"),
                si(self.chirps as f64 * r.chirp_s, "s"),
            ),
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        match preset {
            "same-range" => {
                self.movers = vec![Mover { key: 0, range_m: 8.0, approach_mps: 2.0 }, Mover { key: 1, range_m: 8.0, approach_mps: -2.0 }];
                self.clock.running = false;
            }
            "few-chirps" => {
                self.chirps = 16;
                self.movers = vec![Mover { key: 0, range_m: 6.0, approach_mps: 1.0 }, Mover { key: 1, range_m: 6.3, approach_mps: 2.2 }];
                self.clock.running = false;
            }
            "noisy" => self.noise = 2.0,
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "radar-range-doppler",
        title: "Range-Doppler map: distance and speed at once",
        blurb: "Many chirps in a row let a radar separate objects by how far away they are and by how fast they move.",
        path: &["Radar"],
        make: || Box::new(RangeDopplerDemo::default()),
        explain: Explanation {
            what: "This is the picture a modern radar sensor computes many times per second. Left to right is distance from the \
                radar. Up and down is speed: the upper half is objects approaching, the lower half objects moving away, the middle \
                line is standing still. Brightness is echo strength.\n\n\
                The hollow circles mark where the objects really are, so you can check the radar's answer. The objects keep \
                moving, so the spots drift sideways - faster ones drift faster.",
            why: "One [[chirp]] gives distance (previous demo). But two objects at the same distance fall on the same peak, and a \
                single chirp says nothing about speed.\n\n\
                The fix is to send many chirps in quick succession and compare them. Between one chirp and the next, a moving \
                object has moved a tiny bit - far less than the [[range resolution]], so its peak stays in the same place. But the \
                [[phase]] of that peak changes, because phase is sensitive to movements of a fraction of a [[wavelength]], which at \
                60 GHz is a fraction of 5 mm. An object moving steadily makes the phase advance by the same amount from chirp to \
                chirp. That steady rotation is a frequency - the [[Doppler shift]] again.\n\n\
                So the radar does a second [[FFT]], this time across the chirps, separately for every distance. The result is this \
                two-dimensional map. Objects that share a distance but differ in speed land in different rows.\n\n\
                This is what makes radar good at presence detection: a wall, a chair and a person may all be 3 m away, but only \
                the person is not on the zero-speed line - and even someone sitting still shows up through breathing.",
            formulas: &[
                ("speed resolution = wavelength / (2 x number of chirps x chirp time)", "Watch longer (more chirps) to tell speeds apart more finely."),
                ("fastest speed = wavelength / (4 x chirp time)", "Faster objects appear at a wrong speed, like wagon wheels turning backwards in a film. Shorter chirps raise the limit."),
            ],
            try_this: &[
                Experiment {
                    action: "Put two objects at the same distance, one approaching at 2 m/s and one leaving at 2 m/s.",
                    watch: "two clearly separate spots, one above the other. A single chirp would have shown just one object.",
                    preset: Some("same-range"),
                },
                Experiment {
                    action: "Place two objects close together with similar speeds, then switch between 16 and 128 chirps.",
                    watch: "the spots stretched tall and overlapping with 16 chirps, sharp and separate with 128. More chirps = finer speed resolution.",
                    preset: Some("few-chirps"),
                },
                Experiment {
                    action: "Raise the receiver noise to the maximum.",
                    watch: "the background filling with speckle while the objects still stand out. Adding up many chirps lifts real echoes above the noise.",
                    preset: Some("noisy"),
                },
            ],
            leaves_out: "Objects are single points with equal echo strength at any distance, moving straight towards or away from \
                the radar. With one antenna there is no direction information: an object to the left and one to the right at the \
                same distance and speed are indistinguishable (that needs several receive antennas). Stationary clutter such as \
                walls is not simulated.",
            refs: &["Texas Instruments SPYY005 and SWRA553: velocity resolution and maximum unambiguous velocity (the chirp time includes any idle time)."],
        },
    }
}
