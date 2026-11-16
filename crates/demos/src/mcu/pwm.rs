use crate::prelude::*;
use sim_core::mcu::PwmTimer;

const VDD: f64 = 3.3;

pub struct Pwm {
    timer: PwmTimer,
}

impl Default for Pwm {
    fn default() -> Self {
        Self { timer: PwmTimer { clk_hz: 48e6, prescaler: 47, reload: 999, compare: 250, center_aligned: false } }
    }
}

impl Demo for Pwm {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        let t = &mut self.timer;
        widgets::group(ui, "The timer's settings", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Clock:");
                for mhz in [8.0, 16.0, 48.0, 72.0] {
                    ui.selectable_value(&mut t.clk_hz, mhz * 1e6, format!("{mhz:.0} MHz")).on_hover_text("The clock that feeds the timer, usually the same as the processor clock.");
                }
            });
            ui.add(egui::Slider::new(&mut t.prescaler, 0..=4799).logarithmic(true).text("prescaler"))
                .on_hover_text("Divides the clock: the counter advances once every (prescaler + 1) clock cycles.");
            ui.add(egui::Slider::new(&mut t.reload, 1..=65535).logarithmic(true).text("reload value"))
                .on_hover_text("The counter counts from 0 up to this number, then starts again. It sets the length of one period.");
            t.compare = t.compare.min(t.reload + 1);
            ui.add(egui::Slider::new(&mut t.compare, 0..=t.reload + 1).text("compare value"))
                .on_hover_text("The output pin is high while the counter is below this number.");
            ui.checkbox(&mut t.center_aligned, "count up and down instead of up only (centre-aligned)")
                .on_hover_text("The pulses then stay centred instead of all starting at the same moment. Motor drives use this.");
        });
        let t = self.timer;
        let period_s = 1.0 / t.pwm_hz();
        let tick_s = 1.0 / t.tick_hz();

        widgets::formula(
            ui,
            format!(
                "counter ticks at   clock / (prescaler + 1) = {} / {} = {}\n\
                 PWM frequency    = tick rate / {} = {}        (one period = {})\n\
                 duty cycle       = compare / {} = {:.1} %        smallest step: {:.3} %\n\
                 average voltage  = {VDD} V x duty cycle = {:.2} V",
                si(t.clk_hz, "Hz"),
                t.prescaler + 1,
                si(t.tick_hz(), "Hz"),
                if t.center_aligned { format!("(2 x reload = {})", t.period_ticks()) } else { format!("(reload + 1 = {})", t.period_ticks()) },
                si(t.pwm_hz(), "Hz"),
                si(period_s, "s"),
                if t.center_aligned { "reload".to_owned() } else { "(reload + 1)".to_owned() },
                t.duty() * 100.0,
                t.duty_step() * 100.0,
                t.average_volts(VDD),
            ),
        );

        // Three periods, drawn from corner points so any reload value stays cheap.
        let periods = 3;
        let scale = if period_s < 1e-3 { (1e6, "µs") } else { (1e3, "ms") };
        let ticks = t.period_ticks() as f64;
        let mut counter = Vec::new();
        let mut edges: Vec<(f64, bool)> = Vec::new();
        for k in 0..periods {
            let t0 = k as f64 * period_s * scale.0;
            let x = |tick: f64| t0 + tick * tick_s * scale.0;
            if t.center_aligned {
                let r = t.reload.max(1) as f64;
                counter.extend([[x(0.0), 0.0], [x(r), r], [x(2.0 * r), 0.0]]);
                let c = (t.compare as f64).min(r);
                if c > 0.0 {
                    edges.extend([(x(0.0), true), (x(c), false), (x(2.0 * r - c), true)]);
                }
            } else {
                counter.extend([[x(0.0), 0.0], [x(ticks), t.reload as f64], [x(ticks), 0.0]]);
                let c = (t.compare as f64).min(ticks);
                if c > 0.0 {
                    edges.push((x(0.0), true));
                }
                if c < ticks {
                    edges.push((x(c), false));
                }
            }
        }
        let span = periods as f64 * period_s * scale.0;

        ui.label(RichText::new("The counter and the compare value").strong());
        plots::plot("counter", 220.0).x_axis_label(format!("time [{}]", scale.1)).y_axis_label("counter").include_y(0.0).include_x(0.0).include_x(span).show(ui, |p| {
            p.line(plots::line("counter", counter, pal.trace[0]));
            p.hline(egui_plot::HLine::new("compare value", t.compare as f64).color(pal.trace[1]).width(2.0));
        });
        ui.label(RichText::new("The output pin").strong());
        plots::plot("output", 150.0).x_axis_label(format!("time [{}]", scale.1)).y_axis_label("volts").include_y(-0.3).include_y(VDD + 0.5).include_x(0.0).include_x(span).show(ui, |p| {
            p.line(plots::line("pin", plots::digital(&edges, false, 0.0, span, 0.0, VDD), pal.trace[2]).width(2.0));
            p.hline(egui_plot::HLine::new("average voltage", t.average_volts(VDD)).color(pal.trace[3]).style(egui_plot::LineStyle::dashed_dense()));
        });

        // What it does to a lamp.
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(70.0, 70.0), egui::Sense::hover());
            // The eye judges brightness roughly logarithmically, so show a perceptual value.
            let seen = (t.duty() as f32).powf(1.0 / 2.2);
            let lamp = Color32::from_rgb((255.0 * seen) as u8, (220.0 * seen) as u8, (90.0 * seen) as u8);
            ui.painter().circle_filled(rect.center(), 30.0, lamp);
            ui.painter().circle_stroke(rect.center(), 30.0, (1.5, pal.dim));
            ui.vertical(|ui| {
                ui.label(RichText::new("An LED on this pin").strong());
                if t.pwm_hz() < 100.0 {
                    ui.label(RichText::new(format!("At {} it visibly flickers. Lighting needs a few hundred hertz at least.", si(t.pwm_hz(), "Hz"))).color(pal.bad));
                } else {
                    ui.label(format!("At {} the eye sees steady light at {:.0} % power.", si(t.pwm_hz(), "Hz"), t.duty() * 100.0));
                }
                ui.label(RichText::new("The eye is far more sensitive to changes at the dim end: 10 % power already looks about a third as bright.").small().color(pal.dim));
            });
        });
    }

    fn apply_preset(&mut self, preset: &str) {
        match preset {
            "coarse" => {
                self.timer.reload = 9;
                self.timer.compare = 3;
                self.timer.prescaler = 4799;
            }
            "flicker" => {
                self.timer.prescaler = 4799;
                self.timer.reload = 499;
                self.timer.compare = 250;
            }
            "center" => self.timer.center_aligned = true,
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "mcu-pwm",
        title: "Timers and PWM",
        blurb: "A counter, two numbers to compare it with, and a pin: enough to dim a lamp or steer a motor.",
        path: &["Inside the microcontroller"],
        make: || Box::new(Pwm::default()),
        explain: Explanation {
            what: "The top graph is the value of a hardware [[counter]] inside the microcontroller: it climbs steadily, reaches \
                its limit, and starts again - a sawtooth. The orange line is the [[compare value]]. The bottom graph is an output \
                pin, which the hardware keeps high for as long as the counter is below the orange line.\n\n\
                The dashed line is the average voltage of that pin, and the lamp shows what an LED connected to it would look like.",
            why: "A microcontroller pin can only be fully on or fully off. To get \"half brightness\" you switch it on and off so \
                fast that the lamp, a motor, or the eye only responds to the average. Varying the share of on-time is \
                [[PWM]], pulse-width modulation, and the share itself is the [[duty cycle]].\n\n\
                Doing this in software would keep the processor busy and jitter whenever it had something else to do. So every \
                microcontroller has [[timer]]s: hardware counters that tick along with the clock all by themselves. You configure \
                three numbers and the hardware does the rest.\n\n\
                The [[prescaler]] slows the clock down: the counter advances once every (prescaler + 1) clock cycles. The \
                [[reload value]] is where the counter wraps around, so it sets the length of one [[period]]. The compare value sets \
                where within the period the pin switches off.\n\n\
                There is a trade-off hiding here. The reload value is also the number of brightness steps you get: with a reload \
                value of 9 there are only ten possible duty cycles. More steps need a higher reload value, which makes each period \
                longer, which lowers the PWM frequency - unless you lower the prescaler or raise the clock. Frequency and \
                resolution compete for the same clock ticks.\n\n\
                The same timer hardware is behind almost everything time-related in firmware: periodic interrupts, measuring \
                pulse lengths, generating tones, driving servos.",
            formulas: &[
                ("PWM frequency = clock / ((prescaler + 1) x (reload + 1))", "With 48 MHz, prescaler 47 and reload 999: 48 000 000 / (48 x 1000) = 1 kHz."),
                ("duty cycle = compare / (reload + 1)", "Compare 250 of 1000 ticks: high for 25 % of the time."),
            ],
            try_this: &[
                Experiment {
                    action: "Drag the compare value from 0 to its maximum.",
                    watch: "the pulses widening, the dashed average line rising, and the lamp brightening - quickly at first, then hardly at all.",
                    preset: None,
                },
                Experiment {
                    action: "Set the reload value to 9.",
                    watch: "the compare slider now having only a handful of positions: 10 % steps. Coarse resolution is the price of a short period.",
                    preset: Some("coarse"),
                },
                Experiment {
                    action: "Set prescaler 4799 and reload 499.",
                    watch: "the PWM frequency dropping to 20 Hz and the warning that a lamp would visibly flicker.",
                    preset: Some("flicker"),
                },
                Experiment {
                    action: "Tick \"count up and down\".",
                    watch: "the sawtooth becoming a triangle and each pulse sitting centred on the triangle's low point instead of starting at the period boundary.",
                    preset: Some("center"),
                },
            ],
            leaves_out: "The register names differ from chip to chip (ARR and CCR on one family, TOP and CC on another); the \
                behaviour is the same. Real timers have several compare channels sharing one counter, update the settings only at \
                the end of a period to avoid glitches, and offer inverted outputs and dead time for driving motor bridges.",
            refs: &["Generic up-counting and up/down-counting PWM as found on ARM Cortex-M microcontroller timers."],
        },
    }
}
