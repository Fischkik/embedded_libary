use crate::prelude::*;
use egui_plot::{HLine, MarkerShape, Points, Polygon};
use sim_core::mcu::Adc;

/// Highest voltage the input slider offers: a little above the largest reference, so
/// that "input above the reference" can be tried.
const VIN_MAX: f64 = 3.6;

pub struct AdcDemo {
    adc: Adc,
    vin: f64,
}

impl Default for AdcDemo {
    fn default() -> Self {
        Self { adc: Adc { bits: 6, vref: 3.3 }, vin: 2.0 }
    }
}

impl Demo for AdcDemo {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "The converter's settings", |ui| {
            ui.add(widgets::si_slider(&mut self.vin, 0.0..=VIN_MAX, "voltage on the pin", "V"))
                .on_hover_text("The voltage to be measured, for example from a temperature sensor or a battery.");
            ui.add(egui::Slider::new(&mut self.adc.bits, 1..=12).text("resolution").suffix(" bits"))
                .on_hover_text("How many bits the result has. Each extra bit doubles the number of steps. Most microcontrollers have 12.");
            ui.horizontal_wrapped(|ui| {
                ui.label("Reference voltage:");
                for v in [1.2, 2.5, 3.3] {
                    ui.selectable_value(&mut self.adc.vref, v, format!("{v} V"))
                        .on_hover_text("The top of the measuring range. The converter reports the input as a fraction of this voltage.");
                }
            });
        });

        let adc = self.adc;
        let bits = adc.bits as usize;
        let code = adc.convert(self.vin);
        let steps = adc.sar_steps(self.vin);
        let reported = adc.volts(code);
        let clipped = self.vin >= adc.vref;

        ui.label(RichText::new("How the converter finds the number: one yes/no question per bit").strong());
        plots::plot_bare("search", 230.0)
            .x_axis_label("question number (the last column is the result)")
            .y_axis_label("volts")
            .include_y(0.0)
            .include_y(adc.vref)
            .include_x(0.4)
            .include_x(bits as f64 + 1.6)
            .x_grid_spacer(egui_plot::uniform_grid_spacer(|_| [1.0, 1.0, 1.0]))
            .x_axis_formatter(move |mark, _| {
                let i = mark.value.round();
                if (mark.value - i).abs() > 0.01 || i < 1.0 || i > bits as f64 + 1.0 {
                    String::new()
                } else if i as usize == bits + 1 {
                    "result".to_owned()
                } else {
                    format!("{i:.0}")
                }
            })
            .show(ui, |p| {
                let band = pal.trace[0].gamma_multiply(0.35);
                let (mut lo, mut hi) = (0.0, adc.vref);
                let (mut higher, mut lower) = (Vec::new(), Vec::new());
                for (k, s) in steps.iter().enumerate() {
                    let x = k as f64 + 1.0;
                    p.polygon(Polygon::new("where the input can still be", plots::bar(x - 0.5, x + 0.5, lo, hi)).fill_color(band).stroke((0.0, band)));
                    p.line(plots::line("voltage it compares with", vec![[x - 0.5, s.trial_volts], [x + 0.5, s.trial_volts]], pal.trace[1]).width(2.5));
                    if s.keep { &mut higher } else { &mut lower }.push([x, s.trial_volts]);
                    (lo, hi) = (s.known_lo, s.known_hi);
                }
                let x = bits as f64 + 1.0;
                p.polygon(Polygon::new("where the input can still be", plots::bar(x - 0.5, x + 0.5, lo, hi)).fill_color(band).stroke((1.0, pal.trace[0])));
                p.points(Points::new("answer: input is higher, bit = 1", higher).shape(MarkerShape::Up).radius(7.0).color(pal.text));
                p.points(Points::new("answer: input is lower, bit = 0", lower).shape(MarkerShape::Down).radius(7.0).color(pal.text));
                p.hline(HLine::new("voltage on the pin", self.vin).color(pal.trace[2]).width(2.0));
            });

        widgets::key(
            ui,
            &[
                (pal.trace[0].gamma_multiply(0.5), "blue block: where the input can still be"),
                (pal.trace[1], "orange line: voltage it compares with"),
                (pal.trace[2], "green line: voltage on the pin"),
                (pal.text, "triangle up: input is higher, bit = 1; down: lower, bit = 0"),
            ],
        );
        widgets::formula(
            ui,
            format!(
                "one step (1 LSB)   = reference / 2^bits = {} / {} = {}\n\
                 result             = input / step, rounded down = {} / {} = {code}   (binary {code:0bits$b})\n\
                 firmware reads it as {code} x {} = {}        error: {}",
                si(adc.vref, "V"),
                adc.levels(),
                si(adc.lsb(), "V"),
                si(self.vin, "V"),
                si(adc.lsb(), "V"),
                si(adc.lsb(), "V"),
                si(reported, "V"),
                si(self.vin - reported, "V"),
            ),
        );
        widgets::status(
            ui,
            !clipped,
            if clipped {
                format!("the input is at or above the reference: every voltage from {} upward gives the same result, {code}", si(adc.vref, "V"))
            } else {
                format!("the input is inside the measuring range, 0 V to {}; the error is less than one step", si(adc.vref, "V"))
            },
        );

        ui.label(RichText::new("What the firmware reads for every possible input").strong());
        let mut stairs = Vec::with_capacity(adc.levels() as usize * 2 + 1);
        for c in 0..adc.levels() {
            stairs.push([adc.volts(c), adc.volts(c)]);
            stairs.push([adc.volts(c + 1), adc.volts(c)]);
        }
        stairs.push([VIN_MAX, adc.volts(adc.levels() - 1)]);
        plots::plot("transfer", 240.0)
            .x_axis_label("true voltage on the pin [V]")
            .y_axis_label("voltage the firmware reads [V]")
            .include_x(0.0)
            .include_x(VIN_MAX)
            .include_y(0.0)
            .include_y(VIN_MAX)
            .show(ui, |p| {
                p.line(plots::line("a perfect measurement", vec![[0.0, 0.0], [VIN_MAX, VIN_MAX]], pal.dim).style(egui_plot::LineStyle::dashed_dense()));
                p.line(plots::line("this converter", stairs, pal.trace[0]).width(2.0));
                p.points(Points::new("the current measurement", vec![[self.vin, reported]]).radius(6.0).color(pal.trace[2]));
            });
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "coarse" => self.adc.bits = 3,
            "fine" => self.adc.bits = 12,
            "small" => {
                self.vin = 0.1;
                self.adc.vref = 1.2;
            }
            "clip" => self.adc.vref = 1.2,
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "mcu-adc",
        title: "ADC: measuring a voltage",
        blurb: "A microcontroller only understands numbers. See how it turns a voltage into one, question by question.",
        path: &["Inside the microcontroller"],
        make: || Box::new(AdcDemo::default()),
        explain: Explanation {
            what: "The green line in the top graph is the voltage on a pin. The microcontroller's [[ADC]] has to turn it into a \
                whole number. Read the graph from left to right: each column is one yes/no question. The blue block is the range \
                the input could still be in, the orange line is the voltage the converter compares it with, and the triangle is \
                the answer - pointing up for \"the input is higher\", down for \"the input is lower\". Every answer halves the \
                blue block. The last column is what remains: one small step.\n\n\
                The bottom graph shows the outcome for every possible input at once: a staircase. The dot is the current \
                measurement.",
            why: "A digital pin can only tell \"high\" from \"low\". To measure how warm a sensor is or how full a battery is, the \
                microcontroller needs the size of a voltage as a number. That is the job of the analog-to-digital converter.\n\n\
                It needs a yardstick: the [[reference voltage]]. The converter does not measure volts as such, it measures what \
                fraction of the reference the input is. The yardstick is divided into equal steps, and the number of steps \
                depends on the [[resolution]]: with N bits there are 2 to the power N steps. 6 bits give 64 steps, 12 bits give \
                4096.\n\n\
                The converter in most microcontrollers finds the right step with a guessing game, like finding a word in a \
                dictionary by always opening the remaining part in the middle. It makes a trial voltage at half the reference \
                and asks a [[comparator]]: is the input higher? If yes, the first [[bit]] of the result is 1 and the input must \
                be in the upper half; if no, the bit is 0 and it is in the lower half. Then it halves the remaining range again \
                for the next bit, and so on. N bits take N questions. This method is called successive approximation.\n\n\
                Whatever is left after the last question cannot be resolved: all voltages inside one step give the same \
                number. The width of one step is called one [[LSB]], because it is what the last bit of the result is worth. \
                That is why the bottom graph is a staircase and not a straight line.\n\n\
                Two things make the steps smaller. More bits, which the hardware fixes. Or a lower reference voltage, which \
                spreads the same number of steps over a smaller range - at the price that nothing above the reference can be \
                measured any more.",
            formulas: &[
                ("step = reference / 2^bits", "3.3 V split into 4096 steps (12 bits): each step is 0.8 mV."),
                ("result = input / step, rounded down", "2 V with 51.56 mV steps (6 bits): 2 / 0.05156 = 38.8, so the result is 38."),
                ("voltage = result x reference / 2^bits", "What firmware does to turn the number back into volts: 38 x 51.56 mV = 1.959 V."),
            ],
            try_this: &[
                Experiment {
                    action: "Drag the voltage slowly up and down.",
                    watch: "the triangles flipping and the blue blocks closing in on the green line from different sides. The result only changes when the line crosses into another step.",
                    preset: None,
                },
                Experiment {
                    action: "Set the resolution to 3 bits.",
                    watch: "just three questions and eight stairs, each 0.41 V high. The firmware's reading can be wrong by almost half a volt.",
                    preset: Some("coarse"),
                },
                Experiment {
                    action: "Set the resolution to 12 bits, the usual value in a microcontroller.",
                    watch: "twelve questions, and a staircase so fine that it looks like the straight dashed line. One step is now below a millivolt.",
                    preset: Some("fine"),
                },
                Experiment {
                    action: "Measure a small voltage, 0.1 V, and switch the reference between 3.3 V and 1.2 V.",
                    watch: "the step size in the formula. With the lower reference the same 6 bits cover a smaller range, so the small voltage is measured almost three times as finely.",
                    preset: Some("small"),
                },
                Experiment {
                    action: "Keep the 1.2 V reference and raise the input to 2 V.",
                    watch: "every answer being \"higher\", the result stuck at its highest number, and the staircase running flat: above the reference the converter is blind.",
                    preset: Some("clip"),
                },
            ],
            leaves_out: "This is an ideal converter. A real one adds noise, so the last bit or two flicker; its steps are not all \
                exactly equal, and the whole staircase can be shifted or tilted a little (offset and gain error). Real converters \
                also place the steps half a step lower, so that the error is at most plus or minus half a step instead of \
                always downward. Taking a measurement takes time: the input is first captured on a small capacitor, which \
                needs a low-resistance source, and then each question takes one clock cycle. How often to measure a changing \
                signal is a topic of its own (sampling). A voltage above the supply voltage does not just read as the maximum, \
                it can damage the pin. Other converter types exist, such as sigma-delta converters for very high resolution.",
            refs: &[
                "Ideal N-bit converter transfer function: step = reference / 2^N, result = floor(input / step).",
                "Successive-approximation search, most significant bit first: W. Kester, Analog Devices tutorial MT-021 \"Successive Approximation ADCs\"; Analog Devices (Maxim), \"Understanding SAR ADCs: Their Architecture and Comparison with Other ADCs\".",
            ],
        },
    }
}
