use crate::prelude::*;
use sim_core::field::{COURANT, Field};

const W: usize = 240;
const H: usize = 160;
/// Size of one grid cell in metres. The whole view is 1.2 m x 0.8 m.
const CELL_M: f64 = 0.005;
const BORDER: usize = 22;

struct Source {
    key: u32,
    pos: Pos2,
    phase_deg: f32,
}

struct Wall {
    key: u32,
    a: Pos2,
    b: Pos2,
}

pub struct WaveField {
    field: Field,
    heat: heat::Heat,
    sources: Vec<Source>,
    walls: Vec<Wall>,
    f_hz: f64,
    gain: f32,
    steps_per_frame: usize,
    running: bool,
    next_key: u32,
}

impl Default for WaveField {
    fn default() -> Self {
        let mut d = Self {
            field: Field::new(W, H, BORDER),
            heat: heat::Heat::new("wave-field"),
            sources: Vec::new(),
            walls: Vec::new(),
            f_hz: 2.4e9,
            gain: 2.2,
            steps_per_frame: 3,
            running: true,
            next_key: 0,
        };
        d.layout("one");
        d
    }
}

impl WaveField {
    fn key(&mut self) -> u32 {
        self.next_key += 1;
        self.next_key
    }

    fn add_source(&mut self, pos: Pos2) {
        let key = self.key();
        self.sources.push(Source { key, pos, phase_deg: 0.0 });
    }

    fn add_wall(&mut self, a: Pos2, b: Pos2) {
        let key = self.key();
        self.walls.push(Wall { key, a, b });
    }

    /// Seconds of simulated time per solver step.
    fn dt(&self) -> f64 {
        COURANT as f64 * CELL_M / C
    }

    fn rasterize_walls(&mut self) {
        self.field.clear_walls();
        for w in &self.walls {
            let n = (w.a.distance(w.b).ceil() as usize * 2).max(1);
            for i in 0..=n {
                let p = w.a + (w.b - w.a) * (i as f32 / n as f32);
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    self.field.set_wall(p.x as usize + dx, p.y as usize + dy, true);
                }
            }
        }
    }

    /// Run the solver for `steps` time steps with all antennas transmitting.
    fn advance(&mut self, steps: usize) {
        self.rasterize_walls();
        for _ in 0..steps {
            let t = self.field.steps as f64 * self.dt();
            // Fade the antennas in over the first cycles to avoid a hard click.
            let ramp = (self.field.steps as f32 / 60.0).min(1.0);
            for s in &self.sources {
                let phase = std::f64::consts::TAU * self.f_hz * t + (s.phase_deg as f64).to_radians();
                self.field.inject(s.pos.x as usize, s.pos.y as usize, 0.35 * ramp * phase.sin() as f32);
            }
            self.field.step();
        }
    }

    fn layout(&mut self, name: &str) {
        self.walls.clear();
        self.sources.clear();
        self.field.clear();
        match name {
            "two-sources" => {
                self.add_source(pos2(70.0, 62.0));
                self.add_source(pos2(70.0, 98.0));
            }
            "mirror" => {
                self.add_source(pos2(70.0, 80.0));
                self.add_wall(pos2(170.0, 25.0), pos2(170.0, 135.0));
            }
            "slit" => {
                self.add_source(pos2(50.0, 80.0));
                self.add_wall(pos2(120.0, 0.0), pos2(120.0, 72.0));
                self.add_wall(pos2(120.0, 88.0), pos2(120.0, 160.0));
            }
            "double-slit" => {
                self.add_source(pos2(45.0, 80.0));
                self.add_wall(pos2(110.0, 0.0), pos2(110.0, 58.0));
                self.add_wall(pos2(110.0, 70.0), pos2(110.0, 90.0));
                self.add_wall(pos2(110.0, 102.0), pos2(110.0, 160.0));
            }
            _ => self.add_source(pos2(60.0, 80.0)),
        }
        // Start with the waves already spread out instead of an empty picture.
        self.advance(450);
    }
}

impl Demo for WaveField {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        let lambda_m = C / self.f_hz;

