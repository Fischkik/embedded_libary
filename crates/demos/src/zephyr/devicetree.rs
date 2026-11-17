use crate::prelude::*;
use sim_core::zephyr::{LedPin, led_lit};

const BOARD_A: LedPin = LedPin { port: 0, pin: 13, active_low: true };
const BOARD_B: LedPin = LedPin { port: 1, pin: 5, active_low: false };

const APP_CODE: &str = "/* main.c - the same file for every board */\n\
#define LED0_NODE DT_ALIAS(led0)\n\
static const struct gpio_dt_spec led = GPIO_DT_SPEC_GET(LED0_NODE, gpios);\n\
\n\
gpio_pin_configure_dt(&led, GPIO_OUTPUT_ACTIVE);\n\
gpio_pin_set_dt(&led, 1);     /* 1 = \"LED on\", whatever that takes on this board */";

pub struct Devicetree {
    wired: LedPin,
    described: LedPin,
    want_on: bool,
}

impl Default for Devicetree {
    fn default() -> Self {
        Self { wired: BOARD_A, described: BOARD_A, want_on: true }
    }
}

fn level(high: bool) -> &'static str {
    if high { "HIGH" } else { "LOW" }
}

fn pin_controls(ui: &mut Ui, p: &mut LedPin, what: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label("port:");
        for port in [0, 1] {
            ui.selectable_value(&mut p.port, port, format!("gpio{port}")).on_hover_text(format!("Which group of pins (GPIO controller) {what}."));
        }
        widgets::wrap(ui, "pin", |ui| ui.add(egui::Slider::new(&mut p.pin, 0..=31).text("pin")).on_hover_text(format!("Which pin of that port {what}.")));
        ui.checkbox(&mut p.active_low, "LED lights when the pin is LOW").on_hover_text("Active-low: the LED sits between the supply voltage and the pin, so it lights when the pin is pulled low.");
    });
}

