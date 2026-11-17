use crate::prelude::*;
use egui_plot::{HLine, Polygon};
use sim_core::radar::{MOTION_THRESHOLD, PRESENCE_THRESHOLD_DB, Presence, doppler_hz, doppler_motion};
use sim_core::signals::{PIR_RATE_HZ, PIR_SECONDS, PIR_THRESHOLD_V, PirScene, PirTrace};

const F0: f64 = 24.125e9;
/// Amplitude that gets through a glass pane or thin plasterboard at 24 GHz (about -3 dB;
/// an example value - coated glass can block far more).
const PANE: f64 = 0.7;
const VIEW_M: f64 = 10.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Case {
    Across,
    Towards,
    Still,
    Glass,
}

const CASES: [(Case, &str); 4] = [(Case::Across, "walks across the room"), (Case::Towards, "walks straight at the sensor"), (Case::Still, "sits still, reading"), (Case::Glass, "walks past behind a glass door")];

fn walk(case: Case) -> PirScene {
    let base = PirScene { start: (-4.0, 5.0), heading_deg: 0.0, speed_mps: 1.0, zone_pitch_deg: 6.0, draught: 0.0, single_element: false };
    match case {
        Case::Across | Case::Glass => base,
        Case::Towards => PirScene { start: (0.0, 9.0), heading_deg: -90.0, speed_mps: 0.6, ..base },
        Case::Still => PirScene { start: (1.0, 3.0), speed_mps: 0.0, ..base },
    }
}

struct Seen {
    pir: PirTrace,
    motion: Vec<[f64; 3]>,
    /// Breathing rate found by the radar's presence mode.
    breathing: Option<f64>,
}

fn simulate(case: Case) -> Seen {
    let sc = walk(case);
    // Glass passes none of the far infrared a PIR sensor responds to: it sees an empty room.
    let pir = if case == Case::Glass { PirScene { start: (100.0, 5.0), speed_mps: 0.0, ..sc }.simulate() } else { sc.simulate() };
    let motion = doppler_motion(&sc, F0, if case == Case::Glass { PANE } else { 1.0 });
    let breathing = (case == Case::Still)
        .then(|| Presence { f0_hz: F0, person: true, breaths_per_min: 15.0, chest_mm: 4.0, beats_per_min: 72.0, heart_mm: 0.0, noise: 0.02 }.simulate(1))
        .filter(|t| t.breathing_peak[1] > PRESENCE_THRESHOLD_DB)
        .map(|t| t.breathing_peak[0]);
    Seen { pir, motion, breathing }
}

fn verdicts(s: &Seen) -> (String, String) {
    let pir = if s.pir.detections.is_empty() { "nothing" } else { "motion" };
    let radar = if s.motion.iter().any(|m| m[2] > MOTION_THRESHOLD) {
        "motion".to_owned()
    } else if let Some(rate) = s.breathing {
        format!("presence (breathing, {rate:.0}/min)")
    } else {
        "nothing".to_owned()
    };
    (pir.to_owned(), radar)
}

pub struct RadarVsPir {
    case: Case,
}

impl Default for RadarVsPir {
    fn default() -> Self {
        Self { case: Case::Across }
    }
}

