use crate::prelude::*;
use egui_plot::HLine;
use sim_core::signals::{Buck as BuckModel, BuckTrace};

const SHOW_PERIODS: usize = 3;

pub struct Buck {
    b: BuckModel,
    load_ma: f64,
    /// The last simulation and the settings it was made with: running it takes up to a
    /// few million steps, so it is only repeated when a setting changes.
    cache: Option<(BuckModel, BuckTrace)>,
}

impl Default for Buck {
    fn default() -> Self {
        Self { b: BuckModel { vin: 12.0, duty: 0.275, f_sw_hz: 500e3, l_h: 47e-6, c_f: 22e-6, load_ohm: 6.6 }, load_ma: 500.0, cache: None }
    }
}

impl Demo for Buck {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        let b = &mut self.b;
        widgets::group(ui, "A converter from 12 V to a lower voltage", |ui| {
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "duty", |ui| ui.add(egui::Slider::new(&mut b.duty, 0.05..=0.95).text("switch on for").custom_formatter(|v, _| format!("{:.1} %", v * 100.0))).on_hover_text("The duty cycle: the share of each period in which the switch connects the input to the inductor."));
                widgets::wrap(ui, "f", |ui| ui.add(widgets::si_slider(&mut b.f_sw_hz, 50e3..=2e6, "switching frequency", "Hz").logarithmic(true)).on_hover_text("How many times per second the switch turns on and off."));
            });
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "l", |ui| ui.add(widgets::si_slider(&mut b.l_h, 4.7e-6..=220e-6, "inductor", "H").logarithmic(true)).on_hover_text("The coil that smooths the current. Bigger means less ripple, but a bigger, heavier part."));
                widgets::wrap(ui, "load", |ui| ui.add(egui::Slider::new(&mut self.load_ma, 5.0..=1500.0).logarithmic(true).text("current drawn by the load").suffix(" mA").fixed_decimals(0)).on_hover_text("How much current the circuit behind the converter takes, for example a microcontroller and a radio."));
            });
        });
        b.load_ohm = (b.duty * b.vin / (self.load_ma / 1000.0)).max(0.1);
        let b = self.b;
        if self.cache.as_ref().is_none_or(|(m, _)| *m != b) {
            self.cache = Some((b, b.simulate(SHOW_PERIODS)));
        }
        let tr = &self.cache.as_ref().expect("filled above").1;
        let us = 1e6 / b.f_sw_hz;
        let t_us: Vec<f64> = tr.t.iter().map(|t| t * us).collect();
        let xy = |ys: &[f64]| -> Vec<[f64; 2]> { t_us.iter().zip(ys).map(|(t, y)| [*t, *y]).collect() };

        ui.label(RichText::new("Three switching periods, after the converter has settled").strong());
        plots::plot_bare("switch", 140.0).x_axis_label("time [µs]").y_axis_label("switch [V]").include_y(-0.5).include_y(b.vin + 1.0).show(ui, |p| {
            p.line(plots::line("switch node", xy(&tr.v_switch), pal.trace[1]).width(2.0));
        });
        plots::plot_bare("current", 150.0).x_axis_label("time [µs]").y_axis_label("coil [A]").include_y(0.0).show(ui, |p| {
            p.line(plots::line("inductor current", xy(&tr.i_l), pal.trace[0]).width(2.0));
            p.hline(HLine::new("load current", self.load_ma / 1000.0).color(pal.dim).style(egui_plot::LineStyle::dashed_dense()));
        });
        // A fixed window of +-2 % around the output, so that the ripple is shown at its true size.
        let half = (0.02 * tr.v_out_mean).max(tr.v_ripple);
        ui.label(RichText::new(format!("Output voltage, zoomed in to {} around its average", si(2.0 * half, "V"))).small().color(pal.dim));
        plots::plot_bare("out", 150.0).x_axis_label("time [µs]").y_axis_label("output [V]").include_y(tr.v_out_mean - half).include_y(tr.v_out_mean + half).show(ui, |p| {
            p.line(plots::line("output", xy(&tr.v_out), pal.trace[2]).width(2.0));
            p.hline(HLine::new("duty x input", b.ideal_vout()).color(pal.dim).style(egui_plot::LineStyle::dashed_dense()));
        });
        widgets::key(ui, &[(pal.trace[1], "voltage on the switch side of the coil"), (pal.trace[0], "current through the coil"), (pal.trace[2], "output voltage"), (pal.dim, "dashed: load current / duty x input voltage")]);

        // Where the power goes: this converter (ideal here) against a linear regulator.
        let p_out = tr.v_out_mean * tr.v_out_mean / b.load_ohm;
        let linear_loss = (b.vin - tr.v_out_mean) * tr.v_out_mean / b.load_ohm;
        ui.label(RichText::new("Heat lost making this voltage").strong());
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width().min(560.0), 52.0), egui::Sense::hover());
        let painter = ui.painter();
        let full = (p_out + linear_loss).max(1e-9);
        let bar = |row: f32, loss: f64, name: &str| {
            let y = rect.top() + row * 26.0;
            let w = rect.width() - 190.0;
            let out_w = (p_out / full) as f32 * w;
            let loss_w = (loss / full) as f32 * w;
            painter.text(pos2(rect.left(), y + 10.0), egui::Align2::LEFT_CENTER, name, egui::FontId::proportional(13.0), pal.text);
            let x0 = rect.left() + 150.0;
            painter.rect_filled(egui::Rect::from_min_size(pos2(x0, y + 2.0), vec2(out_w, 16.0)), 2.0, pal.good);
            painter.rect_filled(egui::Rect::from_min_size(pos2(x0 + out_w, y + 2.0), vec2(loss_w.max(1.0), 16.0)), 2.0, pal.bad);
        };
        bar(0.0, 0.0, "switching converter");
        bar(1.0, linear_loss, "linear regulator");
        widgets::key(ui, &[(pal.good, "power delivered to the load"), (pal.bad, "power turned into heat")]);

        widgets::formula(
            ui,
            format!(
                "output = duty x input = {:.3} x {} = {}        simulated average: {}\n\
                 coil current ripple = (input - output) x duty / (inductor x frequency)\n\
                 \x20                   = ({} - {}) x {:.3} / ({} x {}) = {}        simulated: {}\n\
                 linear regulator: efficiency = output / input = {:.0} %, heat = {}   (ideal switching converter: none)",
                b.duty,
                si(b.vin, "V"),
                si(b.ideal_vout(), "V"),
                si(tr.v_out_mean, "V"),
                si(b.vin, "V"),
                si(b.ideal_vout(), "V"),
                b.duty,
                si(b.l_h, "H"),
                si(b.f_sw_hz, "Hz"),
                si(b.ideal_ripple_a(), "A"),
                si(tr.i_ripple, "A"),
                100.0 * tr.v_out_mean / b.vin,
                si(linear_loss, "W"),
            ),
        );
        widgets::status(
            ui,
            !tr.discontinuous,
            if tr.discontinuous {
                format!("the coil current drops to zero in every period: the load is too light for this coil, and the output drifts up to {} instead of {}", si(tr.v_out_mean, "V"), si(b.ideal_vout(), "V"))
            } else {
                format!("the coil current never stops flowing, and the output sits at duty x input with {} of ripple", si(tr.v_ripple, "V"))
            },
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "five" => self.b.duty = 5.0 / 12.0,
            "small-l" => self.b.l_h = 4.7e-6,
            "fast" => self.b.f_sw_hz = 2e6,
            "light" => self.load_ma = 10.0,
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "signals-buck",
        title: "Switching power supplies",
        blurb: "Making 3.3 V from 12 V without turning most of the power into heat: a switch, a coil and a capacitor.",
        path: &["Signals and analog"],
        make: || Box::new(Buck::default()),
        explain: Explanation {
            what: "Three graphs of a [[buck converter]], the circuit that makes a lower voltage from a higher one in almost every \
                battery-powered or USB-powered device. The top graph is a switch flicking between the 12 V input and 0 V, \
                half a million times per second. The middle graph is the current through a coil (an [[inductor]]) behind \
                it. The bottom graph is the output: a steady voltage with a tiny ripple.\n\n\
                The bars underneath compare the heat lost with that of the simple alternative, a linear regulator.",
            why: "A microcontroller wants 3.3 V; the supply delivers 12 V. The simplest way down is a linear regulator: in effect \
                a resistor that adjusts itself so that exactly 3.3 V remain. It works, and it is quiet, but all of the \
                difference is burnt. Of 12 V, 8.7 V are dropped across the regulator at the full load current: 73 % of the \
                power becomes heat. For a battery, that is three quarters of its life gone.\n\n\
                A switching converter avoids burning anything. Its switch is either fully on or fully off, and a switch \
                that is fully on or fully off wastes almost nothing. Like the [[PWM]] demo, it makes an average: on for 27.5 % \
                of the time, the average of 12 V is 3.3 V.\n\n\
                But a load cannot run on 12 V pulses. That is the coil's job. A coil resists changes of current: while the \
                switch is on, the current through it rises steadily; while it is off, the coil keeps the current flowing, \
                now through a diode, and it falls steadily. The current ends up as a gentle sawtooth around the load \
                current, and a capacitor at the output smooths the last of the ripple away. Energy is stored in the coil's \
                magnetic field during one half of the period and handed on in the other, instead of being burnt.\n\n\
                The design trades are visible in the graphs. Switching faster or using a bigger coil makes the sawtooth \
                smaller, but faster switching costs losses in the switch and a bigger coil costs space and money. And if the \
                load takes so little current that the sawtooth reaches zero, the coil runs empty in every period, and the \
                simple rule \"output = duty x input\" stops holding: the controller has to correct for it.",
            formulas: &[
                ("output voltage = duty cycle x input voltage", "27.5 % of 12 V = 3.3 V."),
                ("ripple of the coil current = (input - output) x duty / (inductance x frequency)", "(12 - 3.3) x 0.275 / (47 µH x 500 kHz) = 102 mA, around a load current of 500 mA."),
                ("linear regulator efficiency = output voltage / input voltage", "3.3 V from 12 V: 27.5 %. The rest is heat."),
            ],
            try_this: &[
                Experiment {
                    action: "Make 5 V instead: set the duty to 41.7 %.",
                    watch: "the output moving to 5 V with no other change - the duty cycle alone sets it. The linear regulator's waste shrinks too, to 58 %.",
                    preset: Some("five"),
                },
                Experiment {
                    action: "Fit a ten times smaller coil, 4.7 µH.",
                    watch: "the current's sawtooth growing ten times taller, past a full ampere, and the output ripple growing with it.",
                    preset: Some("small-l"),
                },
                Experiment { action: "Switch at 2 MHz instead of 500 kHz.", watch: "the ripple shrinking to a quarter: the same coil now has to bridge only a quarter of the time.", preset: Some("fast") },
                Experiment {
                    action: "Let the load draw only 10 mA.",
                    watch: "the coil current hitting zero and staying there for part of each period, and the output creeping above 3.3 V. Real converters measure the output and adjust the duty for exactly this reason.",
                    preset: Some("light"),
                },
            ],
            leaves_out: "Everything is ideal: the switch, diode, coil and capacitor waste nothing, so this converter is 100 % efficient; \
                real ones reach roughly 85 to 95 %. The duty cycle is fixed here; a real converter measures its output and \
                adjusts the duty continuously, which also makes it follow changes of input voltage and load. Many converters \
                replace the diode with a second switch, and at light load switch to a burst mode to save power. The fast \
                switching edges make electrical noise that a radio or an ADC nearby can pick up - one reason linear \
                regulators are still used for sensitive analog parts.",
            refs: &["Ideal buck converter in continuous conduction: V_out = D x V_in, inductor ripple = (V_in - V_out) D / (L f); R. W. Erickson and D. Maksimovic, \"Fundamentals of Power Electronics\", chapter 2; Texas Instruments application report SLVA477, \"Basic Calculation of a Buck Converter's Power Stage\"."],
        },
    }
}
