use crate::prelude::*;
use egui_plot::Points;
use sim_core::mcu::{Debounce, bouncy_button, sample_and_debounce};
use sim_core::rng::Rng;

const TOTAL_MS: f64 = 200.0;
const PRESS_MS: f64 = 30.0;
const RELEASE_MS: f64 = 130.0;

pub struct Gpio {
    bounce_ms: f64,
    bounces: u32,
    sample_ms: f64,
    debounce: bool,
    stable_samples: u32,
    seed: u64,
}

impl Default for Gpio {
    fn default() -> Self {
        Self { bounce_ms: 5.0, bounces: 4, sample_ms: 0.5, debounce: false, stable_samples: 10, seed: 4 }
    }
}

impl Demo for Gpio {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "The button (a mechanical switch between the pin and ground, with a pull-up resistor)", |ui| {
            ui.add(egui::Slider::new(&mut self.bounce_ms, 0.0..=15.0).text("contacts chatter for").suffix(" ms").fixed_decimals(1))
                .on_hover_text("How long the metal contacts keep bouncing after they first touch or let go. Typical switches: 1 to 10 ms.");
            ui.add(egui::Slider::new(&mut self.bounces, 0..=10).text("number of bounces"))
                .on_hover_text("How many times the contact opens and closes again before it settles.");
            if ui.button("Press the button again").on_hover_text("Every press bounces a little differently").clicked() {
                self.seed += 1;
            }
        });
        widgets::group(ui, "The firmware", |ui| {
            ui.add(egui::Slider::new(&mut self.sample_ms, 0.1..=20.0).logarithmic(true).text("reads the pin every").suffix(" ms").fixed_decimals(1))
                .on_hover_text("How often the software looks at the pin, for example from a timer interrupt.");
            ui.checkbox(&mut self.debounce, "debounce: only believe a change after several identical readings in a row")
                .on_hover_text("Switch on to filter out the bouncing in software.");
            ui.add_enabled(self.debounce, egui::Slider::new(&mut self.stable_samples, 2..=30).text("identical readings needed"))
                .on_hover_text("How many readings in a row must agree before the software accepts the new level. More = safer against bounce, but slower to react.");
        });

        let edges = bouncy_button(PRESS_MS, RELEASE_MS, self.bounce_ms, self.bounces, &mut Rng::new(self.seed));
        let strategy = if self.debounce { Debounce::Stable { samples: self.stable_samples } } else { Debounce::None };
        let trace = sample_and_debounce(&edges, self.sample_ms, TOTAL_MS, strategy);
        // With a pull-up, the pin is HIGH when the button is open and LOW when pressed.
        let pin: Vec<(f64, bool)> = edges.iter().map(|(t, closed)| (*t, !closed)).collect();

        let draw = |ui: &mut Ui, id: &str, t0: f64, t1: f64, height: f32, show_samples: bool| {
            plots::plot(id, height)
                .x_axis_label("time [ms]")
                .show_axes([true, false])
                .include_y(-0.3)
                .include_y(3.6)
                .include_x(t0)
                .include_x(t1)
                .show(ui, |p| {
                    p.line(plots::line("pin voltage (high = not pressed)", plots::digital(&pin, true, t0, t1, 2.2, 1.0), pal.trace[0]));
                    if show_samples {
                        let pts: Vec<[f64; 2]> = trace.samples.iter().filter(|s| s.0 >= t0 && s.0 <= t1).map(|(t, pressed)| [*t, if *pressed { 2.2 } else { 3.2 }]).collect();
                        p.points(Points::new("firmware reads the pin", pts).radius(2.5).color(pal.trace[1]));
                    }
                    p.line(plots::line("what the firmware believes (high = pressed)", plots::digital(&trace.accepted, false, t0, t1, 0.2, 1.0), pal.trace[2]).width(2.2));
                });
        };
        ui.label(RichText::new("One press and release, 200 ms").strong());
        draw(ui, "whole", 0.0, TOTAL_MS, 210.0, false);
        ui.label(RichText::new("Zoomed in on the moment of pressing").strong());
        draw(ui, "zoom", PRESS_MS - 2.0, PRESS_MS + 20.0, 230.0, true);

        let delay = trace.accepted.first().map(|e| e.0 - PRESS_MS);
        widgets::status(
            ui,
            trace.presses == 1,
            match trace.presses {
                1 => format!("the firmware counted exactly 1 press, {:.1} ms after the finger touched the button", delay.unwrap_or(0.0)),
                0 => "the firmware never noticed the press".to_owned(),
                n => format!("the firmware counted {n} presses for a single push of the button"),
            },
        );
        if self.debounce {
            widgets::formula(ui, format!("reaction delay = readings needed x time between readings = {} x {:.1} ms = {:.1} ms", self.stable_samples, self.sample_ms, self.stable_samples as f64 * self.sample_ms));
        }
    }

    fn apply_preset(&mut self, preset: &str) {
        match preset {
            "debounced" => self.debounce = true,
            "slow-sampling" => self.sample_ms = 20.0,
            "too-short" => {
                self.debounce = true;
                self.stable_samples = 3;
                self.bounce_ms = 12.0;
                self.bounces = 10;
            }
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "mcu-gpio",
        title: "A pin and a bouncing button",
        blurb: "Reading a push button sounds trivial. Then you look at the signal.",
        path: &["Inside the microcontroller"],
        make: || Box::new(Gpio::default()),
        explain: Explanation {
            what: "The blue trace is the voltage on a microcontroller input pin connected to a push button. The button is pressed \
                at 30 ms and released at 130 ms. Instead of one clean step down and one clean step up, the signal chatters.\n\n\
                The orange dots in the zoomed graph are the moments the firmware reads the pin. The green trace is what the \
                firmware concludes: button pressed, or not.",
            why: "A [[GPIO]] pin can be switched to act as an input: the firmware can then ask \"is this pin at a high or a low \
                voltage right now?\" and gets a 1 or a 0.\n\n\
                The usual way to wire a button is between the pin and ground, with a [[pull-up]] resistor from the pin to the \
                supply voltage. While the button is open, the resistor holds the pin high. Pressing the button connects the pin to \
                ground and it reads low. So \"pressed\" is a 0 - the opposite of what you might expect.\n\n\
                The trouble is mechanical. The two pieces of metal inside a switch do not close once; they hit, spring apart, and \
                hit again several times over a few milliseconds. This is contact [[bounce]]. A human cannot perceive it, but a \
                microcontroller reading the pin thousands of times per second sees every single bounce as a separate press. A menu \
                that skips two entries per click, or a counter that counts 5 for one push, is almost always this.\n\n\
                The fix is called [[debounce]]. The simplest form: read the pin at regular intervals and only accept a new state \
                once the same value has been read several times in a row. The chatter never produces that many identical readings, \
                so it is ignored. The price is a small delay before a real press is recognised.",
            formulas: &[
                ("reaction delay = readings needed x time between readings", "Must be longer than the bouncing lasts, and short enough that the user does not notice: 10 to 30 ms is typical."),
            ],
            try_this: &[
                Experiment {
                    action: "Leave debouncing off and press the button again a few times.",
                    watch: "the status line: one push is counted as several presses, and a different number each time.",
                    preset: None,
                },
                Experiment {
                    action: "Switch debouncing on.",
                    watch: "the green trace waiting until the blue one has settled, then changing exactly once.",
                    preset: Some("debounced"),
                },
                Experiment {
                    action: "With debouncing on, require only 3 identical readings while the contacts chatter for 12 ms.",
                    watch: "extra presses sneaking through again: 3 readings x 0.5 ms is shorter than the bouncing.",
                    preset: Some("too-short"),
                },
                Experiment {
                    action: "Switch debouncing off and read the pin only every 20 ms.",
                    watch: "the problem disappearing by itself - slow reading skips over the bounce. This is the cheapest debounce there is.",
                    preset: Some("slow-sampling"),
                },
            ],
            leaves_out: "The voltage jumps instantly between high and low; a real pin has some capacitance that rounds the edges, \
                and adding a small capacitor is a hardware way to debounce. The pin's input threshold and noise from long wires \
                are ignored. Buttons can also be handled by an [[interrupt]] instead of regular reading, which needs debouncing just \
                as much.",
            refs: &["General practice; switch bounce times of 1 to 10 ms are typical for tactile switches."],
        },
    }
}
