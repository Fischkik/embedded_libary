use crate::prelude::*;
use egui_plot::{HLine, Polygon};
use sim_core::signals::{PIR_BAND_HZ, PIR_GAIN, PIR_SECONDS, PIR_THRESHOLD_V, PirScene};

/// How far the top view reaches, metres.
const VIEW_M: f64 = 12.0;

pub struct Pir {
    scene: PirScene,
    /// False: the room is empty (the walk settings are kept but not used).
    present: bool,
}

impl Default for Pir {
    fn default() -> Self {
        Self { scene: PirScene { start: (-4.0, 5.0), heading_deg: 0.0, speed_mps: 1.0, zone_pitch_deg: 6.0, draught: 0.0, single_element: false }, present: true }
    }
}

impl Demo for Pir {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        let s = &mut self.scene;
        widgets::group(ui, "Someone walks past the sensor", |ui| {
            ui.checkbox(&mut self.present, "someone is in the room").on_hover_text("Untick for an empty room, to see what else can make the sensor react.");
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "heading", |ui| {
                    ui.add(egui::Slider::new(&mut s.heading_deg, -90.0..=90.0).text("direction").suffix(" °").fixed_decimals(0))
                        .on_hover_text("0 °: across the sensor's view, from left to right. -90 °: straight towards the sensor. 90 °: straight away from it.")
                });
                widgets::wrap(ui, "speed", |ui| ui.add(egui::Slider::new(&mut s.speed_mps, 0.05..=3.0).logarithmic(true).text("speed").suffix(" m/s").fixed_decimals(2)).on_hover_text("Normal walking is about 1 to 1.5 m/s."));
            });
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "x", |ui| ui.add(egui::Slider::new(&mut s.start.0, -8.0..=8.0).text("starts at x").suffix(" m").fixed_decimals(1)).on_hover_text("Where the walk starts: sideways from the sensor."));
                widgets::wrap(ui, "y", |ui| ui.add(egui::Slider::new(&mut s.start.1, 1.0..=14.0).text("and at distance").suffix(" m").fixed_decimals(1)).on_hover_text("Where the walk starts: how far in front of the sensor."));
            });
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "draught", |ui| ui.add(egui::Slider::new(&mut s.draught, 0.0..=3.0).text("warm draught on the sensor").fixed_decimals(1)).on_hover_text("Warm air from a heater blowing over the sensor between 2 s and 5 s. It warms both elements alike."));
                ui.checkbox(&mut s.single_element, "only one sensor element").on_hover_text("A sensor with a single element instead of the usual pair. Untick for the normal dual sensor.");
            });
        });
        let sc = self.scene;
        // An empty room: the person is put far outside every zone, standing still.
        let tr = if self.present { sc.simulate() } else { PirScene { start: (100.0, 5.0), speed_mps: 0.0, ..sc }.simulate() };

        ui.label(RichText::new("Seen from above: the lens divides the room into zones").strong());
        let side = ui.available_width().min(560.0);
        let (rect, _) = ui.allocate_exact_size(vec2(side, side * 0.5), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, pal.canvas);
        let scale = rect.height() as f64 / VIEW_M;
        let origin = rect.center_bottom() - vec2(0.0, 6.0);
        let to_screen = |x: f64, y: f64| origin + vec2((x * scale) as f32, -(y * scale) as f32);
        let width = sc.zone_pitch_deg.to_radians() / 4.0;
        for (angle, sign) in sc.zones() {
            if sc.single_element && sign < 0.0 {
                continue;
            }
            let color = if sign > 0.0 { pal.trace[0] } else { pal.trace[1] }.gamma_multiply(0.35);
            let (a0, a1) = (angle - width, angle + width);
            let pts = vec![to_screen(0.0, 0.0), to_screen(VIEW_M * a0.sin() * 1.2, VIEW_M * a0.cos() * 1.2), to_screen(VIEW_M * a1.sin() * 1.2, VIEW_M * a1.cos() * 1.2)];
            painter.add(egui::Shape::convex_polygon(pts, color, egui::Stroke::NONE));
        }
        let path: Vec<Pos2> = tr.position.iter().map(|&(x, y)| to_screen(x, y)).collect();
        if self.present {
            painter.add(egui::Shape::line(path.clone(), (2.5, pal.trace[3])));
        }
        if let (true, Some(a), Some(b)) = (self.present, path.first(), path.last()) {
            painter.circle_filled(*a, 5.0, pal.trace[3]);
            let d = (*b - path[path.len().saturating_sub(5)]).normalized() * 10.0;
            painter.arrow(*b - d, d, (2.5, pal.trace[3]));
        }
        painter.circle_filled(origin, 6.0, pal.text);
        painter.text(origin + vec2(10.0, -2.0), egui::Align2::LEFT_BOTTOM, "sensor", egui::FontId::proportional(12.0), pal.text);
        for m in [5.0, 10.0] {
            painter.text(to_screen(-VIEW_M * 0.95, m), egui::Align2::LEFT_CENTER, format!("{m:.0} m"), egui::FontId::proportional(11.0), pal.dim);
            painter.line_segment([to_screen(-VIEW_M, m), to_screen(VIEW_M, m)], (0.5, pal.grid));
        }
        widgets::key(ui, &[(pal.trace[0], "zones seen by element A"), (pal.trace[1], "zones seen by element B (subtracted)"), (pal.trace[3], "the person's path, 8 seconds")]);

        ui.label(RichText::new("What the sensor delivers, and the same after the amplifier").strong());
        plots::plot_bare("raw", 150.0).x_axis_label("time [s]").y_axis_label("sensor [mV]").include_x(0.0).include_x(PIR_SECONDS).include_y(-1.0).include_y(1.0).show(ui, |p| {
            p.line(plots::line("raw", plots::series(&tr.raw_mv, 0.0, 1.0 / sim_core::signals::PIR_RATE_HZ), pal.trace[4]).width(1.8));
        });
        plots::plot_bare("amp", 190.0).x_axis_label("time [s]").y_axis_label("amplified [V]").include_x(0.0).include_x(PIR_SECONDS).include_y(-1.5).include_y(1.5).show(ui, |p| {
            for &(a, b) in &tr.detections {
                let fill = pal.good.gamma_multiply(0.18);
                p.polygon(Polygon::new("motion", plots::bar(a, b, -1.5, 1.5)).fill_color(fill).stroke((0.0, fill)));
            }
            for th in [PIR_THRESHOLD_V, -PIR_THRESHOLD_V] {
                p.hline(HLine::new("threshold", th).color(pal.warn).style(egui_plot::LineStyle::dashed_dense()));
            }
            p.line(plots::line("amplified", plots::series(&tr.amplified_v, 0.0, 1.0 / sim_core::signals::PIR_RATE_HZ), pal.trace[2]).width(2.0));
        });
        widgets::key(ui, &[(pal.trace[4], "sensor output (size illustrative)"), (pal.trace[2], "after the band-pass amplifier"), (pal.warn, "dashed: the comparator's window"), (pal.good, "shaded: \"motion\" reported")]);

        let dist = (sc.start.0.powi(2) + sc.start.1.powi(2)).sqrt();
        let across = sc.speed_mps * sc.heading_deg.to_radians().cos().abs();
        let rate = across / dist / (2.0 * sc.zone_pitch_deg.to_radians());
        widgets::formula(
            ui,
            format!(
                "sideways speed = speed x cos(direction) = {:.2} m/s x cos({:.0} °) = {:.2} m/s\n\
                 signal frequency = sideways speed / distance / (2 x zone angle)\n\
                 \x20                = {:.2} / {:.1} m / (2 x {:.0} °) = {:.2} Hz        the amplifier passes about {:.1} to {:.0} Hz\n\
                 amplifier gain = {:.0}; it reports motion beyond +/- {:.1} V",
                sc.speed_mps,
                sc.heading_deg,
                across,
                across,
                dist,
                sc.zone_pitch_deg,
                rate,
                PIR_BAND_HZ.0,
                PIR_BAND_HZ.1,
                PIR_GAIN,
                PIR_THRESHOLD_V,
            ),
        );
        let walking = self.present;
        widgets::status(
            ui,
            !tr.detections.is_empty() == walking,
            match (!tr.detections.is_empty(), walking) {
                (true, true) => format!("motion detected, first at {:.1} s", tr.detections[0].0),
                (false, true) => "nobody detected, although someone is walking there".to_owned(),
                (true, false) => format!("motion reported at {:.1} s, but nobody is there: a false alarm", tr.detections[0].0),
                (false, false) => "nothing reported, and nobody is there".to_owned(),
            },
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        let s = &mut self.scene;
        match preset {
            "towards" => (s.start, s.heading_deg, s.speed_mps) = ((0.0, 9.0), -90.0, 0.6),
            "slow" => s.speed_mps = 0.1,
            "draught" => (self.present, s.draught) = (false, 2.0),
            "single" => (self.present, s.draught, s.single_element) = (false, 2.0, true),
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "signals-pir",
        title: "PIR sensor front end",
        blurb: "The motion detector on the ceiling: two tiny heat sensors, a lens full of zones, and an amplifier that only listens to the right speed.",
        path: &["Signals and analog"],
        make: || Box::new(Pir::default()),
        explain: Explanation {
            what: "The picture at the top is a room seen from above, with a [[PIR]] motion sensor at the bottom edge. Its lens \
                divides the room into narrow zones, drawn in two colours: zones of the two colours are seen by the two \
                halves of the sensor. The red line is a person walking for eight seconds.\n\n\
                Below are two graphs: the tiny voltage the sensor itself produces, and the same signal after the amplifier. \
                Whenever the amplified signal leaves the dashed window, the sensor reports motion (green shading).",
            why: "Every warm body glows in the infrared - people at a wavelength of around ten micrometres. A PIR sensor (passive \
                infrared: it sends nothing out, it only receives) has a crystal that produces a voltage when the infrared \
                falling on it changes. It does not measure warmth; it measures change of warmth. A person standing still in \
                front of it, or the warm wall behind them, produces nothing.\n\n\
                To make walking produce a change, the sensor has two elements wired against each other, and a lens of many \
                small facets in front. Each facet throws one narrow zone of the room onto one of the two elements, and \
                neighbouring zones go to opposite elements. A person walking across the room passes from a zone of element \
                A to a zone of element B and on to the next A: the output swings positive, negative, positive. That swinging \
                is the motion signal.\n\n\
                The pair has a second purpose. Anything that warms both elements alike - the sun moving across the window, a \
                warm draught, the room heating up in the morning - adds the same amount to both and cancels when one is \
                subtracted from the other.\n\n\
                The sensor's voltage is tiny, so an amplifier raises it a few thousand times. It is also a [[band-pass \
                filter]]: it passes only changes that happen at roughly walking speed, from a fraction of a hertz to a few \
                hertz. Slower changes are drift, faster ones are interference. Finally two comparators form a window: as \
                long as the amplified signal stays between the two thresholds, nothing is reported.\n\n\
                The geometry explains the sensor's best-known weakness. Someone walking straight towards it stays in the \
                same zone all the way, and only gets slowly warmer: no swinging, and little to see.",
            formulas: &[
                ("signal frequency = sideways speed / distance / (2 x zone angle)", "1 m/s across, 6.4 m away, zones 6 degrees apart: about 0.75 Hz - inside the amplifier's band."),
                ("dual sensor output = element A - element B", "A draught warming both elements by the same amount adds nothing to the difference."),
            ],
            try_this: &[
                Experiment {
                    action: "Walk straight towards the sensor instead of across it.",
                    watch: "the person staying inside one zone, the sensor's signal creeping up slowly instead of swinging, and the amplifier letting almost none of it through. Nobody is detected.",
                    preset: Some("towards"),
                },
                Experiment {
                    action: "Walk across, but very slowly: 0.1 m/s.",
                    watch: "the swinging still there in the raw signal, but so slow that the band-pass amplifier treats it as drift. Burglars in films know this trick.",
                    preset: Some("slow"),
                },
                Experiment {
                    action: "Empty the room and let a warm draught blow over the sensor.",
                    watch: "nothing at all in either graph: the draught warms both elements equally and cancels.",
                    preset: Some("draught"),
                },
                Experiment {
                    action: "The same draught on a sensor with only one element.",
                    watch: "a slow bump that the amplifier turns into a false alarm. This is why every PIR sensor has (at least) two elements.",
                    preset: Some("single"),
                },
            ],
            leaves_out: "The person is a point; a real body covers several zones at close range. Real lenses have zones at several \
                heights and in uneven patterns, and their sensitivity falls off towards the sides. The sizes of the voltages \
                are illustrative, chosen so that the relations are right: the gain of a few thousand and a pass band of \
                roughly 0.5 to 5 Hz are typical of published designs. The element itself has a slow thermal response that \
                is folded into the band-pass here. Real detectors often require two or more swings before they report, to \
                reject single spikes, and they hold the output for a while after the last motion.",
            refs: &[
                "Passive infrared sensor principle (dual element wired as opposite inputs, Fresnel lens zones, 8-14 µm window, whole-sensor changes self-cancelling): Wikipedia, \"Passive infrared sensor\".",
                "Front-end design (two gain stages with band-pass filtering, window comparator, human-motion band roughly 0.5-5 Hz): Texas Instruments SNAA301 (TLV8544 PIR reference design); STMicroelectronics, \"Signal conditioning for pyroelectric passive infrared sensors\"; Renesas TB512.",
            ],
        },
    }
}
