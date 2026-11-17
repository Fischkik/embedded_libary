use crate::prelude::*;
use egui_plot::Polygon;
use sim_core::zephyr::{BeatScenario, ticks_for};

const DURATION_MS: f64 = 100.0;

pub struct Timers {
    b: BeatScenario,
}

impl Default for Timers {
    fn default() -> Self {
        Self { b: BeatScenario { ticks_per_s: 1000.0, period_ms: 10.0, work_ms: 2.5 } }
    }
}

impl Demo for Timers {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        let b = &mut self.b;
        widgets::group(ui, "A job that should run at a steady beat", |ui| {
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "period", |ui| ui.add(egui::Slider::new(&mut b.period_ms, 1.0..=50.0).step_by(0.5).text("wanted: once every").suffix(" ms").fixed_decimals(1)).on_hover_text("The beat you ask for, for example how often a sensor should be read."));
                widgets::wrap(ui, "work", |ui| ui.add(egui::Slider::new(&mut b.work_ms, 0.1..=10.0).logarithmic(true).text("the job takes").suffix(" ms").fixed_decimals(1)).on_hover_text("How long the work on each beat takes."));
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Kernel ticks per second:");
                for hz in [100.0, 1000.0, 10_000.0] {
                    ui.selectable_value(&mut b.ticks_per_s, hz, si(hz, "Hz")).on_hover_text("CONFIG_SYS_CLOCK_TICKS_PER_SEC: how finely the kernel counts time. Every timeout is a whole number of ticks.");
                }
            });
        });
        let b = self.b;
        let (timer, sleeper) = b.beats(DURATION_MS);
        let wanted: Vec<f64> = (0..).map(|k| k as f64 * b.period_ms).take_while(|t| *t < DURATION_MS).collect();

        ui.label(RichText::new("Three ways the beat can come out").strong());
        let names = ["thread: work, then k_sleep", "k_timer, periodic", "the beat you wanted"].map(str::to_owned).to_vec();
        plots::timeline("lanes", names, "time [ms]", DURATION_MS).show(ui, |p| {
            let mut bars = |starts: &[f64], width: f64, row: f64, fill: Color32| {
                for &t in starts {
                    p.polygon(Polygon::new("beat", plots::bar(t, (t + width).min(DURATION_MS), row - 0.3, row + 0.3)).fill_color(fill).stroke((0.0, fill)));
                }
            };
            bars(&wanted, 0.3, 2.0, pal.text);
            bars(&timer, b.work_ms, 1.0, pal.trace[2]);
            bars(&sleeper, b.work_ms, 0.0, pal.trace[3]);
        });
        widgets::key(ui, &[(pal.text, "wanted beat"), (pal.trace[2], "job started by a periodic kernel timer"), (pal.trace[3], "job in a loop that sleeps after each run")]);

        ui.label(RichText::new("How late each beat is").strong());
        let late = |starts: &[f64]| -> Vec<[f64; 2]> { starts.iter().enumerate().map(|(k, t)| [k as f64, t - k as f64 * b.period_ms]).collect() };
        plots::plot("late", 180.0).x_axis_label("beat number").y_axis_label("late by [ms]").include_y(0.0).show(ui, |p| {
            p.line(plots::line("k_timer, periodic", late(&timer), pal.trace[2]).width(2.0));
            p.line(plots::line("work, then k_sleep", late(&sleeper), pal.trace[3]).width(2.0));
        });

        let ticks = ticks_for(b.period_ms, b.ticks_per_s).max(1);
        let per_s = |period: f64| (1000.0 / period).floor();
        widgets::formula(
            ui,
            format!(
                "one tick               = 1 / {} = {}\n\
                 k_timer period         = {:.1} ms rounded up to whole ticks = {ticks} tick(s) = {:.1} ms\n\
                 work + k_sleep loop    = work rounded up to ticks + sleep = {:.1} ms + {:.1} ms = {:.1} ms per round\n\
                 beats in one second    : wanted {:.0}, k_timer {:.0}, work + k_sleep {:.0}\n\n\
                 K_TIMER_DEFINE(beat, NULL, NULL);\n\
                 k_timer_start(&beat, K_MSEC({:.0}), K_MSEC({:.0}));     /* first expiry, then the period */\n\
                 while (1) {{\n    k_timer_status_sync(&beat);     /* the thread sleeps until the next expiry */\n    do_the_work();\n}}",
                si(b.ticks_per_s, "Hz"),
                si(b.tick_ms() * 1e-3, "s"),
                b.period_ms,
                b.timer_period_ms(),
                b.sleep_loop_period_ms() - b.timer_period_ms(),
                b.timer_period_ms(),
                b.sleep_loop_period_ms(),
                per_s(b.period_ms),
                per_s(b.timer_period_ms()),
                per_s(b.sleep_loop_period_ms()),
                b.period_ms,
                b.period_ms,
            ),
        );
        let exact = (b.timer_period_ms() - b.period_ms).abs() < 1e-9;
        widgets::status(
            ui,
            exact,
            if exact { "the kernel timer keeps exactly the wanted beat; only the sleeping loop falls behind".to_owned() } else { format!("the tick is too coarse for this beat: {:.1} ms is not a whole number of {} ticks, so even the kernel timer runs slow", b.period_ms, si(b.tick_ms() * 1e-3, "s")) },
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "coarse" => (self.b.ticks_per_s, self.b.period_ms) = (100.0, 25.0),
            "tiny-work" => self.b.work_ms = 0.1,
            "fine" => (self.b.ticks_per_s, self.b.period_ms) = (10_000.0, 2.5),
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "zephyr-timers",
        title: "Kernel timers and the system tick",
        blurb: "Do something every 10 ms. The obvious loop is slow by a third - and the reason is worth knowing.",
        path: &["Zephyr"],
        make: || Box::new(Timers::default()),
        explain: Explanation {
            what: "A job should run once every 10 ms. The top row shows that wanted beat. The two rows below show how it comes \
                out in practice: started by a periodic kernel [[timer]], and written as a [[thread]] that does the work and \
                then sleeps for 10 ms. Each bar is one run of the job.\n\n\
                The graph underneath shows how late each beat is compared with the wanted one.",
            why: "The obvious way to do something regularly is a loop: do the work, sleep for the period, repeat. It has a \
                flaw that the bottom row makes visible. The sleep only starts when the work is finished, so one round takes \
                the work time plus the sleep time. With 2.5 ms of work and a 10 ms sleep the job runs every 13 ms, not every \
                10 ms. Each beat is a little later than the one before, and the lateness adds up without limit. If the work \
                takes longer on some rounds, the beat wobbles as well.\n\n\
                A kernel timer does not have this problem. You give it a period once, and the kernel schedules every expiry \
                from its own clock, not from the moment your code got round to it. The work may take long or short; the \
                next beat is on time regardless. A thread can wait for each expiry and then do the work.\n\n\
                Underneath both sits the system tick. [[Zephyr]]'s kernel does not measure time continuously; it counts \
                ticks, and every timeout - a sleep, a timer period, a wait with a time limit - is a whole number of ticks. A \
                time that falls between two ticks is rounded up, because a timeout promises a minimum delay. With 1000 ticks \
                per second, 10 ms is exactly 10 ticks. With 100 ticks per second a tick is 10 ms, and a wanted 25 ms \
                becomes 30 ms.\n\n\
                This rounding is also why the sleeping loop takes 13 ms and not 12.5 ms per round: the work ends in the \
                middle of a tick, and the sleep is counted from the next tick on.",
            formulas: &[
                ("timeout in ticks = time x ticks per second, rounded up", "25 ms at 100 ticks per second: 2.5, rounded up to 3 ticks = 30 ms."),
                ("period of a work-then-sleep loop = work + sleep (each in whole ticks)", "2.5 ms of work counts as 3 ticks; 3 ms + 10 ms = 13 ms per round, 76 beats per second instead of 100."),
            ],
            try_this: &[
                Experiment {
                    action: "Make the job tiny: 0.1 ms.",
                    watch: "the sleeping loop still falling behind, by one tick per round. Even work that takes almost no time costs a whole tick.",
                    preset: Some("tiny-work"),
                },
                Experiment {
                    action: "Set 100 ticks per second and ask for a beat every 25 ms.",
                    watch: "the kernel timer itself running slow: 25 ms is two and a half ticks, so it becomes 30 ms. The status line turns red.",
                    preset: Some("coarse"),
                },
                Experiment {
                    action: "Set 10 000 ticks per second and a beat every 2.5 ms.",
                    watch: "the kernel timer exact again: at this tick rate 2.5 ms is 25 whole ticks.",
                    preset: Some("fine"),
                },
            ],
            leaves_out: "A timer can also call a function directly at each expiry. That function runs inside the system clock's \
                interrupt handler, so it must be short and must not wait; for real work it submits a work item or wakes a \
                thread, as shown here. Other threads and interrupts delay the start of the job a little even with a timer, \
                but that delay does not add up. The kernel may add one more tick to a sleep, because the current tick is \
                already partly over. Zephyr's kernel is normally \"tickless\": it does not interrupt the processor on every \
                tick, only when the next timeout is due, which is what lets a device sleep; time is still counted in ticks. \
                Typical tick rates are 100 per second on older setups and around 10 000 on current hardware.",
            refs: &[
                "Zephyr documentation, Kernel Services > Timing > Timers: duration and period, \"The function is executed by the system clock interrupt handler\", \"The timer's duration is a minimum delay\", K_TIMER_DEFINE, k_timer_start, k_timer_status_sync.",
                "Zephyr documentation, Kernel Services > Timing > Kernel Timing: \"A tick is the internal count in which the kernel does all its internal uptime and timeout bookkeeping\", CONFIG_SYS_CLOCK_TICKS_PER_SEC, tick-level precision.",
            ],
        },
    }
}
