use crate::prelude::*;
use egui_plot::{HLine, Polygon};
use sim_core::zephyr::{SemScenario, simulate_semaphore};

const DURATION_MS: f64 = 100.0;

pub struct Semaphore {
    s: SemScenario,
}

impl Default for Semaphore {
    fn default() -> Self {
        Self { s: SemScenario { limit: 1, burst: 3, spacing_ms: 1.0, burst_period_ms: 40.0, work_ms: 5.0 } }
    }
}

impl Demo for Semaphore {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        let s = &mut self.s;
        widgets::group(ui, "Events, the semaphore and the thread", |ui| {
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "burst", |ui| ui.add(egui::Slider::new(&mut s.burst, 1..=8).text("events per burst")).on_hover_text("Events come in groups, like bytes of a message or a bouncing button. Each one raises an interrupt."));
                widgets::wrap(ui, "spacing", |ui| {
                    ui.add(egui::Slider::new(&mut s.spacing_ms, 0.5..=10.0).logarithmic(true).text("apart").suffix(" ms").fixed_decimals(1)).on_hover_text("Time between two events inside a burst.")
                });
                widgets::wrap(ui, "period", |ui| {
                    ui.add(egui::Slider::new(&mut s.burst_period_ms, 10.0..=100.0).text("a burst every").suffix(" ms").fixed_decimals(0)).on_hover_text("How often a new burst starts.")
                });
            });
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "limit", |ui| {
                    ui.add(egui::Slider::new(&mut s.limit, 1..=8).text("semaphore limit")).on_hover_text("The highest value the semaphore's count can reach. 1 makes it a binary semaphore: it only remembers that something happened, not how often.")
                });
                widgets::wrap(ui, "work", |ui| {
                    ui.add(egui::Slider::new(&mut s.work_ms, 0.5..=30.0).logarithmic(true).text("thread needs per event").suffix(" ms").fixed_decimals(1)).on_hover_text("How long the thread works on one event before it comes back for the next.")
                });
            });
        });
        let s = self.s;
        let tr = simulate_semaphore(&s, DURATION_MS);

        ui.label(RichText::new("The interrupt gives, the thread takes").strong());
        plots::timeline("lanes", vec!["thread".to_owned(), "interrupt".to_owned()], "time [ms]", DURATION_MS).show(ui, |p| {
            for &(from, to) in &tr.working {
                let fill = pal.trace[2].gamma_multiply(0.8);
                p.polygon(Polygon::new("working on an event", plots::bar(from, to.min(DURATION_MS), -0.3, 0.3)).fill_color(fill).stroke((1.0, pal.canvas)));
            }
            for &(t, lost) in &tr.events {
                let fill = if lost { pal.bad } else { pal.trace[1] };
                p.polygon(Polygon::new(if lost { "lost" } else { "give" }, plots::bar(t - 0.15, t + 0.15, 0.7, 1.3)).fill_color(fill).stroke((0.0, fill)));
            }
        });
        widgets::key(ui, &[(pal.trace[1], "event: the interrupt handler gives the semaphore"), (pal.bad, "event lost: the count was already at its limit"), (pal.trace[2], "the thread works on one event")]);
        ui.label(RichText::new("The semaphore's count: events given but not yet taken").strong());
        plots::plot_bare("count", 150.0).x_axis_label("time [ms]").y_axis_label("count").include_x(0.0).include_x(DURATION_MS).include_y(0.0).include_y(s.limit as f64 + 0.5).show(ui, |p| {
            p.line(plots::line("count", tr.count.clone(), pal.trace[0]).width(2.0));
            p.hline(HLine::new("limit", s.limit as f64).color(pal.bad).style(egui_plot::LineStyle::dashed_dense()));
        });
        widgets::key(ui, &[(pal.trace[0], "count"), (pal.bad, "dashed: the limit")]);

        widgets::status(
            ui,
            tr.lost == 0,
            if tr.lost == 0 {
                format!("all {} events were handled, the slowest {} ms after it happened", tr.handled, (tr.worst_delay_ms * 10.0).round() / 10.0)
            } else {
                format!("{} of {} events were lost: the semaphore was given while its count was already at the limit", tr.lost, tr.events.len())
            },
        );
        let in_rate = s.burst as f64 / s.burst_period_ms * 1000.0;
        let out_rate = 1000.0 / s.work_ms;
        widgets::formula(
            ui,
            format!(
                "arriving on average = events per burst / time between bursts = {} / {:.0} ms = {:.0} per second\n\
                 the thread manages  = 1 / time per event = 1 / {:.1} ms = {:.0} per second{}\n\
                 inside a burst events are {:.1} ms apart: {}",
                s.burst,
                s.burst_period_ms,
                in_rate,
                s.work_ms,
                out_rate,
                if in_rate > out_rate { "   <- too slow on average: no limit is high enough" } else { "" },
                s.spacing_ms,
                if s.spacing_ms < s.work_ms { "faster than the thread works, so the count has to hold the backlog" } else { "the thread keeps up, the count never rises" },
            ),
        );
        egui::CollapsingHeader::new(RichText::new("The same thing as Zephyr code").strong()).default_open(true).show(ui, |ui| {
            widgets::formula(
                ui,
                format!(
                    "K_SEM_DEFINE(my_sem, 0, {});            /* name, starting count, limit */\n\n\
                     void my_isr(const void *arg)\n{{\n    k_sem_give(&my_sem);               /* never waits: allowed in an interrupt handler */\n}}\n\n\
                     void my_thread(void *p1, void *p2, void *p3)\n{{\n    while (1) {{\n        k_sem_take(&my_sem, K_FOREVER);  /* sleeps here until there is something to do */\n        handle_one_event();              /* {:.1} ms */\n    }}\n}}",
                    s.limit, s.work_ms
                ),
            );
        });
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "counting" => self.s.limit = 3,
            "overload" => (self.s.limit, self.s.work_ms) = (8, 20.0),
            "slow-events" => self.s.spacing_ms = 8.0,
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "zephyr-semaphore",
        title: "Semaphores: an interrupt wakes a thread",
        blurb: "How an interrupt handler hands work to a thread - and how events get lost when the semaphore cannot count.",
        path: &["Zephyr"],
        make: || Box::new(Semaphore::default()),
        explain: Explanation {
            what: "The top row shows events: each small bar is an [[interrupt]], for example a byte arriving. The row below \
                shows a [[thread]] that deals with the events one at a time. Between them sits a [[semaphore]]. Its count is \
                the lower graph: how many events have been reported but not yet picked up.\n\n\
                In the opening state events come three at a time, and the third of every burst is red: lost.",
            why: "An interrupt [[handler]] must be short, so it should not do the real work itself. It needs a way to tell a \
                thread \"there is something for you\" and return at once. The thread, in turn, should not keep checking - it \
                should sleep and use no processor time until there is something to do.\n\n\
                A semaphore does exactly this. It is a counter with two operations. Give adds one to the count. Take \
                subtracts one - and if the count is zero, the taking thread goes to sleep until someone gives. Give never \
                waits, so an interrupt handler is allowed to call it.\n\n\
                The count has a limit, chosen when the semaphore is created. With a limit of 1 it is a binary semaphore: it \
                can only say \"something happened\". If a second and third event arrive while the thread is still busy with \
                the first, the count is already 1, and a give at the limit simply has no effect. The events are not \
                queued; they are gone. With a higher limit the semaphore counts, and the thread works off the backlog after \
                the burst.\n\n\
                A semaphore carries no data, only a number. And it has no owner: anyone may give. That distinguishes it from \
                a mutex, [[Zephyr]]'s [[lock]] for protecting shared data. A mutex belongs to the thread that locked it, only \
                that thread can unlock it, the kernel raises the owner's priority while a more urgent thread waits (see the \
                Priority inversion demo), and it must not be used from interrupt handlers at all.",
            formulas: &[
                ("give: count = count + 1, unless count = limit        take: count = count - 1, or wait while count = 0", "With limit 1, three quick gives leave the count at 1."),
                ("on average: events per second in must stay below 1 / time per event", "3 events per 40 ms = 75 per second in; at 5 ms each the thread manages 200 per second. Fine on average - the bursts are the problem."),
            ],
            try_this: &[
                Experiment {
                    action: "Raise the limit to 3.",
                    watch: "the count climbing to 2 during each burst and stepping down again as the thread catches up. No more red bars.",
                    preset: Some("counting"),
                },
                Experiment {
                    action: "Keep the limit at 1 and spread the events of a burst 8 ms apart.",
                    watch: "no event lost and a count that never leaves zero: every give finds the thread already waiting. A binary semaphore is enough when the thread is always faster than the events.",
                    preset: Some("slow-events"),
                },
                Experiment {
                    action: "Set the limit to 8 and make the thread slow: 20 ms per event.",
                    watch: "the count ratcheting up from burst to burst until it hits the limit, then events being lost after all. A bigger counter only buys time when the thread is too slow on average.",
                    preset: Some("overload"),
                },
            ],
            leaves_out: "One thread and one source of events; when several threads wait, the semaphore goes to the most urgent \
                one that has waited longest. Take can also give up after a time limit instead of waiting for ever. The \
                interrupt handler itself is drawn as taking no time. To pass the data that belongs to an event - which byte \
                arrived? - a semaphore is not enough: that is what message queues are for (planned as its own demo).",
            refs: &[
                "Zephyr documentation, Kernel Services > Synchronization > Semaphores: count and limit, \"Giving the semaphore increments its count, unless the count is already equal to the limit\", \"A semaphore may be given by a thread or an ISR\", K_SEM_DEFINE(my_sem, 0, 1).",
                "Zephyr documentation, Kernel Services > Synchronization > Mutexes: owning thread, priority inheritance, \"Mutex objects are not designed for use by ISRs\".",
            ],
        },
    }
}
