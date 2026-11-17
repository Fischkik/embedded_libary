use crate::prelude::*;
use egui_plot::{Points, VLine};
use sim_core::zephyr::{PowerState, pick_power_state, pm_average_current};

/// Example states, lightest first. The numbers are illustrative, not from one chip.
const STATES: [PowerState; 3] = [
    PowerState { name: "suspend-to-idle", current_a: 800e-6, min_residency_us: 1_000.0, exit_latency_us: 10.0 },
    PowerState { name: "standby", current_a: 30e-6, min_residency_us: 10_000.0, exit_latency_us: 500.0 },
    PowerState { name: "suspend-to-ram", current_a: 2e-6, min_residency_us: 1_000_000.0, exit_latency_us: 5_000.0 },
];
const RUN_A: f64 = 4e-3;
/// Current while the processor merely waits for the next interrupt, without a sleep state.
const IDLE_A: f64 = 1.5e-3;
const IDLE_RANGE: (f64, f64) = (100e-6, 10.0);

pub struct Pm {
    idle_s: f64,
    work_s: f64,
    locked: [bool; 3],
    enabled: bool,
}

impl Default for Pm {
    fn default() -> Self {
        Self { idle_s: 2.0, work_s: 1e-3, locked: [false; 3], enabled: true }
    }
}

impl Pm {
    fn average(&self, idle_s: f64) -> f64 {
        let states: &[PowerState] = if self.enabled { &STATES } else { &[] };
        pm_average_current(states, RUN_A, IDLE_A, self.work_s * 1e6, idle_s * 1e6, &self.locked)
    }
}

