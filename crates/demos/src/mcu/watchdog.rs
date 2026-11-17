use crate::prelude::*;
use egui_plot::{MarkerShape, Points, Polygon};
use sim_core::mcu::{FwState, Watchdog, simulate_watchdog};

const DURATION_MS: f64 = 1000.0;

pub struct WatchdogDemo {
    w: Watchdog,
    hang: bool,
    hang_at_ms: f64,
}

impl Default for WatchdogDemo {
    fn default() -> Self {
        Self { w: Watchdog { timeout_ms: 100.0, loop_ms: 20.0, boot_ms: 50.0, hang_at_ms: None, kick_from_timer: false }, hang: true, hang_at_ms: 300.0 }
    }
}

impl Demo for WatchdogDemo {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        let w = &mut self.w;
        widgets::group(ui, "Watchdog and firmware", |ui| {
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "timeout", |ui| {
                    ui.add(egui::Slider::new(&mut w.timeout_ms, 10.0..=500.0).logarithmic(true).text("watchdog timeout").suffix(" ms").fixed_decimals(0))
                        .on_hover_text("If the watchdog is not restarted within this time, it resets the whole chip.")
                });
                widgets::wrap(ui, "loop", |ui| {
                    ui.add(egui::Slider::new(&mut w.loop_ms, 2.0..=300.0).logarithmic(true).text("main loop takes").suffix(" ms").fixed_decimals(0))
                        .on_hover_text("One pass of the firmware's main loop. At the end of each pass it restarts (\"kicks\") the watchdog.")
                });
                widgets::wrap(ui, "boot", |ui| {
                    ui.add(egui::Slider::new(&mut w.boot_ms, 5.0..=200.0).logarithmic(true).text("start-up takes").suffix(" ms").fixed_decimals(0))
                        .on_hover_text("How long the firmware needs after a reset before its main loop runs again.")
                });
            });
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut self.hang, "the firmware gets stuck").on_hover_text("A bug: the firmware ends up in a loop it never leaves, for example waiting for a sensor that does not answer.");
                widgets::wrap(ui, "hang", |ui| {
                    ui.add_enabled(self.hang, egui::Slider::new(&mut self.hang_at_ms, 0.0..=900.0).text("at").suffix(" ms").fixed_decimals(0)).on_hover_text("The moment the firmware gets stuck.")
                });
            });
            ui.checkbox(&mut w.kick_from_timer, "kick the watchdog from a timer interrupt instead of the main loop (a mistake)")
                .on_hover_text("Tempting, because it always works. But the timer interrupt keeps firing while the main loop is stuck, so the watchdog never notices.");
        });
        w.hang_at_ms = self.hang.then_some(self.hang_at_ms);
        let w = self.w;
        let tr = simulate_watchdog(&w, DURATION_MS);

        ui.label(RichText::new("Time left before the watchdog resets the chip").strong());
        plots::plot_bare("counter", 200.0).x_axis_label("time [ms]").y_axis_label("time left [ms]").include_x(0.0).include_x(DURATION_MS).include_y(0.0).include_y(w.timeout_ms).show(ui, |p| {
            p.line(plots::line("watchdog countdown", tr.counter.clone(), pal.trace[0]));
            p.points(Points::new("reset", tr.resets.iter().map(|&t| [t, 0.0]).collect::<Vec<_>>()).shape(MarkerShape::Cross).radius(7.0).color(pal.bad));
        });
        plots::timeline("state", vec!["firmware".to_owned()], "time [ms]", DURATION_MS).show(ui, |p| {
            for &(state, from, to) in &tr.states {
                let (name, fill) = match state {
                    FwState::Running => ("working", pal.good.gamma_multiply(0.8)),
                    FwState::Hung => ("stuck", pal.bad.gamma_multiply(0.8)),
                    FwState::Booting => ("starting up", pal.dim.gamma_multiply(0.7)),
                };
                p.polygon(Polygon::new(name, plots::bar(from, to, -0.3, 0.3)).fill_color(fill).stroke((0.0, fill)));
            }
        });
        widgets::key(ui, &[(pal.trace[0], "blue line: watchdog countdown"), (pal.good, "working"), (pal.bad, "stuck"), (pal.dim, "starting up after a reset"), (pal.bad, "cross: the watchdog resets the chip")]);

        let stuck_for_ever = matches!(tr.states.last(), Some((FwState::Hung, _, to)) if *to >= DURATION_MS) && w.kick_from_timer;
        let boot_loop = !w.kick_from_timer && w.loop_ms >= w.timeout_ms;
        widgets::status(
            ui,
            !stuck_for_ever && !boot_loop,
            if stuck_for_ever {
                "the firmware is stuck and nothing will ever reset it: the timer interrupt keeps kicking the watchdog".to_owned()
            } else if boot_loop {
                "the main loop is slower than the timeout: the watchdog resets the chip again and again before the first kick".to_owned()
            } else {
                format!("{} reset(s); the firmware was working {:.0} % of the time", tr.resets.len(), tr.working_share * 100.0)
            },
        );
        widgets::formula(
            ui,
            format!(
                "safety margin   = timeout / slowest main loop = {:.0} ms / {:.0} ms = {:.1} x  (must be above 1)\n\
                 longest outage  = timeout + start-up = {:.0} ms + {:.0} ms = {:.0} ms after the firmware gets stuck",
                w.timeout_ms,
                w.loop_ms,
                w.timeout_ms / w.loop_ms,
                w.timeout_ms,
                w.boot_ms,
                w.timeout_ms + w.boot_ms,
            ),
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "healthy" => self.hang = false,
            "too-short" => {
                self.hang = false;
                self.w.timeout_ms = 15.0;
            }
            "timer-kick" => self.w.kick_from_timer = true,
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "mcu-watchdog",
        title: "Watchdog: recovering from a hang",
        blurb: "A countdown that resets the chip unless the firmware keeps proving it is alive.",
        path: &["Inside the microcontroller"],
        make: || Box::new(WatchdogDemo::default()),
        explain: Explanation {
            what: "The top graph is a countdown inside the microcontroller: the [[watchdog]]. It runs down steadily, and every \
                time the firmware finishes a pass of its [[main loop]] it sets the countdown back to the top. That is the \
                sawtooth. The bar below shows what the firmware is doing: working, stuck, or starting up.\n\n\
                At 300 ms the firmware gets stuck. The sawtooth stops, the countdown reaches zero, and the watchdog resets \
                the chip (the cross). After the start-up time the firmware works again.",
            why: "Firmware runs for years without anyone watching. Sooner or later something unforeseen happens: a sensor \
                stops answering and the code waits for it for ever, or a rare bug sends the program into a loop it never \
                leaves. A PC user would restart the machine. An embedded device has to do that by itself.\n\n\
                The watchdog is a small timer with its own clock that the firmware cannot switch off once it is started. It \
                counts down, and if it reaches zero it resets the chip, exactly as if the power had been switched off and on. \
                Healthy firmware prevents this by restarting the countdown regularly - engineers say it kicks, or feeds, the \
                watchdog.\n\n\
                The idea only works if the kick proves that the firmware is really doing its job. That is why the kick \
                belongs at the end of the main loop, after all the work, and not in a timer [[interrupt]]: interrupts keep \
                firing even when the main loop is stuck.\n\n\
                Choosing the timeout is a trade-off. It must be clearly longer than the slowest pass of the main loop, or the \
                watchdog bites healthy firmware. But the longer it is, the longer the device stays dead after a real hang.",
            formulas: &[
                ("timeout > slowest pass of the main loop", "With a 20 ms loop and a 100 ms timeout there is a five-fold margin."),
                ("longest outage = timeout + start-up time", "Stuck right after a kick: 100 ms until the reset, plus 50 ms to start up = 150 ms."),
            ],
            try_this: &[
                Experiment { action: "Untick \"the firmware gets stuck\".", watch: "a clean sawtooth that never reaches zero: a healthy system does not notice its watchdog.", preset: Some("healthy") },
                Experiment {
                    action: "Without a hang, set the timeout to 15 ms - shorter than the 20 ms main loop.",
                    watch: "reset after reset: the countdown runs out before the first kick, every time. The device never gets any work done.",
                    preset: Some("too-short"),
                },
                Experiment {
                    action: "Let the firmware get stuck again, and tick \"kick the watchdog from a timer interrupt\".",
                    watch: "the sawtooth carrying on happily while the bar stays red to the end. The watchdog is being told all is well by code that knows nothing about the main loop.",
                    preset: Some("timer-kick"),
                },
                Experiment { action: "Drag the moment the firmware gets stuck back and forth.", watch: "the time from getting stuck to the reset changing between almost nothing and a full timeout, depending on when the last kick was.", preset: None },
            ],
            leaves_out: "Here the countdown starts when the main loop starts; on real chips it starts when the firmware enables \
                the watchdog, or already at power-up. A reset loses everything in memory, so real firmware often records why \
                it was reset and counts the resets. Many chips also have a window watchdog, which resets the chip when it is \
                kicked too early as well, to catch code that runs wild. In firmware with several tasks, each task has to \
                report in before the watchdog is kicked. The names differ per chip (IWDG, WDT, WWDG); the idea is the same.",
            refs: &["Generic independent watchdog timer as found on ARM Cortex-M microcontrollers: a free-running down-counter on its own clock that resets the chip at zero and can only be reloaded, not stopped."],
        },
    }
}
