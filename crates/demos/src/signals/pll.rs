use crate::prelude::*;
use egui_plot::{HLine, VLine};
use sim_core::ble::channel_mhz;
use sim_core::signals::Pll as PllModel;

const DURATION_US: f64 = 300.0;
const TOLERANCE_HZ: f64 = 10e3;

pub struct Pll {
    pll: PllModel,
    n_from: u32,
    n_to: u32,
}

impl Default for Pll {
    fn default() -> Self {
        Self { pll: PllModel { f_ref_hz: 1e6, vco_min_hz: 2.36e9, vco_max_hz: 2.52e9, loop_hz: 30e3, damping: 0.7 }, n_from: 2402, n_to: 2480 }
    }
}

/// "Bluetooth LE channel 39" if N MHz is one of the 40 channel frequencies.
fn ble_channel(n: u32) -> Option<u8> {
    (0..40).find(|&i| channel_mhz(i) == n)
}

impl Demo for Pll {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "A radio changes channel", |ui| {
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "from", |ui| ui.add(egui::Slider::new(&mut self.n_from, 2300..=2600).text("divider N before")).on_hover_text("The loop is settled with this divider. The oscillator runs at N times the reference."));
                widgets::wrap(ui, "to", |ui| ui.add(egui::Slider::new(&mut self.n_to, 2300..=2600).text("divider N after")).on_hover_text("At time zero the divider is switched to this number. Nothing else changes."));
            });
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "speed", |ui| ui.add(widgets::si_slider(&mut self.pll.loop_hz, 3e3..=100e3, "loop speed", "Hz").logarithmic(true)).on_hover_text("How quickly the loop reacts to a difference. Set by the loop filter. Faster settles sooner but lets more noise through."));
                widgets::wrap(ui, "damping", |ui| ui.add(egui::Slider::new(&mut self.pll.damping, 0.05..=2.0).logarithmic(true).text("damping").fixed_decimals(2)).on_hover_text("How firmly the loop brakes as it approaches the target. Around 0.7 is the usual choice; low values overshoot and ring."));
            });
        });
        let p = self.pll;
        let tr = p.hop(self.n_from, self.n_to, DURATION_US, TOLERANCE_HZ);

        // The loop as a block diagram.
        let total = ui.available_width().min(760.0);
        let (rect, _) = ui.allocate_exact_size(vec2(total, 118.0), egui::Sense::hover());
        let painter = ui.painter();
        let names = [("phase detector", "which one is ahead?"), ("loop filter", "smooths the answer"), ("oscillator", "tunable")];
        let (bw, bh, gap) = (150.0, 46.0, (total - 3.0 * 150.0 - 150.0) / 3.0);
        let x0 = rect.left() + 86.0;
        let y = rect.top() + 8.0;
        let boxes: Vec<egui::Rect> = (0..3).map(|i| egui::Rect::from_min_size(pos2(x0 + i as f32 * (bw + gap), y), vec2(bw, bh))).collect();
        let small = egui::FontId::proportional(12.0);
        let short_arrow = |from: Pos2, to: Pos2| {
            let d = (to - from).normalized();
            painter.line_segment([from, to - d * 5.0], (1.5, pal.dim));
            painter.arrow(to - d * 10.0, d * 10.0, (1.5, pal.dim));
        };
        for (b, (name, role)) in boxes.iter().zip(names) {
            painter.rect_filled(*b, 4.0, pal.canvas);
            painter.rect_stroke(*b, 4.0, (1.5, pal.trace[0]), egui::StrokeKind::Inside);
            painter.text(b.center() - vec2(0.0, 8.0), egui::Align2::CENTER_CENTER, name, egui::FontId::proportional(14.0), ui.visuals().strong_text_color());
            painter.text(b.center() + vec2(0.0, 10.0), egui::Align2::CENTER_CENTER, role, small.clone(), pal.dim);
        }
        painter.text(pos2(rect.left(), boxes[0].center().y), egui::Align2::LEFT_CENTER, format!("reference\n{}", si(p.f_ref_hz, "Hz")), small.clone(), pal.text);
        short_arrow(pos2(rect.left() + 62.0, boxes[0].center().y), boxes[0].left_center());
        short_arrow(boxes[0].right_center(), boxes[1].left_center());
        short_arrow(boxes[1].right_center(), boxes[2].left_center());
        let out = boxes[2].right_center() + vec2(26.0, 0.0);
        short_arrow(boxes[2].right_center(), out + vec2(22.0, 0.0));
        painter.text(out + vec2(26.0, 0.0), egui::Align2::LEFT_CENTER, "output", small.clone(), pal.text);
        // Feedback path through the divider.
        let div = egui::Rect::from_center_size(pos2(boxes[1].center().x, y + bh + 40.0), vec2(bw, 34.0));
        painter.rect_filled(div, 4.0, pal.canvas);
        painter.rect_stroke(div, 4.0, (1.5, pal.trace[1]), egui::StrokeKind::Inside);
        painter.text(div.center(), egui::Align2::CENTER_CENTER, format!("divide by N = {}", self.n_to), egui::FontId::proportional(14.0), ui.visuals().strong_text_color());
        painter.line_segment([out, pos2(out.x, div.center().y)], (1.5, pal.dim));
        short_arrow(pos2(out.x, div.center().y), div.right_center());
        let back = pos2(boxes[0].center().x, div.center().y);
        painter.line_segment([div.left_center(), back], (1.5, pal.dim));
        short_arrow(back, boxes[0].center_bottom());

        ui.label(RichText::new("The oscillator's frequency after the divider is changed").strong());
        let mhz: Vec<[f64; 2]> = tr.t_us.iter().zip(&tr.f_vco_hz).map(|(t, f)| [*t, f / 1e6]).collect();
        plots::plot_bare("freq", 230.0).x_axis_label("time [µs]").y_axis_label("frequency [MHz]").include_x(0.0).include_x(DURATION_US).show(ui, |pl| {
            pl.hline(HLine::new("target", tr.target_hz / 1e6).color(pal.good).style(egui_plot::LineStyle::dashed_dense()));
            for limit in [p.vco_min_hz, p.vco_max_hz] {
                if (tr.target_hz - limit).abs() < 0.12e9 && (limit < tr.target_hz) == (limit == p.vco_max_hz) {
                    pl.hline(HLine::new("limit", limit / 1e6).color(pal.bad).style(egui_plot::LineStyle::dashed_dense()));
                }
            }
            if let Some(t) = tr.lock_time_us {
                pl.vline(VLine::new("locked", t).color(pal.dim));
            }
            pl.line(plots::line("oscillator", mhz, pal.trace[0]).width(2.5));
        });
        ui.label(RichText::new("How far the two inputs of the phase detector are out of step").strong());
        let turns: Vec<[f64; 2]> = tr.t_us.iter().zip(&tr.phase_error_turns).map(|(t, e)| [*t, e * 360.0]).collect();
        plots::plot_bare("phase", 130.0).x_axis_label("time [µs]").y_axis_label("degrees").include_x(0.0).include_x(DURATION_US).include_y(0.0).show(ui, |pl| {
            pl.line(plots::line("phase difference", turns, pal.trace[1]).width(2.0));
        });
        widgets::key(ui, &[(pal.trace[0], "oscillator frequency"), (pal.good, "dashed: N x reference"), (pal.bad, "dashed: edge of the oscillator's range"), (pal.dim, "vertical line: settled"), (pal.trace[1], "phase difference")]);

        let channel = |n: u32| ble_channel(n).map_or(String::new(), |c| format!("  (Bluetooth LE channel {c})"));
        widgets::formula(
            ui,
            format!(
                "output = N x reference:   before {} x {} = {}{}\n\
                 \x20                         after  {} x {} = {}{}\n\
                 smallest step = one reference = {}        went past the target by {}",
                self.n_from,
                si(p.f_ref_hz, "Hz"),
                si(self.n_from as f64 * p.f_ref_hz, "Hz"),
                channel(self.n_from),
                self.n_to,
                si(p.f_ref_hz, "Hz"),
                si(tr.target_hz, "Hz"),
                channel(self.n_to),
                si(p.f_ref_hz, "Hz"),
                si(tr.overshoot_hz.max(0.0), "Hz"),
            ),
        );
        widgets::status(
            ui,
            tr.lock_time_us.is_some(),
            match tr.lock_time_us {
                Some(t) => format!("locked: within {} of the target after {}", si(TOLERANCE_HZ, "Hz"), si(t * 1e-6, "s")),
                None if tr.target_hz > p.vco_max_hz || tr.target_hz < p.vco_min_hz => format!("cannot lock: the target is outside the oscillator's range of {} to {}", si(p.vco_min_hz, "Hz"), si(p.vco_max_hz, "Hz")),
                None => "not settled within the time shown: the loop is too slow or rings too long".to_owned(),
            },
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "ringing" => self.pll.damping = 0.12,
            "slow" => self.pll.loop_hz = 5e3,
            "out-of-range" => self.n_to = 2560,
            "next-channel" => (self.n_from, self.n_to) = (2402, 2404),
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "signals-pll",
        title: "PLL: making frequencies",
        blurb: "How a radio makes 2.48 GHz out of a 1 MHz reference - and hops to another channel by changing one number.",
        path: &["Signals and analog"],
        make: || Box::new(Pll::default()),
        explain: Explanation {
            what: "The diagram at the top is a [[PLL]], a phase-locked loop: three boxes in a row and one on the way back. The \
                graph below shows the frequency of its [[oscillator]] over time. At time zero the number N in the \
                \"divide by\" box is changed, and the oscillator moves to a new frequency and settles there.\n\n\
                The second graph shows what drives that movement: how far the two signals arriving at the phase detector \
                are out of step.",
            why: "A radio needs a frequency in the gigahertz range that is exact to a few parts per million, and it needs to \
                change it quickly - a Bluetooth connection moves to another channel many times a second. An accurate frequency comes \
                from a [[crystal]], but crystals only work at a few tens of megahertz and cannot be retuned. An oscillator \
                that can be tuned over a wide range exists too, but left alone it drifts. A PLL combines the two: the \
                tunable oscillator is made to follow the crystal.\n\n\
                The trick is a comparison in a loop. The oscillator's output is divided down by a counter: only every \
                N-th cycle is passed on. The phase detector compares this divided signal with the reference and reports \
                which of the two is ahead and by how much. If the divided signal is falling behind, the oscillator is too \
                slow, and the loop filter turns the report into a higher tuning value; if it is ahead, a lower one. The \
                loop comes to rest only when the two signals stay exactly in step - and that means the oscillator is \
                running at exactly N times the reference.\n\n\
                Changing the frequency is now trivial: change N. The divided signal suddenly no longer matches, the loop \
                notices and steers the oscillator until it does again. With a 1 MHz reference, N = 2402 gives 2.402 GHz and \
                N = 2480 gives 2.480 GHz, each as exact as the crystal.\n\n\
                Two properties of the loop have to be chosen. A fast loop settles quickly but passes more of the \
                reference's noise on to the output. And like any control loop it can be damped too little, so that it \
                shoots past the target and rings, or too much, so that it creeps.",
            formulas: &[
                ("output frequency = N x reference frequency", "2480 x 1 MHz = 2.480 GHz. The output can only be set in steps of one reference frequency."),
                ("the loop is at rest when: output / N = reference, cycle for cycle", "Not just the same frequency on average: the two stay in step, which is why it is called phase-locked."),
            ],
            try_this: &[
                Experiment {
                    action: "Hop only to the neighbouring Bluetooth channel: N from 2402 to 2404.",
                    watch: "the same shape of curve, forty times smaller, and settled a little sooner because it has less far to go. The shape belongs to the loop, not to the size of the hop.",
                    preset: Some("next-channel"),
                },
                Experiment {
                    action: "Lower the damping to 0.12.",
                    watch: "the oscillator shooting far past 2480 MHz and swinging back and forth before it settles. During that time the radio is on the wrong frequency.",
                    preset: Some("ringing"),
                },
                Experiment { action: "Slow the loop down to 5 kHz.", watch: "a smooth approach that is simply not finished within the time shown.", preset: Some("slow") },
                Experiment {
                    action: "Ask for N = 2560.",
                    watch: "the oscillator running into the top of its tuning range and staying there, while the phase difference grows without end. The loop cannot lock on a frequency its oscillator cannot reach.",
                    preset: Some("out-of-range"),
                },
            ],
            leaves_out: "The loop is modelled by its averages; a real phase detector works on individual edges, usually with a \
                charge pump, and the loop filter is a small network of resistors and capacitors. Real PLLs can also divide by \
                fractions (fractional-N) to get steps finer than the reference, and their output carries phase noise, which \
                the choice of loop speed trades off. The tuning range and loop values here are examples. The PLL in a \
                microcontroller's clock tree (see the clock demo) is the same circuit, used once at start-up instead of for \
                hopping.",
            refs: &["Phase-locked loop with a divider in the feedback path (integer-N synthesizer): f_out = N x f_ref; second-order loop with natural frequency and damping. Analog Devices tutorial MT-086, \"Fundamentals of Phase Locked Loops (PLLs)\"; F. M. Gardner, \"Phaselock Techniques\"."],
        },
    }
}