impl Demo for Pm {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "What the application does", |ui| {
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "idle", |ui| {
                    ui.add(widgets::si_slider(&mut self.idle_s, IDLE_RANGE.0..=IDLE_RANGE.1, "nothing to do for", "s").logarithmic(true))
                        .on_hover_text("How long until the next thread has to run: the idle time the kernel sees when the last thread goes to sleep.")
                });
                widgets::wrap(ui, "work", |ui| {
                    ui.add(widgets::si_slider(&mut self.work_s, 100e-6..=0.1, "then works for", "s").logarithmic(true)).on_hover_text("How long the threads are busy before everything sleeps again.")
                });
            });
            ui.checkbox(&mut self.enabled, "power management switched on (CONFIG_PM=y)").on_hover_text("Without it the kernel only halts the processor until the next interrupt; the clocks keep running.");
        });

        let chosen = if self.enabled { pick_power_state(&STATES, self.idle_s * 1e6, &self.locked) } else { None };
        widgets::group(ui, "The sleep states this example chip offers, and the kernel's choice", |ui| {
            egui::Grid::new("states").striped(true).num_columns(6).show(ui, |ui| {
                for h in ["state", "current", "worth it from", "waking takes", "forbidden?", "verdict"] {
                    ui.label(RichText::new(h).strong());
                }
                ui.end_row();
                for (i, st) in STATES.iter().enumerate() {
                    let need = st.min_residency_us + st.exit_latency_us;
                    ui.add(egui::Label::new(RichText::new(st.name).monospace()).extend());
                    ui.monospace(si(st.current_a, "A"));
                    ui.monospace(si(st.min_residency_us * 1e-6, "s"));
                    ui.monospace(si(st.exit_latency_us * 1e-6, "s"));
                    ui.checkbox(&mut self.locked[i], "locked").on_hover_text("pm_policy_state_lock_get: some part of the firmware forbids this state for now, for example a driver in the middle of a transfer that the state would break.");
                    let (text, color) = if !self.enabled {
                        ("power management is off".to_owned(), pal.dim)
                    } else if chosen == Some(i) {
                        ("CHOSEN".to_owned(), pal.good)
                    } else if self.locked[i] {
                        ("locked".to_owned(), pal.warn)
                    } else if self.idle_s * 1e6 < need {
                        (format!("idle time too short: needs {}", si(need * 1e-6, "s")), pal.dim)
                    } else {
                        ("fits, but a deeper state fits too".to_owned(), pal.dim)
                    };
                    ui.add(egui::Label::new(RichText::new(text).color(color).strong()).extend());
                    ui.end_row();
                }
            });
        });

        let avg = self.average(self.idle_s);
        let rule = match chosen {
            Some(i) => {
                let st = STATES[i];
                format!(
                    "rule: a state fits if   idle time >= min-residency + exit-latency\n\
                     {:<16}: {} >= {} + {} = {}   -> fits, and it is the deepest that does\n\
                     average current = (run x work + sleep x (idle - waking) + run x waking) / (work + idle)\n\
                     \x20               = ({} x {} + {} x {} + {} x {}) / {} = {}",
                    st.name,
                    si(self.idle_s, "s"),
                    si(st.min_residency_us * 1e-6, "s"),
                    si(st.exit_latency_us * 1e-6, "s"),
                    si((st.min_residency_us + st.exit_latency_us) * 1e-6, "s"),
                    si(RUN_A, "A"),
                    si(self.work_s, "s"),
                    si(st.current_a, "A"),
                    si(self.idle_s - st.exit_latency_us * 1e-6, "s"),
                    si(RUN_A, "A"),
                    si(st.exit_latency_us * 1e-6, "s"),
                    si(self.work_s + self.idle_s, "s"),
                    si(avg, "A"),
                )
            }
            None => format!(
                "rule: a state fits if   idle time >= min-residency + exit-latency\n\
                 no sleep state is used: the processor only halts until the next interrupt, at {}\n\
                 average current = ({} x {} + {} x {}) / {} = {}",
                si(IDLE_A, "A"),
                si(RUN_A, "A"),
                si(self.work_s, "s"),
                si(IDLE_A, "A"),
                si(self.idle_s, "s"),
                si(self.work_s + self.idle_s, "s"),
                si(avg, "A"),
            ),
        };
        widgets::formula(ui, rule);

        ui.label(RichText::new("Average current against the length of the idle time").strong());
        let (a, b) = (IDLE_RANGE.0.log10(), IDLE_RANGE.1.log10());
        let xs: Vec<f64> = (0..=300).map(|i| a + (b - a) * i as f64 / 300.0).collect();
        let with_pm: Vec<[f64; 2]> = xs.iter().map(|&x| [x, self.average(10f64.powf(x)).log10()]).collect();
        let without: Vec<[f64; 2]> = xs.iter().map(|&x| [x, pm_average_current(&[], RUN_A, IDLE_A, self.work_s * 1e6, 10f64.powf(x) * 1e6, &[]).log10()]).collect();
        let power_of_ten = |unit: &'static str| {
            move |mark: egui_plot::GridMark, _: &std::ops::RangeInclusive<f64>| {
                let i = mark.value.round();
                if (mark.value - i).abs() < 0.01 { si(10f64.powf(i), unit) } else { String::new() }
            }
        };
        plots::plot_bare("avg", 230.0)
            .x_axis_label("idle time until the next event")
            .y_axis_label("average current")
            .x_grid_spacer(egui_plot::uniform_grid_spacer(|_| [1.0, 1.0, 1.0]))
            .y_grid_spacer(egui_plot::uniform_grid_spacer(|_| [1.0, 1.0, 1.0]))
            .x_axis_formatter(power_of_ten("s"))
            .y_axis_formatter(power_of_ten("A"))
            .show(ui, |p| {
                p.line(plots::line("without power management", without, pal.dim).style(egui_plot::LineStyle::dashed_dense()));
                p.line(plots::line("with the kernel choosing a state", with_pm, pal.trace[0]).width(2.5));
                for st in &STATES {
                    p.vline(VLine::new(format!("{} possible from here", st.name), ((st.min_residency_us + st.exit_latency_us) * 1e-6).log10()).color(pal.grid));
                }
                p.points(Points::new("now", vec![[self.idle_s.log10(), avg.log10()]]).radius(6.0).color(pal.text));
            });
        widgets::key(ui, &[(pal.trace[0], "with the kernel choosing a state"), (pal.dim, "dashed: without power management"), (pal.grid, "vertical lines: where each state becomes possible"), (pal.text, "dot: the current setting")]);
        if let Some(i) = chosen {
            let st = STATES[i];
            widgets::formula(
                ui,
                format!(
                    "/* how this state is described in the devicetree */\n{}: {} {{\n    compatible = \"zephyr,power-state\";\n    power-state-name = \"{}\";\n    min-residency-us = <{:.0}>;\n    exit-latency-us = <{:.0}>;\n}};",
                    st.name.replace('-', "_"),
                    st.name,
                    st.name,
                    st.min_residency_us,
                    st.exit_latency_us
                ),
            );
        }
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "short" => self.idle_s = 5e-3,
            "locked" => self.locked[2] = true,
            "off" => self.enabled = false,
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "zephyr-pm",
        title: "Power management: the kernel picks the sleep",
        blurb: "When every thread is asleep, Zephyr decides how deeply the chip may sleep - from how long it has until the next job.",
        path: &["Zephyr"],
        make: || Box::new(Pm::default()),
        explain: Explanation {
            what: "The table lists three [[sleep mode]]s of an example chip, from light to deep: each draws less current than \
                the one before, but takes longer to wake from and is only worth entering for a longer sleep. The last column \
                is the kernel's verdict for the idle time you set at the top.\n\n\
                The graph shows the average current for every possible idle time. The dot is the current setting, and the \
                vertical lines mark where each state becomes possible.",
            why: "The demo on sleep modes showed that a device lives long by sleeping deeply between short bursts of work. On \
                a bare microcontroller the firmware has to decide when and how deeply to sleep. With an [[RTOS]] that is \
                awkward: no single [[thread]] knows what the others are about to do.\n\n\
                The kernel does know. It keeps the list of every timeout in the system, so when the last thread goes to \
                sleep it knows exactly how long nothing will happen. [[Zephyr]]'s power management uses that moment. A \
                small piece of code called the policy looks at the idle time ahead and picks a state.\n\n\
                The rule of the standard policy is simple. Each state comes with two numbers, written down once in the \
                [[devicetree]]. The exit latency is how long waking up takes. The minimum residency is how long the chip \
                must stay in the state for it to pay off, because entering and leaving cost energy too: the deepest state \
                here needs a full second, since its 5 ms of waking up cost as much as standby does in two thirds of a \
                second. A state fits if the idle time is at least the sum of the two. Of all the states that fit, the policy takes the deepest.\n\n\
                So the application does not manage power directly. It simply sleeps and waits as much as it can, and the \
                longer its threads leave the processor alone, the deeper the chip sleeps - automatically. This is also why \
                a thread that wakes up every millisecond \"just to check\" is so costly: it never leaves the kernel an idle \
                time long enough for a deep state.\n\n\
                Sometimes a deep state would break something, for example a serial transfer in progress that needs its clock. \
                A driver can then forbid a state for a while, and the policy falls back to the next one.",
            formulas: &[
                ("a state fits if: idle time >= min-residency + exit-latency", "standby here: 10 ms + 0.5 ms = 10.5 ms. With 2 s of idle time all three fit, and the deepest, suspend-to-ram, is chosen."),
                ("average current = (run x work + sleep x time asleep + run x waking) / (work + idle)", "1 ms of work at 4 mA, then 2 s in suspend-to-ram at 2 µA, 5 ms of it waking up at 4 mA: 14 µA."),
            ],
            try_this: &[
                Experiment {
                    action: "Shorten the idle time to 5 ms.",
                    watch: "the choice dropping to the lightest state and the average current jumping from microamperes to about a milliampere. Then drag the slider and watch the dot step down the curve at each vertical line.",
                    preset: Some("short"),
                },
                Experiment {
                    action: "With 2 s of idle time, lock the deepest state.",
                    watch: "the policy falling back to standby and the average current rising from 14 µA to about 33 µA, although nothing about the application changed.",
                    preset: Some("locked"),
                },
                Experiment {
                    action: "Switch power management off.",
                    watch: "the dashed line: the processor merely halts between interrupts at 1.5 mA, about a hundred times more than with a sleep state.",
                    preset: Some("off"),
                },
            ],
            leaves_out: "The three states and all their numbers are examples; a real chip's states are in its devicetree files, \
                and not every chip offers all of them. The state names are Zephyr's own. Waking up is counted at the full run \
                current and drawn as if the chip woke early enough to be on time. Devices - a sensor, a radio, a display - \
                have their own power management on top of this, and an application can replace the policy with its own. An \
                interrupt can end any sleep early, which the policy cannot know in advance.",
            refs: &[
                "Zephyr documentation, Power Management > System Power Management: the policy manager, residency policy \"if (time_to_next_scheduled_event >= (state.min_residency_us + state.exit_latency))\", pm_policy_state_lock_get, CONFIG_PM.",
                "Zephyr devicetree binding zephyr,power-state: power-state-name, min-residency-us, exit-latency-us.",
            ],
        },
    }
}
