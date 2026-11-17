use crate::prelude::*;
use sim_core::zephyr::{BootProblem, InitItem, InitLevel, boot_order};

pub struct Boot {
    items: Vec<InitItem>,
}

impl Default for Boot {
    fn default() -> Self {
        let item = |name: &str, level, priority, depends_on, needs_kernel| InitItem { name: name.to_owned(), level, priority, depends_on, needs_kernel };
        Self {
            items: vec![
                item("Clock control", InitLevel::PreKernel1, 30, None, false),
                item("GPIO driver", InitLevel::PreKernel1, 40, Some(0), false),
                item("Serial port driver", InitLevel::PreKernel1, 55, Some(0), false),
                item("I2C bus driver", InitLevel::PostKernel, 50, Some(0), false),
                item("Temperature sensor driver", InitLevel::PostKernel, 90, Some(3), true),
                item("Your own init function", InitLevel::Application, 90, Some(4), true),
            ],
        }
    }
}

fn describe(level: InitLevel) -> &'static str {
    match level {
        InitLevel::PreKernel1 => "hardware that needs nothing else. The kernel is not running yet.",
        InitLevel::PreKernel2 => "hardware that builds on the first level. The kernel is still not running.",
        InitLevel::PostKernel => "drivers that sleep, wait or take locks while starting up.",
        InitLevel::Application => "the application's own start-up functions.",
    }
}

