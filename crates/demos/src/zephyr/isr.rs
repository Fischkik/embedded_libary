use crate::prelude::*;
use egui_plot::{Polygon, VLine};
use sim_core::zephyr::{IsrScenario, interrupt_to_thread};

const DURATION_MS: f64 = 40.0;

pub struct Isr {
    s: IsrScenario,
    lock: bool,
}

impl Default for Isr {
    fn default() -> Self {
        Self { s: IsrScenario { irq_at_ms: 12.0, isr_ms: 0.5, urgent_work_ms: 3.0, background_cooperative: false, background_yields_every_ms: 10.0, lock_at_ms: 10.0, lock_ms: 0.0 }, lock: false }
    }
}

impl Demo for Isr {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        let s = &mut self.s;
        widgets::group(ui, "An interrupt arrives while a background thread is busy", |ui| {
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "irq", |ui| ui.add(egui::Slider::new(&mut s.irq_at_ms, 1.0..=25.0).text("the interrupt arrives at").suffix(" ms").fixed_decimals(1)).on_hover_text("The moment the hardware signals the event, for example a pin changing."));
                widgets::wrap(ui, "isr", |ui| ui.add(egui::Slider::new(&mut s.isr_ms, 0.1..=3.0).logarithmic(true).text("its handler runs for").suffix(" ms").fixed_decimals(1)).on_hover_text("How long the interrupt handler (ISR) takes. Real handlers take microseconds; it is drawn long here so that you can see it."));
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("The background thread is:");
                ui.selectable_value(&mut s.background_cooperative, false, "preemptible").on_hover_text("Priority zero or above: a more urgent thread can take the processor from it at any time.");
                ui.selectable_value(&mut s.background_cooperative, true, "cooperative").on_hover_text("Negative priority: no other thread can take the processor from it; it has to give it up itself.");
                widgets::wrap(ui, "yield", |ui| {
                    ui.add_enabled(s.background_cooperative, egui::Slider::new(&mut s.background_yields_every_ms, 2.0..=30.0).text("and yields every").suffix(" ms").fixed_decimals(0)).on_hover_text("How often the cooperative thread calls k_yield to let others run.")
                });
            });
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut self.lock, "the background thread locks interrupts").on_hover_text("irq_lock: for a short stretch the thread switches all interrupt handling off, to protect data it shares with a handler.");
                widgets::wrap(ui, "lock-at", |ui| ui.add_enabled(self.lock, egui::Slider::new(&mut s.lock_at_ms, 0.0..=25.0).text("from").suffix(" ms").fixed_decimals(1)).on_hover_text("Where the locked stretch begins."));
                widgets::wrap(ui, "lock-len", |ui| ui.add_enabled(self.lock, egui::Slider::new(&mut s.lock_ms, 0.5..=10.0).text("for").suffix(" ms").fixed_decimals(1)).on_hover_text("How long interrupts stay locked. Real code keeps this to microseconds."));
            });
        });
        if !self.lock {
            s.lock_ms = 0.0;
        } else if s.lock_ms == 0.0 {
            s.lock_ms = 6.0;
        }
        let s = self.s;
        let tr = interrupt_to_thread(&s);

        ui.label(RichText::new("From the interrupt to the thread that deals with it").strong());
        let names = ["background thread", "urgent thread", "interrupt handler"].map(str::to_owned).to_vec();
        plots::timeline("lanes", names, "time [ms]", DURATION_MS).show(ui, |p| {
            let mut bar = |name: &str, from: f64, to: f64, row: f64, half: f64, fill: Color32| {
                if to > from {
                    p.polygon(Polygon::new(name, plots::bar(from, to.min(DURATION_MS), row - half, row + half)).fill_color(fill).stroke((0.0, fill)));
                }
            };
            let bg = pal.dim.gamma_multiply(0.7);
            bar("background", 0.0, tr.isr.0, 0.0, 0.3, bg);
            bar("background", tr.isr.1, tr.urgent.0, 0.0, 0.3, bg);
            bar("background", tr.urgent.1, DURATION_MS, 0.0, 0.3, bg);
            bar("waiting", tr.isr.1, tr.urgent.0, 1.0, 0.1, pal.trace[2].gamma_multiply(0.35));
            bar("urgent", tr.urgent.0, tr.urgent.1, 1.0, 0.3, pal.trace[2]);
            bar("waiting", s.irq_at_ms, tr.isr.0, 2.0, 0.1, pal.bad);
            bar("isr", tr.isr.0, tr.isr.1, 2.0, 0.3, pal.trace[3]);
            bar("irq lock", s.lock_at_ms, s.lock_at_ms + s.lock_ms, 0.42, 0.05, pal.warn);
            p.vline(VLine::new("interrupt", s.irq_at_ms).color(pal.text));
        });
        widgets::key(
            ui,
            &[
                (pal.text, "vertical line: the interrupt arrives"),
                (pal.trace[3], "interrupt handler runs"),
                (pal.bad, "handler held back: interrupts are locked"),
                (pal.trace[2], "urgent thread runs (pale: ready, waiting)"),
                (pal.warn, "line above the background thread: interrupts locked"),
            ],
        );

        let wait_for_yield = tr.urgent.0 - tr.isr.1;
        widgets::formula(
            ui,
            format!(
                "interrupt -> handler starts     = time left under the interrupt lock = {:.1} ms\n\
                 interrupt -> urgent thread runs = wait for the handler + handler + wait for the background thread\n\
                 \x20                               = {:.1} ms + {:.1} ms + {:.1} ms = {:.1} ms",
                tr.isr_latency_ms, tr.isr_latency_ms, s.isr_ms, wait_for_yield, tr.thread_latency_ms
            ),
        );
        widgets::status(
            ui,
            tr.isr_latency_ms == 0.0 && wait_for_yield == 0.0,
            if tr.isr_latency_ms > 0.0 {
                "the handler itself is late: the interrupt arrived while interrupts were locked".to_owned()
            } else if wait_for_yield > 0.0 {
                "the handler ran at once, but the urgent thread has to wait until the cooperative thread yields".to_owned()
            } else {
                "nothing in the way: the handler runs at once and the urgent thread straight after it".to_owned()
            },
        );

        widgets::group(ui, "What an interrupt handler may call", |ui| {
            egui::Grid::new("allowed").striped(true).num_columns(3).show(ui, |ui| {
                for (call, ok, why) in [
                    ("k_sem_give(&sem)", true, "never waits: the usual way to wake a thread"),
                    ("k_msgq_put(&q, &item, K_NO_WAIT)", true, "copies the item in, or fails at once if the queue is full"),
                    ("k_work_submit(&work)", true, "hands a job to the workqueue thread"),
                    ("k_sem_take(&sem, K_NO_WAIT)", true, "allowed, because it does not wait"),
                    ("k_sem_take(&sem, K_FOREVER)", false, "would wait - a handler must never wait"),
                    ("k_sleep(K_MSEC(1))", false, "sleeping is waiting"),
                    ("k_mutex_lock(&mutex, ...)", false, "mutexes are for threads only"),
                ] {
                    ui.add(egui::Label::new(RichText::new(call).monospace()).extend());
                    ui.label(RichText::new(if ok { "yes" } else { "no" }).strong().color(if ok { pal.good } else { pal.bad }));
                    ui.add(egui::Label::new(why).extend());
                    ui.end_row();
                }
            });
        });
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "cooperative" => self.s.background_cooperative = true,
            "locked" => (self.lock, self.s.lock_ms) = (true, 6.0),
            "both" => (self.lock, self.s.lock_ms, self.s.background_cooperative) = (true, 6.0, true),
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "zephyr-interrupts",
        title: "Interrupts in Zephyr: from handler to thread",
        blurb: "An event happens. How long until your code reacts - and what can stand in the way?",
        path: &["Zephyr"],
        make: || Box::new(Isr::default()),
        explain: Explanation {
            what: "The vertical line is the moment an [[interrupt]] arrives. The top row is its [[handler]], which does one \
                thing: it gives a [[semaphore]] that an urgent [[thread]] is waiting for. The middle row is that urgent \
                thread, which does the real work. The bottom row is a background thread that was busy when it all began.\n\n\
                The numbers below give the two delays that matter: from the interrupt to the handler, and from the \
                interrupt to the thread.",
            why: "In [[Zephyr]] the reaction to a hardware event has two halves. The handler runs first, immediately, \
                whatever any thread is doing - handlers take precedence over all threads, cooperative ones included. But \
                the handler is restricted: it must be quick, and it must never wait for anything, so it cannot sleep, take \
                a [[lock]], or wait on a queue. The table at the bottom shows what that means for the calls you have met so \
                far. So the handler does the minimum and passes the event on to a thread.\n\n\
                When the handler has finished, the scheduler looks at the threads again. What happens next depends on the \
                thread that was interrupted. If it is a [[preemptible thread]], the more urgent thread gets the processor \
                at once. If it is a [[cooperative thread]], it continues as if nothing had happened, and the urgent thread \
                waits until the cooperative one gives up the processor by itself. A cooperative thread is never preempted \
                by another thread - not even by one that an interrupt has just woken.\n\n\
                The other thing that can stand in the way is an interrupt lock. A thread sometimes shares data with a \
                handler and has to make sure the handler cannot run halfway through a change. It then locks interrupts \
                for a moment. An interrupt that arrives meanwhile is not lost; it is held back until the lock is released. \
                That delay hits every interrupt in the system, which is why such stretches are kept to a few instructions.",
            formulas: &[(
                "interrupt -> thread = wait for the handler + handler + wait for the interrupted thread",
                "Preemptible background thread, no lock: 0 + 0.5 + 0 = 0.5 ms. Cooperative, yielding every 10 ms, interrupt at 12 ms: 0 + 0.5 + 8 = 8.5 ms.",
            )],
            try_this: &[
                Experiment {
                    action: "Make the background thread cooperative.",
                    watch: "the handler still running at once, but the urgent thread waiting 8 ms as a pale bar until the background thread's next yield. Drag the interrupt time: the wait depends on where in the 10 ms it lands.",
                    preset: Some("cooperative"),
                },
                Experiment {
                    action: "Back to preemptible, and let the background thread lock interrupts from 10 ms for 6 ms.",
                    watch: "the handler itself starting 4 ms late, at the end of the yellow line. Everything after it is pushed back by the same amount.",
                    preset: Some("locked"),
                },
                Experiment { action: "Combine both: a cooperative thread that also locks interrupts.", watch: "the two delays adding up.", preset: Some("both") },
            ],
            leaves_out: "Times are stretched a thousandfold: real handlers and locks last microseconds. Only one interrupt is \
                shown; a more urgent interrupt can cut into a running handler, as in the microcontroller demo on interrupts. \
                How a handler is attached to its interrupt line (IRQ_CONNECT with the line number, a priority, the function \
                and an argument, then irq_enable) is normally done for you by the device driver. On ARM Cortex-M, Zephyr \
                can also leave a few \"zero-latency\" interrupts outside the lock, at the price that their handlers may not \
                call the kernel at all. The background thread yields at regular intervals here; a real cooperative thread \
                gives up the processor whenever it sleeps or waits.",
            refs: &[
                "Zephyr documentation, Kernel Services > Interrupts: \"An ISR normally preempts the execution of the current thread\"; IRQ lock; \"Thread execution resumes only once all ISR work has been completed\"; offloading ISR work; IRQ_CONNECT and irq_enable; zero-latency interrupts.",
                "Zephyr documentation, Scheduling (cooperative threads), Semaphores, Message Queues, Mutexes and Workqueue pages for which calls an ISR may make.",
            ],
        },
    }
}