        widgets::group(ui, "Scene", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Start from:");
                for (label, name) in [("One antenna", "one"), ("Two antennas", "two-sources"), ("Metal plate", "mirror"), ("Gap in a wall", "slit"), ("Two gaps", "double-slit")] {
                    if ui.button(label).clicked() {
                        self.layout(name);
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                if ui.button("Add antenna").on_hover_text("Add another transmitting antenna. All antennas send the same frequency.").clicked() && self.sources.len() < 6 {
                    self.add_source(pos2(120.0, 40.0));
                }
                if ui.button("Add metal wall").on_hover_text("Add a wall that reflects radio waves completely. Drag its two ends.").clicked() && self.walls.len() < 8 {
                    self.add_wall(pos2(150.0, 50.0), pos2(150.0, 110.0));
                }
                if ui.button("Clear waves").on_hover_text("Wipe the field and let the waves build up again.").clicked() {
                    self.field.clear();
                }
                let label = if self.running { "Pause" } else { "Play" };
                if ui.button(label).clicked() {
                    self.running = !self.running;
                }
            });
            ui.label(RichText::new("Drag antennas (circles) and wall ends (squares). Right-click one to remove it.").small().color(pal.dim));
        });
        widgets::group(ui, "The wave", |ui| {
            ui.add(widgets::si_slider(&mut self.f_hz, 1e9..=6e9, "frequency", "Hz").logarithmic(true))
                .on_hover_text("Frequency of all antennas. Higher frequency means shorter wavelength.");
            if self.sources.len() == 2 {
                ui.add(egui::Slider::new(&mut self.sources[1].phase_deg, -180.0..=180.0).text("phase of antenna 2").suffix(" deg").fixed_decimals(0))
                    .on_hover_text("Delays the second antenna relative to the first. This steers the direction in which the two waves add up.");
            }
            ui.add(egui::Slider::new(&mut self.gain, 0.3..=4.0).logarithmic(true).text("brightness").fixed_decimals(1))
                .on_hover_text("Only changes how strongly the picture is coloured, so weak waves far from the antenna become visible.");
            ui.add(egui::Slider::new(&mut self.steps_per_frame, 1..=8).text("simulation speed"))
                .on_hover_text("Solver steps per screen frame.");
        });
        widgets::formula(
            ui,
            format!(
                "wavelength = c / frequency = {}      view = 1.2 m x 0.8 m      slow motion: about {:.0e} times",
                si(lambda_m, "m"),
                1.0 / (self.dt() * self.steps_per_frame as f64 * 60.0)
            ),
        );

        // Advance the simulation.
        if self.running {
            self.advance(self.steps_per_frame);
            ui.ctx().request_repaint();
        }

        // Draw.
        let (values, gain, dark) = (self.field.values(), self.gain, pal.dark);
        let field = &self.field;
        let wall_color = if dark { Color32::from_gray(225) } else { Color32::from_gray(40) };
        self.heat.update(ui.ctx(), W, H, true, |i| if field.is_wall(i % W, i / W) { wall_color } else { heat::diverging(values[i] * gain, dark) });

        let world = egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(W as f32, H as f32));
        let mut remove_source = None;
        let mut remove_wall = None;
        scene::scene(ui, "field", world, 560.0, |s| {
            self.heat.paint(&s.painter, s.rect);
            // Scale bar: 10 cm.
            let (a, b) = (s.pos(pos2(12.0, 150.0)), s.pos(pos2(12.0 + (0.10 / CELL_M) as f32, 150.0)));
            s.painter.line_segment([a, b], (3.0, pal.text));
            s.painter.text(a + vec2(0.0, -6.0), egui::Align2::LEFT_BOTTOM, "10 cm", egui::FontId::proportional(13.0), pal.text);

            for (i, w) in self.walls.iter_mut().enumerate() {
                for (end, p) in [(0, &mut w.a), (1, &mut w.b)] {
                    let r = s.handle(("wall", w.key, end), p, 7.0);
                    let c = s.pos(*p);
                    s.painter.rect_filled(egui::Rect::from_center_size(c, vec2(11.0, 11.0)), 2.0, pal.trace[2]);
                    r.context_menu(|ui| {
                        if ui.button("Remove this wall").clicked() {
                            remove_wall = Some(i);
                            ui.close();
                        }
                    });
                }
            }
            for (i, src) in self.sources.iter_mut().enumerate() {
                let r = s.handle(("src", src.key), &mut src.pos, 9.0);
                let c = s.pos(src.pos);
                s.painter.circle_filled(c, 8.0, pal.trace[3]);
                s.painter.circle_stroke(c, 8.0, (1.5, pal.text));
                s.painter.text(c + vec2(11.0, -11.0), egui::Align2::LEFT_BOTTOM, format!("antenna {}", i + 1), egui::FontId::proportional(13.0), pal.text);
                r.context_menu(|ui| {
                    if ui.button("Remove this antenna").clicked() {
                        remove_source = Some(i);
                        ui.close();
                    }
                });
            }
        });
        if let Some(i) = remove_wall {
            self.walls.remove(i);
        }
        if let Some(i) = remove_source {
            self.sources.remove(i);
        }
        ui.label(RichText::new("Orange: field pushed one way. Blue: the other way. Background colour: nothing happening.").small().color(pal.dim));
    }

    fn apply_preset(&mut self, preset: &str) {
        if preset == "steer" {
            self.layout("two-sources");
            self.sources[1].phase_deg = 120.0;
            self.field.clear();
            self.advance(450);
        } else {
            self.layout(preset);
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "wave-field",
        title: "Waves in space",
        blurb: "Watch radio waves spread from an antenna, bounce off metal, bend through gaps and add up.",
        path: &["Wireless", "Radio basics"],
        make: || Box::new(WaveField::default()),
        explain: Explanation {
            what: "You are looking down on a table-sized area, 1.2 m wide. Each circle is an [[antenna]] sending a steady [[carrier]]. \
                The colours show the field at every point: orange where it is pushed one way, blue where it is pushed the other way. \
                The rings are wave peaks moving outwards; the distance between two orange rings is one [[wavelength]].\n\n\
                White (or black) lines are metal walls. Everything is draggable, and the waves react while you drag.",
            why: "An antenna shakes the electric field around it. That disturbance cannot stay put: each bit of space passes it on to \
                its neighbours, so it spreads in all directions at the speed of light. The simulation does exactly that - every cell \
                of a grid looks at its four neighbours, over and over.\n\n\
                Three things follow from this one rule. Metal forces the field to zero, so a wave that hits a wall has to turn back: \
                reflection. A wave that meets a gap spreads out again behind it as if the gap were a new antenna: this bending is \
                called diffraction, and it is why radio reaches around corners. And where two waves meet they simply add: \
                [[interference]]. Peak on peak gives a stronger signal, peak on trough gives nothing at all.\n\n\
                Interference is not a curiosity. It is why a Bluetooth link can drop when you move a device by a few centimetres \
                (the direct wave and a reflection cancel at that spot), and it is how radars and Wi-Fi routers steer a beam using \
                several antennas.",
            formulas: &[("wavelength = c / frequency", "2.4 GHz: 12.5 cm. 5.8 GHz: 5.2 cm. Compare with the 10 cm scale bar.")],
            try_this: &[
                Experiment {
                    action: "Choose \"Two antennas\" and look at the pattern to the right of them.",
                    watch: "bright beams with calm, dark lanes in between. In a dark lane the two waves always arrive exactly out of step and cancel.",
                    preset: Some("two-sources"),
                },
                Experiment {
                    action: "With two antennas, move the \"phase of antenna 2\" slider.",
                    watch: "the beams swinging to a new direction although no antenna moved. This is how a phased-array antenna steers.",
                    preset: Some("steer"),
                },
                Experiment {
                    action: "Choose \"Gap in a wall\", then raise the frequency.",
                    watch: "the wave behind the gap. At low frequency it fans out widely; at high frequency (short wavelength) it stays more of a beam.",
                    preset: Some("slit"),
                },
                Experiment {
                    action: "Choose \"Metal plate\" and drag the antenna slowly towards the plate.",
                    watch: "the stripes standing still between antenna and plate: the outgoing and reflected waves form a standing pattern of strong and dead spots.",
                    preset: Some("mirror"),
                },
            ],
            leaves_out: "This is a flat, two-dimensional world: real waves also spread up and down, which makes them fade faster with \
                distance. Walls here are perfect mirrors; real walls let part of the wave through and soak up some. Antennas radiate \
                equally in all directions, which no real antenna does. The edges of the view absorb waves to imitate open space, \
                and do so almost but not quite perfectly.",
            refs: &["Scalar wave equation on a grid (finite-difference time-domain method), Courant number 0.5."],
        },
    }
}
