use crate::prelude::*;
use sim_core::radio::{Antenna as Model, dipole_field};
use std::f64::consts::{FRAC_PI_2, TAU};

pub struct Antenna {
    a: Model,
    phase_deg: f64,
    receiver_deg: f64,
}

impl Default for Antenna {
    fn default() -> Self {
        Self { a: Model { single: true, spacing_wl: 0.5, phase: 0.0 }, phase_deg: 0.0, receiver_deg: 30.0 }
    }
}

/// Draw a polar pattern: `r(angle)` in 0..=1, angle 0 pointing right, counter-clockwise.
fn polar(ui: &mut Ui, title: &str, r: impl Fn(f64) -> f64, marker: Option<f64>, wire: Option<&[(Pos2, Pos2)]>) {
    let pal = theme::palette(ui);
    ui.vertical(|ui| {
        ui.label(RichText::new(title).strong());
        let (rect, _) = ui.allocate_exact_size(vec2(250.0, 250.0), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        let c = rect.center();
        let scale = 110.0;
        painter.rect_filled(rect, 4.0, pal.canvas);
        for ring in [0.5, 1.0] {
            painter.circle_stroke(c, scale * ring, (0.6, pal.grid));
        }
        painter.line_segment([c - vec2(scale, 0.0), c + vec2(scale, 0.0)], (0.6, pal.grid));
        painter.line_segment([c - vec2(0.0, scale), c + vec2(0.0, scale)], (0.6, pal.grid));
        let pts: Vec<Pos2> = (0..=360).map(|k| k as f64 * TAU / 360.0).map(|a| c + vec2((a.cos() * r(a) * scale as f64) as f32, -(a.sin() * r(a) * scale as f64) as f32)).collect();
        // Fill as a fan of thin triangles from the centre: the pattern need not be convex.
        let fill = pal.trace[0].gamma_multiply(0.18);
        let mut mesh = egui::Mesh::default();
        mesh.colored_vertex(c, fill);
        for p in &pts {
            mesh.colored_vertex(*p, fill);
        }
        for k in 1..pts.len() as u32 {
            mesh.add_triangle(0, k, k + 1);
        }
        painter.add(egui::Shape::mesh(mesh));
        painter.add(egui::Shape::line(pts, (2.0, pal.trace[0])));
        for (a, b) in wire.unwrap_or(&[]) {
            painter.line_segment([c + a.to_vec2(), c + b.to_vec2()], (4.0, pal.warn));
        }
        if let Some(m) = marker {
            let p = c + vec2((m.cos() * scale as f64) as f32, -(m.sin() * scale as f64) as f32);
            painter.line_segment([c, p], (1.0, pal.dim));
            painter.circle_filled(p, 6.0, pal.trace[3]);
        }
    });
}

impl Demo for Antenna {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "The antenna and where the receiver is", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut self.a.single, true, "one dipole").on_hover_text("A straight wire half a wavelength long (6.1 cm at 2.45 GHz), standing upright, fed in the middle.");
                ui.selectable_value(&mut self.a.single, false, "two dipoles side by side").on_hover_text("Two such wires next to each other, both fed from the same transmitter.");
                widgets::wrap(ui, "rx", |ui| ui.add(egui::Slider::new(&mut self.receiver_deg, 0.0..=360.0).text("receiver direction").suffix(" °").fixed_decimals(0)).on_hover_text("Where the receiver is, seen from above. 0 ° is to the right."));
            });
            ui.add_enabled_ui(!self.a.single, |ui| {
                ui.horizontal_wrapped(|ui| {
                    widgets::wrap(ui, "spacing", |ui| ui.add(egui::Slider::new(&mut self.a.spacing_wl, 0.1..=1.5).text("distance between them").suffix(" wavelengths").fixed_decimals(2)).on_hover_text("How far apart the two wires are, measured in wavelengths. At 2.45 GHz one wavelength is 12.2 cm."));
                    widgets::wrap(ui, "phase", |ui| ui.add(egui::Slider::new(&mut self.phase_deg, -180.0..=180.0).text("the second is fed later by").suffix(" °").fixed_decimals(0)).on_hover_text("A delay in the feed of the second wire, as a fraction of one oscillation: 180 ° is half a period."));
                });
            });
        });
        self.a.phase = self.phase_deg.to_radians();
        let a = self.a;
        let d = a.directivity();
        let rx = self.receiver_deg.to_radians();
        let gain = a.gain_dbi(rx, d);
        let top_max = (0..720).map(|j| a.field(FRAC_PI_2, j as f64 * TAU / 720.0)).fold(0.0f64, f64::max).max(1e-12);

        ui.horizontal_wrapped(|ui| {
            // Side view: the plane that contains the wire. Angle 0 = horizontal, 90 = up the wire.
            let wire = [(pos2(0.0, -24.0), pos2(0.0, 24.0))];
            polar(ui, "Seen from the side (one wire, upright)", |ang| dipole_field(FRAC_PI_2 - ang).abs(), None, Some(&wire));
            let dots: Vec<(Pos2, Pos2)> = if a.single {
                vec![(pos2(-3.0, 0.0), pos2(3.0, 0.0))]
            } else {
                let half = (a.spacing_wl * 55.0 / 1.5 + 4.0) as f32;
                vec![(pos2(-half - 3.0, 0.0), pos2(-half + 3.0, 0.0)), (pos2(half - 3.0, 0.0), pos2(half + 3.0, 0.0))]
            };
            polar(ui, "Seen from above, with the receiver", |ang| a.field(FRAC_PI_2, ang) / top_max, Some(rx), Some(&dots));
        });
        widgets::key(ui, &[(pal.trace[0], "how strongly the antenna radiates in each direction (distance from the centre)"), (pal.warn, "the wire(s)"), (pal.trace[3], "the receiver")]);

        let rel_db = gain - 10.0 * d.log10();
        widgets::formula(
            ui,
            format!(
                "half-wave dipole, at angle theta from the wire: field = cos(90 ° x cos theta) / sin theta\n\
                 directivity = strongest / average over all directions = {:.2}  ->  10 x log10({:.2}) = {:.2} dBi\n\
                 towards the receiver ({:.0} °): {:+.1} dBi, that is {:+.1} dB compared with the best direction",
                d,
                d,
                10.0 * d.log10(),
                self.receiver_deg,
                gain,
                rel_db,
            ),
        );
        widgets::status(
            ui,
            rel_db > -3.0,
            if rel_db > -3.0 {
                "the receiver is inside the main beam".to_owned()
            } else if rel_db < -20.0 {
                format!("the receiver sits in a gap of the pattern: {:.0} dB weaker than in the best direction, less than a hundredth of the power", -rel_db)
            } else {
                format!("the receiver is off the main beam: {:.0} dB weaker than in the best direction", -rel_db)
            },
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "pair" => (self.a.single, self.receiver_deg) = (false, 90.0),
            "null" => (self.a.single, self.receiver_deg) = (false, 0.0),
            "steer" => (self.a.single, self.a.spacing_wl, self.phase_deg, self.receiver_deg) = (false, 0.25, 90.0, 0.0),
            "wide" => (self.a.single, self.a.spacing_wl, self.receiver_deg) = (false, 1.5, 90.0),
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "antenna-pattern",
        title: "Antennas and radiation patterns",
        blurb: "No antenna sends equally in all directions. See the shape of its radiation, and how two antennas together can aim it.",
        path: &["Wireless", "Radio basics"],
        make: || Box::new(Antenna::default()),
        explain: Explanation {
            what: "Each round picture shows how strongly an [[antenna]] radiates in each direction: the further the blue line is \
                from the centre, the stronger the radiation that way. The left picture looks at a single upright wire from \
                the side; the right one looks at the antenna from above, with a red dot for the receiver.\n\n\
                Choose \"two dipoles\" to put a second wire beside the first and watch the top view change shape.",
            why: "The simplest real antenna is a [[dipole]]: a straight wire half a [[wavelength]] long, fed in the middle - about \
                6 cm at 2.4 GHz. The current flowing up and down the wire makes the wave, and a wave is made by the movement \
                of charge across the line of sight. Looking along the wire from its end, there is no movement across your line \
                of sight at all, so nothing is radiated in that direction. Looking at it from the side, all of the movement is \
                across your view, so that is where it radiates most. The result is the figure-eight on the left: in three \
                dimensions a doughnut around the wire.\n\n\
                Seen from above, the doughnut is a circle: the upright dipole sends equally in all horizontal directions. \
                Because nothing goes up or down, what would have gone there is sent sideways instead, so sideways it is 1.64 \
                times stronger than an imaginary antenna that spreads its power evenly over a sphere. Engineers write that \
                as 2.15 [[dBi]]. This is what antenna gain means: not extra power, only power taken from some directions and \
                given to others.\n\n\
                Two antennas fed from the same transmitter interfere, like the waves in the \"Waves in space\" demo. In some \
                directions their waves arrive in step and add up; in others one arrives half a period late and they cancel. \
                Moving them apart or delaying one of them moves those directions around. With the right delay, the beam \
                points wherever you want - without moving anything. This is how phased-array radars and the antennas of \
                mobile-phone base stations aim.\n\n\
                For a small Bluetooth device the lesson is more modest: its antenna, and the way it sits in a housing or a \
                hand, leaves some directions much weaker than others.",
            formulas: &[
                ("half-wave dipole: field = cos(90 ° x cos theta) / sin theta", "theta is the angle from the wire. At 90 ° (sideways) the field is 1, at 0 ° (along the wire) it is 0."),
                ("gain in dBi = 10 x log10(directivity)", "A half-wave dipole: 10 x log10(1.64) = 2.15 dBi. Two side by side, half a wavelength apart: about 6 dBi."),
            ],
            try_this: &[
                Experiment {
                    action: "Put two dipoles side by side, half a wavelength apart, and look at the receiver at 90 °.",
                    watch: "the circle squeezed into two lobes pointing up and down the picture. The gain in that direction rises to about 6 dBi.",
                    preset: Some("pair"),
                },
                Experiment {
                    action: "Keep the pair and move the receiver to 0 °, along the line through both wires.",
                    watch: "the receiver sitting in a hole of the pattern. Here the wave of one wire arrives exactly half a period after the other's, and they cancel.",
                    preset: Some("null"),
                },
                Experiment {
                    action: "Place the two a quarter wavelength apart and feed the second a quarter period (90 °) later.",
                    watch: "a heart-shaped pattern that sends everything to one side and nothing to the other. Change the delay to -90 ° and it turns round: steering without moving anything.",
                    preset: Some("steer"),
                },
                Experiment {
                    action: "Move them 1.5 wavelengths apart.",
                    watch: "the pattern breaking into many narrow lobes with gaps between them. Wide spacing makes sharp beams, but in many directions at once.",
                    preset: Some("wide"),
                },
            ],
            leaves_out: "The antennas are ideal: thin, lossless, in free space, far from the ground and from each other's influence \
                (two real dipoles this close change each other's currents, which shifts these numbers a little). Patterns \
                are shown only in two planes. Real small devices use printed or chip antennas whose patterns are far less \
                regular, and the hand, body or housing around them changes the pattern again. A real antenna also loses some \
                power as heat, so its gain is a little lower than its directivity.",
            refs: &[
                "Half-wave dipole pattern and directivity 1.64 (2.15 dBi); two-element array factor: C. A. Balanis, \"Antenna Theory: Analysis and Design\", chapters 4 and 6.",
                "Directivities computed in sim-core by numerical integration over all directions, and the two-dipole value (3.96) checked by an independent integration.",
            ],
        },
    }
}