impl Demo for Boot {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "What has to be started, and when you ask for it", |ui| {
            for (i, item) in self.items.iter_mut().enumerate() {
                ui.push_id(i, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.allocate_ui(vec2(190.0, ui.spacing().interact_size.y), |ui| {
                            ui.label(RichText::new(item.name.as_str()).strong());
                        });
                        egui::ComboBox::from_label("level")
                            .selected_text(item.level.name())
                            .show_ui(ui, |ui| {
                                for level in InitLevel::ALL {
                                    ui.selectable_value(&mut item.level, level, level.name()).on_hover_text(describe(level));
                                }
                            })
                            .response
                            .on_hover_text("The stage of the start-up in which this is initialized. Stages run in a fixed order.");
                        widgets::wrap(ui, "priority", |ui| ui.add(egui::Slider::new(&mut item.priority, 0..=99).text("priority")).on_hover_text("The order inside a level: lower numbers are initialized first."));
                    });
                });
            }
        });

        let order = boot_order(&self.items);
        let failed = order.iter().filter(|(_, p)| p.is_some()).count();
        ui.label(RichText::new("The start-up, from top to bottom").strong());
        ui.label("Reset: the processor starts, the clock and the memory for variables are set up.");
        let mut n = 0;
        for level in InitLevel::ALL {
            if level == InitLevel::PostKernel {
                ui.label(RichText::new("The kernel starts. From here on threads exist, and code may sleep and wait.").color(pal.trace[0]).strong());
            }
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(level.name()).monospace().strong());
                    ui.label(RichText::new(describe(level)).color(pal.dim));
                });
                for &(i, problem) in order.iter().filter(|(i, _)| self.items[*i].level == level) {
                    n += 1;
                    let item = &self.items[i];
                    ui.horizontal_wrapped(|ui| {
                        ui.monospace(format!("{n}."));
                        ui.label(format!("{} (priority {})", item.name, item.priority));
                        match problem {
                            None => ui.label(RichText::new("ok").color(pal.good).strong()),
                            Some(BootProblem::DependencyNotReady(d)) => ui.label(RichText::new(format!("FAILS: it needs \"{}\", which is not up yet", self.items[d].name)).color(pal.bad).strong()),
                            Some(BootProblem::NoKernelYet) => ui.label(RichText::new("FAILS: its start-up has to wait for the hardware, and waiting needs the kernel").color(pal.bad).strong()),
                        };
                    });
                }
            });
        }
        ui.label(RichText::new("main() runs.").strong());

        widgets::status(
            ui,
            failed == 0,
            if failed == 0 { "everything was started after the things it depends on".to_owned() } else { format!("{failed} item(s) failed to start: code that uses them finds the device \"not ready\"") },
        );
        let keys: Vec<String> = order.iter().map(|(i, _)| format!("{}/{}", self.items[*i].level.name(), self.items[*i].priority)).collect();
        widgets::formula(
            ui,
            format!(
                "order = level first, then priority number, lower first:\n  {}\n\n\
                 SYS_INIT(my_init, APPLICATION, {});      /* run my_init() at this level and priority */\n\
                 if (!device_is_ready(sensor)) {{ ... }}   /* always check before using a device */",
                keys.join("  <  "),
                self.items[5].priority,
            ),
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "sensor-first" => self.items[4].priority = 40,
            "too-early" => (self.items[3].level, self.items[4].level) = (InitLevel::PreKernel2, InitLevel::PreKernel2),
            "clock-late" => self.items[0].priority = 60,
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "zephyr-boot",
        title: "From reset to main(): how Zephyr boots",
        blurb: "Before your main() runs, Zephyr has started a dozen drivers - in an order you can get wrong.",
        path: &["Zephyr"],
        make: || Box::new(Boot::default()),
        explain: Explanation {
            what: "At the top are six things that have to be started before the application can run: five [[driver]]s and one \
                function of your own. For each you choose a level and a priority. Below, the start-up is played through from \
                top to bottom, and every item reports whether it could start.\n\n\
                The opening state is a working order. Two of the items depend on another one: the temperature sensor sits on \
                the I2C bus, and your own function wants to read the sensor.",
            why: "On a bare microcontroller, main() is almost the first thing that runs. In [[Zephyr]] a lot happens first. \
                Every driver that the configuration has switched on must initialize its hardware, and the kernel itself \
                must be started. By the time main() runs, the serial port prints, the buses work and the sensors answer.\n\n\
                The drivers cannot start in any order. A sensor on a bus can only be set up when the bus driver works; the \
                bus driver needs its clock switched on first. Zephyr handles this with two numbers per driver.\n\n\
                The level is the coarse stage. The first two levels run before the kernel exists. Code there is alone on the \
                processor: no [[thread]]s, no sleeping, no waiting for a [[lock]]. That is fine for switching on a clock or \
                setting up pins. After the kernel has started comes a level for drivers whose start-up has to wait - for a \
                chip to answer over a slow bus, say. Last comes a level for the application's own start-up functions.\n\n\
                Within a level, the priority number decides: lower numbers first. Zephyr's own drivers come with sensible \
                defaults - bus drivers before the devices on those buses.\n\n\
                When the order is wrong, nothing dramatic happens at first. The driver's start-up simply fails, and the \
                device is marked as not ready. The trouble shows later, in the application, as a sensor that never delivers \
                a value. That is why Zephyr code checks device_is_ready before using a device.",
            formulas: &[("order = level, then priority number (lower first)", "POST_KERNEL/50 (the bus) runs before POST_KERNEL/90 (the sensor on it); anything in PRE_KERNEL_1 runs before both.")],
            try_this: &[
                Experiment {
                    action: "Give the temperature sensor priority 40, ahead of its bus (50).",
                    watch: "the sensor failing because the I2C bus driver is not up yet - and your own function failing with it, because it needs the sensor.",
                    preset: Some("sensor-first"),
                },
                Experiment {
                    action: "Move the I2C bus driver and the sensor to PRE_KERNEL_2.",
                    watch: "the bus driver starting fine, but the sensor failing for a different reason: its start-up has to wait for the chip, and before the kernel runs nothing can wait.",
                    preset: Some("too-early"),
                },
                Experiment {
                    action: "Back at the start, give the clock control priority 60.",
                    watch: "the GPIO and serial drivers failing, because they are now started before the clock they need. Find the smallest change that repairs it.",
                    preset: Some("clock-late"),
                },
            ],
            leaves_out: "Real systems have dozens of such items, and Zephyr works out much of the order from the devicetree, \
                which records which device sits on which bus; the build warns about orders that contradict it. There is an \
                even earlier level for code that must run before anything else, and the priorities shown are examples, not \
                the real defaults. What exactly happens between reset and the first level (setting up memory, the processor's \
                own start-up code) is specific to the chip. A driver can also be told not to start at boot at all and be \
                started by the application later.",
            refs: &[
                "Zephyr documentation, Device Driver Model > Initialization Levels: PRE_KERNEL_1, PRE_KERNEL_2, POST_KERNEL; priority \"lower values initialize earlier\"; SYS_INIT; deferred initialization.",
                "Zephyr documentation, Application Development: device_is_ready() before using a device.",
            ],
        },
    }
}
