use crate::prelude::*;
use egui_plot::HLine;
use sim_core::signals::NonInverting;

pub struct OpAmp {
    amp: NonInverting,
    swing_v: f64,
    f_hz: f64,
}

impl Default for OpAmp {
    fn default() -> Self {
        Self { amp: NonInverting { r1_ohm: 1e3, r2_ohm: 22e3, supply_v: 3.3, gbw_hz: 1e6 }, swing_v: 0.1, f_hz: 1e3 }
    }
}

impl Demo for OpAmp {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        let a = &mut self.amp;
        widgets::group(ui, "A small sensor signal and an amplifier", |ui| {
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "swing", |ui| ui.add(widgets::si_slider(&mut self.swing_v, 1e-3..=1.0, "the sensor's signal swings from 0 to", "V").logarithmic(true)).on_hover_text("The size of the input signal. Many sensors deliver only millivolts."));
                widgets::wrap(ui, "f", |ui| ui.add(widgets::si_slider(&mut self.f_hz, 10.0..=1e6, "signal frequency", "Hz").logarithmic(true)).on_hover_text("How fast the sensor signal changes."));
            });
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "r2", |ui| ui.add(widgets::si_slider(&mut a.r2_ohm, 100.0..=1e6, "R2 (feedback)", "ohm").logarithmic(true)).on_hover_text("The resistor from the output back to the inverting input. Larger means more gain."));
                widgets::wrap(ui, "r1", |ui| ui.add(widgets::si_slider(&mut a.r1_ohm, 100.0..=1e6, "R1 (to ground)", "ohm").logarithmic(true)).on_hover_text("The resistor from the inverting input to ground. Smaller means more gain."));
            });
        });
        let a = self.amp;
        let (gain, bw) = (a.gain(), a.bandwidth_hz());

        // The circuit: a triangle, the feedback divider, input and output.
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width().min(560.0), 150.0), egui::Sense::hover());
        let painter = ui.painter();
        let wire = (1.5, pal.dim);
        let text = |p: Pos2, s: String, c: Color32| {
            painter.text(p, egui::Align2::CENTER_CENTER, s, egui::FontId::proportional(13.0), c);
        };
        let tri_left = rect.left() + 190.0;
        let (top, mid, bottom) = (rect.top() + 14.0, rect.top() + 52.0, rect.top() + 90.0);
        let tip = pos2(tri_left + 90.0, mid);
        painter.add(egui::Shape::closed_line(vec![pos2(tri_left, top), tip, pos2(tri_left, bottom)], (2.0, pal.trace[0])));
        let (plus, minus) = (pos2(tri_left, mid - 18.0), pos2(tri_left, mid + 18.0));
        text(plus + vec2(12.0, 0.0), "+".to_owned(), pal.text);
        text(minus + vec2(12.0, 0.0), "-".to_owned(), pal.text);
        painter.line_segment([pos2(rect.left() + 70.0, plus.y), plus], wire);
        text(pos2(rect.left() + 34.0, plus.y), "sensor".to_owned(), pal.trace[1]);
        let out = pos2(tip.x + 120.0, mid);
        painter.line_segment([tip, out], wire);
        text(out + vec2(34.0, 0.0), "to the ADC".to_owned(), pal.trace[2]);
        // Feedback: from the output down, back through R2 to the "-" input, and R1 to ground.
        let node = pos2(tip.x + 50.0, mid);
        let fb_y = rect.top() + 118.0;
        let junction = pos2(tri_left - 40.0, fb_y);
        painter.line_segment([node, pos2(node.x, fb_y)], wire);
        painter.line_segment([pos2(node.x, fb_y), junction], wire);
        painter.line_segment([junction, pos2(junction.x, minus.y)], wire);
        painter.line_segment([pos2(junction.x, minus.y), minus], wire);
        let resistor = |centre: Pos2, label: String| {
            let r = egui::Rect::from_center_size(centre, vec2(58.0, 18.0));
            painter.rect_filled(r, 2.0, pal.canvas);
            painter.rect_stroke(r, 2.0, (1.5, pal.warn), egui::StrokeKind::Inside);
            text(centre + vec2(0.0, 19.0), label, pal.text);
        };
        resistor(pos2((node.x + junction.x) / 2.0 + 20.0, fb_y), format!("R2 = {}", si(a.r2_ohm, "ohm")));
        painter.line_segment([junction, pos2(junction.x - 110.0, fb_y)], wire);
        resistor(pos2(junction.x - 60.0, fb_y), format!("R1 = {}", si(a.r1_ohm, "ohm")));
        text(pos2(junction.x - 128.0, fb_y), "ground".to_owned(), pal.dim);

        ui.label(RichText::new("Input and output over two periods").strong());
        let n = 400;
        let period = 1.0 / self.f_hz;
        let (scale, unit) = if period >= 1e-3 { (1e3, "ms") } else { (1e6, "µs") };
        let mut input = Vec::with_capacity(n + 1);
        let mut ideal = Vec::with_capacity(n + 1);
        let mut real = Vec::with_capacity(n + 1);
        let mut clipped = false;
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for k in 0..=n {
            let t = 2.0 * period * k as f64 / n as f64;
            let (i, r) = a.output(self.swing_v, self.f_hz, t);
            input.push([t * scale, self.swing_v / 2.0 * (1.0 + (std::f64::consts::TAU * self.f_hz * t).sin())]);
            ideal.push([t * scale, i]);
            real.push([t * scale, r]);
            clipped |= r >= a.supply_v - 1e-9;
            (lo, hi) = (lo.min(r), hi.max(r));
        }
        let show_ideal = gain * self.swing_v <= a.supply_v * 2.0;
        plots::plot_bare("waves", 250.0).x_axis_label(format!("time [{unit}]")).y_axis_label("volts").include_y(0.0).include_y(a.supply_v * 1.1).show(ui, |p| {
            p.hline(HLine::new("supply", a.supply_v).color(pal.bad).style(egui_plot::LineStyle::dashed_dense()));
            if show_ideal {
                p.line(plots::line("ideal", ideal, pal.dim).style(egui_plot::LineStyle::dashed_dense()));
            }
            p.line(plots::line("input", input, pal.trace[1]).width(2.0));
            p.line(plots::line("output", real, pal.trace[2]).width(2.5));
        });
        widgets::key(ui, &[(pal.trace[1], "input: the sensor's signal"), (pal.trace[2], "output of the amplifier"), (pal.dim, "dashed grey: gain x input, what a perfect amplifier would deliver"), (pal.bad, "dashed: the supply voltage, the highest the output can go")]);

        let used = (hi - lo) / a.supply_v;
        widgets::formula(
            ui,
            format!(
                "gain       = 1 + R2 / R1 = 1 + {} / {} = {:.1}\n\
                 output     = gain x input: the largest input {} would become {}   (supply: {})\n\
                 bandwidth  = gain-bandwidth product / gain = {} / {:.1} = {}\n\
                 at {}: gain is still {:.1}, and the output swing uses {:.0} % of the ADC's range",
                si(a.r2_ohm, "ohm"),
                si(a.r1_ohm, "ohm"),
                gain,
                si(self.swing_v, "V"),
                si(gain * self.swing_v, "V"),
                si(a.supply_v, "V"),
                si(a.gbw_hz, "Hz"),
                gain,
                si(bw, "Hz"),
                si(self.f_hz, "Hz"),
                a.gain_at(self.f_hz),
                used * 100.0,
            ),
        );
        let slow = a.gain_at(self.f_hz) < 0.9 * gain;
        widgets::status(
            ui,
            !clipped && !slow && used > 0.5,
            if clipped {
                "the output hits the supply voltage: the tops of the signal are cut off and that information is gone".to_owned()
            } else if slow {
                format!("the signal is too fast for this much gain: only {:.0} % of the intended gain is left", 100.0 * a.gain_at(self.f_hz) / gain)
            } else if used <= 0.5 {
                format!("the amplifier works, but the output only uses {:.0} % of the ADC's range: more gain would give a finer measurement", used * 100.0)
            } else {
                "the signal fills most of the ADC's range without touching the supply".to_owned()
            },
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "clip" => self.amp.r2_ohm = 47e3,
            "weak" => self.amp.r2_ohm = 1e3,
            "fast" => self.f_hz = 100e3,
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "signals-opamp",
        title: "Op-amp circuits: gain from two resistors",
        blurb: "A sensor delivers millivolts, the converter wants volts. An amplifier chip and two resistors bridge the gap.",
        path: &["Signals and analog"],
        make: || Box::new(OpAmp::default()),
        explain: Explanation {
            what: "The drawing shows the most common amplifier circuit there is: an [[op-amp]] (the triangle) and two resistors. \
                The sensor's signal goes in at the left, the amplified signal comes out at the right, on its way to the \
                [[ADC]].\n\n\
                The graph shows two periods of the small input signal and of the output. The dashed line at the top is the \
                supply voltage.",
            why: "Many sensors produce tiny signals: a few millivolts from a microphone, a thermocouple or a light sensor. An \
                ADC that divides 3.3 V into 4096 steps has steps of 0.8 mV, so a 10 mV signal would only ever show up as a \
                dozen different numbers. The signal has to be made bigger first, so that it fills the converter's range.\n\n\
                An op-amp is an amplifier with two inputs, marked + and -, and one output. By itself it is almost useless: \
                it amplifies the difference between its inputs a hundred thousand times or more, and that number varies \
                wildly from chip to chip and with temperature. The art is to tame it with feedback.\n\n\
                Two resistors feed a fixed fraction of the output back to the - input. Now think of what the op-amp does: \
                if the - input is the tiniest bit below the + input, the huge gain drives the output up; that raises the \
                - input; and it goes on until the two inputs are practically equal. So the output always ends up at \
                whatever value makes the fed-back fraction equal to the input signal. If the resistors feed back one \
                twenty-third of the output, the output must be 23 times the input.\n\n\
                The op-amp's own unreliable gain has dropped out of the result. What remains depends only on the ratio of \
                two resistors, which are cheap and precise. That is why this circuit is everywhere.\n\n\
                Two limits remain. The output cannot go beyond the supply voltage: ask for more and the signal is cut off \
                flat. And the op-amp has only so much gain to give at high frequencies; the more gain the circuit asks \
                for, the lower the frequency up to which it works. Gain times bandwidth is a constant of the chip.",
            formulas: &[
                ("gain = 1 + R2 / R1", "R2 = 22 kohm, R1 = 1 kohm: gain 23. A signal of 0 to 100 mV becomes 0 to 2.3 V."),
                ("bandwidth = gain-bandwidth product / gain", "An op-amp with 1 MHz, used at gain 23, works up to about 43 kHz."),
            ],
            try_this: &[
                Experiment {
                    action: "Raise R2 to 47 kohm.",
                    watch: "the gain doubling to 48 and the tops of the output being sliced off flat at the supply voltage. The dashed grey curve shows what was asked for.",
                    preset: Some("clip"),
                },
                Experiment {
                    action: "Lower R2 to 1 kohm.",
                    watch: "a gain of only 2: a clean signal that uses 6 % of the converter's range. Most of the ADC's steps are wasted.",
                    preset: Some("weak"),
                },
                Experiment {
                    action: "Back at 22 kohm, speed the signal up to 100 kHz.",
                    watch: "the output shrinking and lagging behind the input although nothing was changed in the circuit: 100 kHz is beyond the 43 kHz this gain leaves.",
                    preset: Some("fast"),
                },
            ],
            leaves_out: "The output here can swing all the way from 0 V to the supply (\"rail to rail\"); many op-amps stop a volt \
                or so short on either side. Real op-amps also add a small error voltage of their own (offset) and some noise, \
                both of which are amplified along with the signal, and they limit how fast the output can change. A signal \
                that swings below 0 V needs either a negative supply or a shifted reference point. This is one of several \
                standard circuits; the inverting amplifier, the buffer and the difference amplifier follow the same \
                reasoning.",
            refs: &["Non-inverting amplifier with negative feedback: gain = 1 + R2/R1; closed-loop bandwidth = gain-bandwidth product / gain. P. Horowitz and W. Hill, \"The Art of Electronics\", chapter 4; Texas Instruments, \"Op Amps for Everyone\"."],
        },
    }
}
