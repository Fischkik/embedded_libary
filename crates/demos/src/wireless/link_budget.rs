use crate::prelude::*;
use egui_plot::{HLine, Points, VLine};
use sim_core::radio::{Link, fspl_db};

const SPEC_SENSITIVITY_DBM: f64 = -70.0;
const D_RANGE_M: (f64, f64) = (0.1, 1000.0);

pub struct LinkBudget {
    link: Link,
    distance_m: f64,
}

impl Default for LinkBudget {
    fn default() -> Self {
        Self { link: Link { tx_dbm: 0.0, tx_gain_dbi: 0.0, rx_gain_dbi: 0.0, extra_loss_db: 0.0, f_hz: 2.44e9, sensitivity_dbm: -90.0 }, distance_m: 20.0 }
    }
}

impl Demo for LinkBudget {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        let l = &mut self.link;
        widgets::group(ui, "Sender, receiver and what lies between", |ui| {
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "tx", |ui| ui.add(egui::Slider::new(&mut l.tx_dbm, -20.0..=20.0).text("transmit power").suffix(" dBm").fixed_decimals(0)).on_hover_text("0 dBm is 1 mW. Bluetooth LE allows -20 dBm (0.01 mW) to +20 dBm (100 mW); small devices typically use 0 dBm."));
                widgets::wrap(ui, "d", |ui| ui.add(egui::Slider::new(&mut self.distance_m, D_RANGE_M.0..=D_RANGE_M.1).logarithmic(true).text("distance").suffix(" m").fixed_decimals(1)).on_hover_text("How far apart sender and receiver are."));
            });
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "gt", |ui| ui.add(egui::Slider::new(&mut l.tx_gain_dbi, -10.0..=10.0).text("antenna gain, sender").suffix(" dBi").fixed_decimals(1)).on_hover_text("How much the sender's antenna favours the direction of the receiver. A small chip antenna is often below 0 dBi."));
                widgets::wrap(ui, "gr", |ui| ui.add(egui::Slider::new(&mut l.rx_gain_dbi, -10.0..=10.0).text("antenna gain, receiver").suffix(" dBi").fixed_decimals(1)).on_hover_text("The same for the receiver's antenna."));
            });
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "loss", |ui| ui.add(egui::Slider::new(&mut l.extra_loss_db, 0.0..=40.0).text("walls, bodies, housing").suffix(" dB").fixed_decimals(0)).on_hover_text("Everything that weakens the signal beyond spreading out: walls, people in the way, a metal housing."));
                widgets::wrap(ui, "sens", |ui| ui.add(egui::Slider::new(&mut l.sensitivity_dbm, -100.0..=-60.0).text("receiver sensitivity").suffix(" dBm").fixed_decimals(0)).on_hover_text("The weakest signal the receiver can still decode. The Bluetooth specification demands -70 dBm or better; many chips manage around -95 dBm."));
            });
        });
        let l = self.link;
        let d = self.distance_m;
        let loss = fspl_db(d, l.f_hz);
        let rx = l.received_dbm(d);
        let margin = rx - l.sensitivity_dbm;
        let range = l.range_m();

        // The budget as a staircase: each step adds or removes decibels.
        ui.label(RichText::new("The budget, step by step").strong());
        let steps = [
            ("transmit", l.tx_dbm),
            ("+ antenna", l.tx_gain_dbi),
            ("- distance", -loss),
            ("- walls", -l.extra_loss_db),
            ("+ antenna", l.rx_gain_dbi),
        ];
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width().min(760.0), 170.0), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        let (top_dbm, bottom_dbm) = (25.0, -110.0);
        let y = |dbm: f64| rect.top() + ((top_dbm - dbm) / (top_dbm - bottom_dbm)) as f32 * rect.height();
        let col_w = (rect.width() - 80.0) / (steps.len() + 1) as f32;
        painter.rect_filled(rect, 4.0, pal.canvas);
        for dbm in [0.0, -50.0, -100.0] {
            painter.line_segment([pos2(rect.left(), y(dbm)), pos2(rect.right(), y(dbm))], (0.5, pal.grid));
            painter.text(pos2(rect.right() - 4.0, y(dbm) - 2.0), egui::Align2::RIGHT_BOTTOM, format!("{dbm:.0} dBm"), egui::FontId::proportional(11.0), pal.dim);
        }
        painter.line_segment([pos2(rect.left(), y(l.sensitivity_dbm)), pos2(rect.right(), y(l.sensitivity_dbm))], (1.5, pal.warn));
        let mut level = 0.0;
        for (k, (name, delta)) in steps.iter().enumerate() {
            let x = rect.left() + 8.0 + k as f32 * col_w;
            let (from, to) = (if k == 0 { 0.0 } else { level }, if k == 0 { *delta } else { level + delta });
            let color = if to >= from { pal.good } else { pal.bad };
            let r = egui::Rect::from_two_pos(pos2(x, y(from)), pos2(x + col_w - 10.0, y(to)));
            painter.rect_filled(r.expand2(vec2(0.0, 0.5)), 2.0, color.gamma_multiply(0.7));
            painter.text(pos2(x, rect.bottom() - 4.0), egui::Align2::LEFT_BOTTOM, *name, egui::FontId::proportional(10.5), pal.text);
            painter.text(pos2(r.center().x, r.top() - 2.0), egui::Align2::CENTER_BOTTOM, format!("{:+.1}", delta), egui::FontId::monospace(11.0), pal.text);
            level = to;
        }
        let x = rect.left() + 8.0 + steps.len() as f32 * col_w;
        painter.circle_filled(pos2(x + 10.0, y(rx)), 6.0, if margin >= 0.0 { pal.good } else { pal.bad });
        painter.text(pos2(x + 20.0, y(rx)), egui::Align2::LEFT_CENTER, format!("arrives: {rx:.1} dBm"), egui::FontId::proportional(12.0), pal.text);
        widgets::key(ui, &[(pal.good, "adds power"), (pal.bad, "takes power away"), (pal.warn, "the receiver's sensitivity: the signal must arrive above this line")]);

        ui.label(RichText::new("Received power over distance").strong());
        let (a, b) = (D_RANGE_M.0.log10(), D_RANGE_M.1.log10());
        let curve: Vec<[f64; 2]> = (0..=200).map(|i| a + (b - a) * i as f64 / 200.0).map(|x| [x, l.received_dbm(10f64.powf(x))]).collect();
        plots::plot_bare("distance", 220.0)
            .x_axis_label("distance")
            .y_axis_label("received [dBm]")
            .include_y(-110.0)
            .include_y(10.0)
            .x_grid_spacer(egui_plot::uniform_grid_spacer(|_| [1.0, 1.0, 1.0]))
            .x_axis_formatter(|mark, _| {
                let i = mark.value.round();
                if (mark.value - i).abs() < 0.01 { si(10f64.powf(i), "m") } else { String::new() }
            })
            .show(ui, |p| {
                p.line(plots::line("received power", curve, pal.trace[0]).width(2.5));
                p.hline(HLine::new("sensitivity", l.sensitivity_dbm).color(pal.warn).width(1.5));
                p.hline(HLine::new("spec minimum", SPEC_SENSITIVITY_DBM).color(pal.dim).style(egui_plot::LineStyle::dashed_dense()));
                if range > D_RANGE_M.0 && range < D_RANGE_M.1 {
                    p.vline(VLine::new("range", range.log10()).color(pal.warn).style(egui_plot::LineStyle::dashed_dense()));
                }
                p.points(Points::new("now", vec![[d.log10(), rx]]).radius(6.0).color(pal.text));
            });
        widgets::key(ui, &[(pal.trace[0], "power arriving at the receiver"), (pal.warn, "this receiver's sensitivity, and the range it gives"), (pal.dim, "dashed: -70 dBm, the minimum the Bluetooth specification demands")]);

        widgets::formula(
            ui,
            format!(
                "received = transmit + gains - path loss - walls = {:+.0} {:+.1} {:+.1} - {:.1} - {:.0} = {:.1} dBm\n\
                 path loss in free space = 20 log10(4 pi x distance x frequency / c) = 20 log10(4 pi x {} x {} / c) = {:.1} dB\n\
                 margin = received - sensitivity = {:.1} - ({:.0}) = {:+.1} dB        range: {}",
                l.tx_dbm,
                l.tx_gain_dbi,
                l.rx_gain_dbi,
                loss,
                l.extra_loss_db,
                rx,
                si(d, "m"),
                si(l.f_hz, "Hz"),
                loss,
                rx,
                l.sensitivity_dbm,
                margin,
                si(range, "m"),
            ),
        );
        widgets::status(
            ui,
            margin >= 6.0,
            if margin < 0.0 {
                format!("the signal arrives {:.0} dB below what the receiver can decode: no link", -margin)
            } else if margin < 6.0 {
                format!("the link works, but with only {margin:.0} dB to spare: a person stepping in the way can break it")
            } else {
                format!("the link works with {margin:.0} dB to spare")
            },
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "double" => self.distance_m = 40.0,
            "walls" => self.link.extra_loss_db = 20.0,
            "spec" => self.link.sensitivity_dbm = -70.0,
            "boost" => (self.link.tx_dbm, self.link.extra_loss_db) = (8.0, 20.0),
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "link-budget",
        title: "Link budget: power, distance, sensitivity",
        blurb: "Will the signal arrive? Add up what the sender puts in, subtract what the way takes, and compare with what the receiver needs.",
        path: &["Wireless", "Radio basics"],
        make: || Box::new(LinkBudget::default()),
        explain: Explanation {
            what: "The staircase follows the radio signal from the sender to the receiver. It starts at the transmit power, goes \
                up a little with each antenna's gain, and falls steeply over the distance and through any walls. The yellow \
                line is the weakest signal the receiver can still decode. If the end of the staircase lies above it, the \
                link works.\n\n\
                The graph below shows the same received power for every distance, and where it crosses the receiver's limit: \
                the range.",
            why: "Radio power is measured in [[dBm]]: decibels relative to one milliwatt. 0 dBm is 1 mW, +10 dBm is 10 mW, -10 dBm \
                is 0.1 mW, and every 10 dB is another factor of ten. The point of this scale is that the powers involved span \
                an absurd range: a Bluetooth device sends about 1 mW, and the receiver works with a hundred-millionth of a \
                millionth of that. In dB, all those factors become additions and subtractions, and the whole calculation fits \
                on one line. That line is the link budget.\n\n\
                The largest item is the loss over the distance. Even in empty space the wave spreads out over a growing \
                sphere, and the receiving antenna catches only its own small share. That share falls with the square of the \
                distance: double the distance and only a quarter arrives, which is 6 dB less. The share also depends on the \
                frequency, because an antenna for a higher frequency is smaller and catches less. At 2.4 GHz the first metre \
                alone costs 40 dB, ten metres 60 dB, a hundred metres 80 dB.\n\n\
                Then come the losses that free space does not have: walls, furniture, people (bodies are mostly water, \
                which absorbs 2.4 GHz well), a metal housing. A single wall can take anything from a few dB to well over ten.\n\n\
                What remains is compared with the receiver's [[sensitivity]]. The difference is the margin. A link with a \
                margin of 1 dB works on the bench and fails when someone walks past, so designers plan for some spare.\n\n\
                Because loss grows with the square of the distance, every 6 dB of improvement doubles the range - wherever \
                the decibels come from: more transmit power, better antennas, a more sensitive receiver, or one wall fewer.",
            formulas: &[
                ("received power = transmit power + antenna gains - path loss - other losses (all in dB)", "0 dBm + 0 + 0 - 66.2 dB at 20 m = -66.2 dBm."),
                ("free-space path loss = 20 log10(4 pi x distance x frequency / speed of light)", "At 2.44 GHz: 40.2 dB at 1 m, 60.2 dB at 10 m. Each doubling of distance adds 6 dB."),
            ],
            try_this: &[
                Experiment {
                    action: "Double the distance from 20 m to 40 m.",
                    watch: "the received power falling by exactly 6 dB - a quarter of the power - and the margin shrinking by the same 6 dB.",
                    preset: Some("double"),
                },
                Experiment {
                    action: "Back at 20 m, put 20 dB of walls in the way.",
                    watch: "the range dropping from about 310 m to about 31 m. Twenty decibels cost a factor of ten in distance.",
                    preset: Some("walls"),
                },
                Experiment {
                    action: "Use a receiver that just meets the specification: -70 dBm.",
                    watch: "the range shrinking from about 310 m to about 31 m: the specification's minimum is 20 dB less sensitive than a typical chip.",
                    preset: Some("spec"),
                },
                Experiment {
                    action: "With the walls back, raise the transmit power to +8 dBm.",
                    watch: "8 dB of the 20 dB loss won back - and the battery paying for it, because the radio now draws several times the current.",
                    preset: Some("boost"),
                },
            ],
            leaves_out: "Free space is the best case. Indoors, signals also bounce off walls and floors and arrive by several paths at \
                once; these copies add or cancel depending on position, so moving a device a few centimetres can change the \
                received power by 20 dB or more (fading). That is a strong reason to keep a generous margin. The noise and \
                interference from other devices, which raise the level a receiver actually needs, are not modelled. Bluetooth \
                also has a slower \"coded\" mode that trades data rate for a few dB of extra sensitivity.",
            refs: &[
                "Friis transmission equation and free-space path loss, e.g. Balanis, \"Antenna Theory\", section 2.17.",
                "Bluetooth Core Specification 5.4, Vol 6 Part A: transmit power at the maximum setting between 0.01 mW (-20 dBm) and 100 mW (+20 dBm); reference sensitivity level -70 dBm at a bit error rate of 0.1 %.",
                "Typical chip sensitivity: e.g. Nordic nRF52832 product brief, -96 dBm at 1 Mbit/s.",
            ],
        },
    }
}