impl Demo for RadarVsPir {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "A person in the room", |ui| {
            ui.horizontal_wrapped(|ui| {
                for (c, name) in CASES {
                    ui.selectable_value(&mut self.case, c, name).on_hover_text("Show this situation to both sensors.");
                }
            });
        });
        let seen = simulate(self.case);
        let sc = walk(self.case);

        let side = ui.available_width().min(520.0);
        let (rect, _) = ui.allocate_exact_size(vec2(side, side * 0.42), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, pal.canvas);
        let scale = rect.height() as f64 / VIEW_M;
        let origin = rect.center_bottom() - vec2(0.0, 8.0);
        let to_screen = |x: f64, y: f64| origin + vec2((x * scale) as f32, -(y * scale) as f32);
        if self.case == Case::Glass {
            painter.line_segment([to_screen(-VIEW_M, 3.5), to_screen(VIEW_M, 3.5)], (3.0, pal.trace[0].gamma_multiply(0.6)));
            painter.text(to_screen(-VIEW_M * 0.95, 3.5) + vec2(0.0, -4.0), egui::Align2::LEFT_BOTTOM, "glass", egui::FontId::proportional(12.0), pal.dim);
        }
        // Drawn from the walk itself: behind glass the PIR model is given an empty room.
        let (hx, hy) = (sc.heading_deg.to_radians().cos(), sc.heading_deg.to_radians().sin());
        let path: Vec<Pos2> = seen.motion.iter().map(|m| to_screen(sc.start.0 + hx * sc.speed_mps * m[0], sc.start.1 + hy * sc.speed_mps * m[0])).collect();
        painter.add(egui::Shape::line(path.clone(), (2.5, pal.trace[3])));
        if let (Some(a), Some(b)) = (path.first(), path.last()) {
            painter.circle_filled(*a, 6.0, pal.trace[3]);
            if sc.speed_mps > 0.0 {
                let d = (*b - path[path.len().saturating_sub(5)]).normalized() * 10.0;
                painter.arrow(*b - d, d, (2.5, pal.trace[3]));
            }
        }
        painter.circle_filled(origin, 6.0, pal.text);
        painter.text(origin + vec2(10.0, -2.0), egui::Align2::LEFT_BOTTOM, "PIR sensor and radar", egui::FontId::proportional(12.0), pal.text);
        widgets::key(ui, &[(pal.trace[3], "the person: 8 seconds of walking, or a dot if sitting")]);

        let shade = |p: &mut egui_plot::PlotUi, spans: &[(f64, f64)], lo: f64, hi: f64| {
            for &(a, b) in spans {
                let fill = pal.good.gamma_multiply(0.18);
                p.polygon(Polygon::new("detected", plots::bar(a, b, lo, hi)).fill_color(fill).stroke((0.0, fill)));
            }
        };
        ui.label(RichText::new("PIR sensor: amplified signal").strong());
        plots::plot_bare("pir", 130.0).x_axis_label("time [s]").y_axis_label("PIR [V]").include_x(0.0).include_x(PIR_SECONDS).include_y(-1.5).include_y(1.5).show(ui, |p| {
            shade(p, &seen.pir.detections, -1.5, 1.5);
            for th in [PIR_THRESHOLD_V, -PIR_THRESHOLD_V] {
                p.hline(HLine::new("threshold", th).color(pal.warn).style(egui_plot::LineStyle::dashed_dense()));
            }
            p.line(plots::line("PIR", plots::series(&seen.pir.amplified_v, 0.0, 1.0 / PIR_RATE_HZ), pal.trace[2]).width(2.0));
        });
        ui.label(RichText::new("Radar: strength of the Doppler (motion) signal").strong());
        let mut spans = Vec::new();
        let mut start = None;
        for m in &seen.motion {
            match (m[2] > MOTION_THRESHOLD, start) {
                (true, None) => start = Some(m[0]),
                (false, Some(s)) => {
                    spans.push((s, m[0]));
                    start = None;
                }
                _ => {}
            }
        }
        if let Some(s) = start {
            spans.push((s, PIR_SECONDS));
        }
        plots::plot_bare("radar", 130.0).x_axis_label("time [s]").y_axis_label("motion level").include_x(0.0).include_x(PIR_SECONDS).include_y(0.0).include_y(1.2).show(ui, |p| {
            shade(p, &spans, 0.0, 1.2);
            p.hline(HLine::new("threshold", MOTION_THRESHOLD).color(pal.warn).style(egui_plot::LineStyle::dashed_dense()));
            p.line(plots::line("radar", seen.motion.iter().map(|m| [m[0], m[2].min(1.2)]).collect(), pal.trace[0]).width(2.0));
        });
        widgets::key(ui, &[(pal.trace[2], "PIR after its amplifier"), (pal.trace[0], "radar motion signal"), (pal.warn, "dashed: thresholds"), (pal.good, "shaded: \"motion\" reported")]);

        ui.label(RichText::new("Verdicts for all four situations").strong());
        egui::Frame::group(ui.style()).show(ui, |ui| {
            egui::Grid::new("verdicts").striped(true).num_columns(3).show(ui, |ui| {
                for h in ["the person...", "PIR sensor", "radar"] {
                    ui.label(RichText::new(h).strong());
                }
                ui.end_row();
                for (c, name) in CASES {
                    let (pir, radar) = verdicts(&simulate(c));
                    let color = |v: &str| if v == "nothing" { pal.bad } else { pal.good };
                    ui.add(egui::Label::new(RichText::new(name).strong().color(if c == self.case { pal.trace[3] } else { pal.text })).extend());
                    ui.add(egui::Label::new(RichText::new(&pir).color(color(&pir))).extend());
                    ui.add(egui::Label::new(RichText::new(&radar).color(color(&radar))).extend());
                    ui.end_row();
                }
            });
        });

        let (x, y) = sc.start;
        let r = (x * x + y * y).sqrt();
        let (dx, dy) = (sc.heading_deg.to_radians().cos(), sc.heading_deg.to_radians().sin());
        let towards = -(x * dx + y * dy) * sc.speed_mps / r;
        let across = (x * dy - y * dx).abs() * sc.speed_mps / r;
        widgets::formula(
            ui,
            format!(
                "radar: Doppler = 2 x speed towards the sensor x f / c = {} per m/s at 24.125 GHz\n\
                 \x20      at the start of this walk: {} towards it -> {}\n\
                 PIR:   needs heat moving from zone to zone: {} across the view at {}",
                si(doppler_hz(1.0, F0), "Hz"),
                si(towards, "m/s"),
                si(doppler_hz(towards, F0).abs(), "Hz"),
                si(across, "m/s"),
                si(r, "m"),
            ),
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        self.case = match preset {
            "towards" => Case::Towards,
            "still" => Case::Still,
            "glass" => Case::Glass,
            _ => Case::Across,
        };
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "radar-vs-pir",
        title: "Radar versus PIR sensors",
        blurb: "Both switch the light on when you walk in. They notice very different things, and each misses what the other sees.",
        path: &["Radar"],
        make: || Box::new(RadarVsPir::default()),
        explain: Explanation {
            what: "A [[PIR]] sensor and a 24 GHz radar hang side by side and watch the same person. Pick a situation at the top. \
                The room is drawn from above; the two graphs show what each sensor's electronics see over 8 seconds, with its \
                threshold dashed and its \"motion\" reports shaded. The table runs all four situations through both models.\n\n\
                The PIR model is the one from the PIR demo; the radar is the motion sensor from the Doppler demo, which measures the [[Doppler shift]], \
                with the presence mode of the breathing demo added.",
            why: "A PIR sensor is passive: it sends nothing and only receives the far infrared that every warm body gives off. \
                Its lens divides the room into narrow zones, and its two elements are wired against each other, so it reacts \
                only when heat moves from one zone into the next. That makes it cheap and frugal, and blind to anything that \
                does not cross zones: a person walking straight at it, or sitting still. It also needs a direct line of sight \
                - ordinary glass blocks the long infrared wavelengths it sees - and a body warmer than the background.\n\n\
                A radar is active: it transmits microwaves and listens for echoes. A moving body shifts the echo's frequency \
                by the Doppler effect, but only in proportion to the speed towards or away from the radar. So it is best \
                where the PIR is worst - someone walking straight at it - and weakest for someone crossing right in front of \
                it, where for a moment the distance does not change. Microwaves pass through non-metallic materials, so a \
                radar can hide behind a plastic cover or see through glass. With the breathing analysis it can even tell \
                that someone is sitting still.\n\n\
                Seeing through things cuts both ways: a radar may also react to a person in the next room, a fan or a \
                curtain moving in a draught, while a PIR reacts to heaters and sunlight. Each sensor has blind spots that the \
                other covers, which is why some products combine both and report motion only when both agree.",
            formulas: &[
                ("radar: Doppler frequency = 2 x speed towards the sensor x frequency / speed of light", "1 m/s at 24.125 GHz: 161 Hz. Walking across, the speed towards the sensor passes through zero."),
                ("PIR: signal only when the body's angle changes, angular speed = speed across / distance", "1 m/s across at 5 m: 0.2 rad/s, about 11° per second: nearly two of the 6° zones each second."),
            ],
            try_this: &[
                Experiment { action: "Let the person walk straight at the sensor.", watch: "the PIR staying silent while the radar reports motion the whole time.", preset: Some("towards") },
                Experiment {
                    action: "Let the person sit still and read.",
                    watch: "neither motion signal reacting - but the radar's presence mode finding breathing at 15 per minute.",
                    preset: Some("still"),
                },
                Experiment { action: "Put a glass door between the person and the sensors.", watch: "the PIR seeing nothing at all; the radar's signal only somewhat weaker.", preset: Some("glass") },
            ],
            leaves_out: "Both models are simplified: the radar's echo falls with the square of the distance (its power with the fourth \
                power), and its threshold, the 5 Hz filter and the 3 dB loss through glass are example values. Real \
                radar modules see a person at up to about 15 m. False triggers are not simulated - for the radar moving \
                curtains, fans and people behind thin walls, for the PIR heaters, sunlight and pets - and neither is the \
                temperature of the room: on a hot day the difference between body and background shrinks and a PIR sensor \
                becomes less sensitive. Power consumption is not compared in numbers.",
            refs: &[
                "Panasonic PIR motion sensor FAQ: the sensor's filter passes about 5 µm and longer (far infrared); ordinary glass does not, so a person behind glass is not detected.",
                "Infineon Sense2GoL (24 GHz radar) product brief: detection through non-metallic materials, human target up to 15 m.",
                "Doppler shift 2 v f0 / c checked against the HB100 datasheet (see the Doppler demo); PIR model and sources: see the PIR demo.",
            ],
        },
    }
}