impl Demo for Devicetree {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "1. The board: how the LED is really wired", |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("example board A").on_hover_text("LED on gpio0 pin 13, lights when the pin is low.").clicked() {
                    self.wired = BOARD_A;
                }
                if ui.button("example board B").on_hover_text("LED on gpio1 pin 5, lights when the pin is high.").clicked() {
                    self.wired = BOARD_B;
                }
            });
            ui.push_id("wired", |ui| pin_controls(ui, &mut self.wired, "the LED is soldered to"));
        });
        widgets::group(ui, "2. The devicetree: what the build is told about the board", |ui| {
            if ui.button("copy from the board above").on_hover_text("Make the description match the hardware. For a supported board, Zephyr ships this file ready-made.").clicked() {
                self.described = self.wired;
            }
            ui.push_id("described", |ui| pin_controls(ui, &mut self.described, "the description names"));
            widgets::formula(ui, self.described.dts());
        });
        widgets::group(ui, "3. The application", |ui| {
            widgets::formula(ui, APP_CODE);
            ui.checkbox(&mut self.want_on, "the application switches the LED on: gpio_pin_set_dt(&led, 1)").on_hover_text("Untick for gpio_pin_set_dt(&led, 0): the application wants the LED off.");
        });

        let (d, w) = (self.described, self.wired);
        let pin_level = d.level_for(self.want_on);
        let lit = led_lit(d, w, self.want_on);
        let same_pin = (d.port, d.pin) == (w.port, w.pin);
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(64.0, 64.0), egui::Sense::hover());
            ui.painter().circle_filled(rect.center(), 26.0, if lit { pal.trace[2] } else { pal.canvas });
            ui.painter().circle_stroke(rect.center(), 26.0, (1.5, pal.dim));
            ui.vertical(|ui| {
                ui.label(RichText::new(if lit { "The LED is lit" } else { "The LED is dark" }).strong());
                widgets::status(
                    ui,
                    lit == self.want_on,
                    if lit == self.want_on {
                        "the LED does what the application asked for".to_owned()
                    } else if !same_pin {
                        format!("the firmware drives gpio{} pin {}, but the LED is on gpio{} pin {}: the description names the wrong pin", d.port, d.pin, w.port, w.pin)
                    } else {
                        "right pin, wrong level: the description has the LED's polarity the wrong way round".to_owned()
                    },
                );
            });
        });
        widgets::formula(
            ui,
            format!(
                "the build turned the devicetree into:   led = {{ .port = gpio{}, .pin = {}, .dt_flags = {} }}\n\
                 application asks for      : {}\n\
                 level put on gpio{} pin {:<2} : {} = \"{}\"{}\n\
                 the real LED              : on gpio{} pin {}, lights when {}",
                d.port,
                d.pin,
                if d.active_low { "GPIO_ACTIVE_LOW" } else { "GPIO_ACTIVE_HIGH" },
                if self.want_on { "on (1)" } else { "off (0)" },
                d.port,
                d.pin,
                level(pin_level),
                if self.want_on { "on" } else { "off" },
                if d.active_low { ", inverted because the pin is marked active-low" } else { "" },
                w.port,
                w.pin,
                level(!w.active_low),
            ),
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "other-board" => self.wired = BOARD_B,
            "other-board-fixed" => (self.wired, self.described) = (BOARD_B, BOARD_B),
            "wrong-flag" => self.described.active_low = false,
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "zephyr-devicetree",
        title: "Devicetree: describing the hardware",
        blurb: "The same application runs on two different boards. The difference is in a description, not in the code.",
        path: &["Zephyr"],
        make: || Box::new(Devicetree::default()),
        explain: Explanation {
            what: "Three things are on screen, from top to bottom. The board: where its LED is really connected. The \
                [[devicetree]]: a text file that describes the board to the build. And the application, which only ever says \
                \"the LED called led0, on or off\".\n\n\
                The lamp at the bottom shows what the real LED does, and the box beneath it follows the request step by step: \
                what the application asked for, which pin and level that became, and what the LED needs.",
            why: "Firmware written directly for one board is full of facts about that board: the LED is on pin 13, the sensor \
                is on that bus at that address, the pin must go low to switch the LED on. Move to another board and all of \
                that has to be found and changed.\n\n\
                [[Zephyr]] takes those facts out of the code and puts them into the devicetree. It is a tree of \"nodes\", \
                one per piece of hardware, each with properties. The LED node here has one property, gpios, with three \
                values: which port, which pin, and whether the LED is on when the pin is high or low. The aliases block gives \
                the node the board-independent name led0.\n\n\
                The devicetree is not read by the running firmware. When the application is built, Zephyr's build tools read \
                it and turn it into C constants. The line GPIO_DT_SPEC_GET(DT_ALIAS(led0), gpios) is replaced, at build time, \
                by a small structure with exactly those three values. A typing mistake in the devicetree is therefore a \
                build error, not a surprise on the running device.\n\n\
                One of those values earns special attention: the active-low flag. The application works with logical levels, \
                1 meaning \"on\". The GPIO driver inverts that for a pin marked active-low. So the application never needs to \
                know how the LED is wired.\n\n\
                For a board Zephyr supports, this file already exists. For your own board you write it once. To change a \
                detail for one application, you add an overlay: a small file that is laid over the board's devicetree.",
            formulas: &[("pin level = logical value, inverted if the pin is marked GPIO_ACTIVE_LOW", "The application writes 1. On an active-low pin the driver puts LOW on the wire, and the LED between supply and pin lights up.")],
            try_this: &[
                Experiment {
                    action: "Move to example board B without touching the devicetree.",
                    watch: "the LED going dark: the firmware faithfully drives gpio0 pin 13, but on this board the LED is somewhere else.",
                    preset: Some("other-board"),
                },
                Experiment {
                    action: "Now give board B its own devicetree (\"copy from the board above\").",
                    watch: "the LED lit again, with a different pin and the opposite level - and not one character of the application changed.",
                    preset: Some("other-board-fixed"),
                },
                Experiment {
                    action: "Back on board A, untick \"LED lights when the pin is LOW\" in the devicetree only.",
                    watch: "an LED that is on when the application says off and off when it says on. Toggle the application's checkbox to see it. This is the most common devicetree mistake.",
                    preset: Some("wrong-flag"),
                },
            ],
            leaves_out: "A real devicetree describes the whole board - processor, memory, every bus and every device on it - in \
                files that include each other, and each kind of node has a \"binding\" that says which properties it must \
                have. The build also checks that the GPIO controller named in the node exists and is enabled. Example board A \
                is wired like the first LED of a popular Nordic development kit; board B is made up. The application shown is \
                shortened: the real sample also checks that the GPIO driver is ready.",
            refs: &[
                "Zephyr sample samples/basic/blinky (main.c): DT_ALIAS(led0), GPIO_DT_SPEC_GET, gpio_pin_configure_dt with GPIO_OUTPUT_ACTIVE.",
                "Zephyr documentation: Devicetree (build/dts) and the GPIO driver API (struct gpio_dt_spec, gpio_pin_set_dt, GPIO_ACTIVE_LOW: logical levels are inverted for active-low pins).",
            ],
        },
    }
}
