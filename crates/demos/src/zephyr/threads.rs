use crate::prelude::*;
use egui_plot::{MarkerShape, Points, Polygon};
use sim_core::zephyr::{Slicing, Thread, simulate};

const DURATION_MS: f64 = 100.0;

pub struct Threads {
    threads: Vec<Thread>,
    slicing: Slicing,
}

impl Default for Threads {
    fn default() -> Self {
        let thread = |name: &str, priority, work_ms, sleep_ms| Thread { name: name.to_owned(), priority, work_ms, sleep_ms, enabled: true };
        Self {
            threads: vec![thread("Radio", -2, 1.0, 9.0), thread("Sensor", 1, 3.0, 17.0), thread("Maths A", 5, 1.0, 0.0), thread("Maths B", 5, 1.0, 0.0)],
            slicing: Slicing { enabled: true, slice_ms: 4.0 },
        }
    }
}

/// "1", "0.5", "12.3": milliseconds without trailing zeros.
fn num(v: f64) -> String {
    format!("{}", (v * 10.0).round() / 10.0)
}

fn kind(t: &Thread) -> &'static str {
    if t.is_cooperative() { "cooperative" } else { "preemptible" }
}

/// "Maths A" -> "maths_a": a name that is valid in C.
fn c_name(t: &Thread) -> String {
    t.name.to_lowercase().replace(' ', "_")
}

impl Threads {
    /// The current settings written the way a Zephyr application states them.
    fn as_code(&self) -> String {
        let mut out = String::from("# prj.conf - the application's configuration\n");
        if self.slicing.enabled {
            out += &format!("CONFIG_TIMESLICING=y\nCONFIG_TIMESLICE_SIZE={:.0}\n", self.slicing.slice_ms);
        } else {
            out += "CONFIG_TIMESLICING=n\n";
        }
        out += "\n/* main.c - one line per thread. The arguments are: name, stack size in bytes,\n   function, its three arguments, priority, options, start delay in ms */\n";
        for t in self.threads.iter().filter(|t| t.enabled) {
            out += &format!("K_THREAD_DEFINE({0}_tid, 1024, {0}, NULL, NULL, NULL, {1}, 0, 0);\n", c_name(t), t.priority);
        }
        if let Some(t) = self.threads.iter().find(|t| t.enabled && t.sleep_ms > 0.0) {
            out += &format!(
                "\n/* each thread is a function with an endless loop, for example: */\n\
                 void {0}(void *p1, void *p2, void *p3)\n{{\n    while (1) {{\n        do_the_work();              /* takes {1} ms */\n        k_sleep(K_MSEC({2:.0}));       /* give up the processor for {2:.0} ms */\n    }}\n}}\n",
                c_name(t),
                num(t.work_ms),
                t.sleep_ms,
            );
        }
        out
    }
}

impl Demo for Threads {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "Threads", |ui| {
            for (i, t) in self.threads.iter_mut().enumerate() {
                ui.push_id(i, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.allocate_ui(vec2(110.0, ui.spacing().interact_size.y), |ui| {
                            ui.checkbox(&mut t.enabled, RichText::new(t.name.as_str()).color(pal.trace[(i + 1) % 6]).strong()).on_hover_text("Untick to remove this thread.");
                        });
                        widgets::wrap(ui, "priority", |ui| {
                            ui.add(egui::Slider::new(&mut t.priority, -4..=8).text("priority"))
                                .on_hover_text("Lower number = more urgent. Below zero the thread is cooperative: no other thread can take the processor away from it. From zero upward it is preemptible.")
                        });
                        ui.label(RichText::new(kind(t)).color(if t.is_cooperative() { pal.warn } else { pal.dim }));
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.add_space(24.0);
                        widgets::wrap(ui, "work", |ui| {
                            ui.add_enabled(t.sleep_ms > 0.0, egui::Slider::new(&mut t.work_ms, 0.2..=30.0).logarithmic(true).text("works for").suffix(" ms").fixed_decimals(1))
                                .on_hover_text("How much processor time one pass of the thread's loop needs. Without a sleep the thread simply works all the time.")
                        });
                        widgets::wrap(ui, "sleep", |ui| {
                            ui.add(egui::Slider::new(&mut t.sleep_ms, 0.0..=50.0).step_by(1.0).text("then sleeps for").suffix(" ms").fixed_decimals(0))
                                .on_hover_text("How long the thread sleeps (k_sleep) after each pass. While it sleeps, other threads can run. 0 = it never sleeps.")
                        });
                    });
                });
            }
        });
        widgets::group(ui, "The scheduler's settings", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut self.slicing.enabled, "time slicing").on_hover_text("CONFIG_TIMESLICING: make preemptible threads of the same priority take turns.");
                widgets::wrap(ui, "slice", |ui| {
                    ui.add_enabled(self.slicing.enabled, egui::Slider::new(&mut self.slicing.slice_ms, 1.0..=50.0).logarithmic(true).text("time slice").suffix(" ms").fixed_decimals(0))
                        .on_hover_text("CONFIG_TIMESLICE_SIZE: how long a thread may run before an equal has to be given a turn. Zephyr's default is 20 ms.")
                });
            });
        });
        self.slicing.slice_ms = self.slicing.slice_ms.round();

        let trace = simulate(&self.threads, self.slicing, DURATION_MS);

        ui.label(RichText::new("Who has the processor, for 100 milliseconds").strong());
        let lanes = self.threads.len();
        let names: Vec<String> = std::iter::once("idle".to_owned()).chain(self.threads.iter().map(|t| format!("{} ({})", t.name, t.priority))).collect();
        plots::plot_bare("gantt", 80.0 + 50.0 * lanes as f32)
            .x_axis_label("time [ms]")
            .include_x(0.0)
            .include_x(DURATION_MS)
            .include_y(-0.6)
            .include_y(lanes as f64 + 0.6)
            .y_grid_spacer(egui_plot::uniform_grid_spacer(|_| [1.0, 1.0, 1.0]))
            .y_axis_formatter(move |mark, _| {
                let i = mark.value.round();
                if (mark.value - i).abs() < 0.01 && i >= 0.0 { names.get(i as usize).cloned().unwrap_or_default() } else { String::new() }
            })
            .show(ui, |p| {
                for s in &trace.ready {
                    let fill = pal.trace[s.who % 6].gamma_multiply(0.3);
                    p.polygon(Polygon::new("ready", plots::bar(s.start_ms, s.end_ms, s.who as f64 - 0.1, s.who as f64 + 0.1)).fill_color(fill).stroke((0.0, fill)));
                }
                for s in &trace.running {
                    let y = s.who as f64;
                    let (name, fill) = if s.who == 0 { ("idle", pal.dim.gamma_multiply(0.6)) } else { ("running", pal.trace[s.who % 6].gamma_multiply(0.8)) };
                    p.polygon(Polygon::new(name, plots::bar(s.start_ms, s.end_ms, y - 0.32, y + 0.32)).fill_color(fill).stroke((0.0, fill)));
                }
                let wakes: Vec<[f64; 2]> = trace.wakes.iter().map(|&(i, t)| [t, i as f64 + 1.45]).collect();
                p.points(Points::new("thread wakes up", wakes).shape(MarkerShape::Down).radius(4.5).color(pal.text));
            });
        widgets::key(
            ui,
            &[
                (pal.trace[1], "thick bar: running"),
                (pal.trace[1].gamma_multiply(0.3), "thin pale bar: ready, but has to wait"),
                (pal.dim.gamma_multiply(0.6), "idle: every thread is asleep"),
                (pal.text, "triangle: thread wakes up from its sleep"),
            ],
        );

        let enabled: Vec<(usize, &Thread)> = self.threads.iter().enumerate().filter(|(_, t)| t.enabled).collect();
        widgets::group(ui, "How each thread fared", |ui| {
            egui::Grid::new("stats").striped(true).num_columns(7).show(ui, |ui| {
                for h in ["thread", "priority", "kind", "share of the processor", "longest wait for the processor", "pushed aside", "slice ran out"] {
                    ui.label(RichText::new(h).strong());
                }
                ui.end_row();
                for &(i, t) in &enabled {
                    let s = trace.stats[i];
                    ui.label(RichText::new(t.name.as_str()).color(pal.trace[(i + 1) % 6]));
                    ui.monospace(format!("{}", t.priority));
                    ui.label(kind(t));
                    ui.monospace(format!("{:.0} %", 100.0 * s.cpu_ms / DURATION_MS));
                    let wait = format!("{} ms{}", num(s.worst_wait_ms), if s.waiting_at_end { " and still waiting" } else { "" });
                    ui.label(RichText::new(wait).monospace().color(if s.waiting_at_end { pal.bad } else { pal.text }));
                    ui.monospace(format!("{}x", s.preempted));
                    ui.monospace(format!("{}x", s.sliced));
                    ui.end_row();
                }
            });
        });
        let starved: Vec<&str> = enabled.iter().filter(|(i, _)| trace.stats[*i].waiting_at_end && (trace.stats[*i].runs == 0 || trace.stats[*i].worst_wait_ms >= DURATION_MS / 2.0)).map(|(_, t)| t.name.as_str()).collect();
        widgets::status(
            ui,
            starved.is_empty(),
            if starved.is_empty() { "every thread gets the processor".to_owned() } else { format!("starved - ready to run, but never given the processor: {}", starved.join(", ")) },
        );

        let mut lines = vec!["a thread that sleeps needs:  work / (work + sleep)  of the processor".to_owned()];
        let mut asked = 0.0;
        for (_, t) in enabled.iter().filter(|(_, t)| t.sleep_ms > 0.0) {
            let share = t.work_ms / (t.work_ms + t.sleep_ms);
            asked += share;
            lines.push(format!("  {:<8} {} / ({} + {}) = {:.0} %", t.name, num(t.work_ms), num(t.work_ms), num(t.sleep_ms), share * 100.0));
        }
        let hungry = enabled.iter().filter(|(_, t)| t.sleep_ms <= 0.0).count();
        lines.push(format!(
            "left for the {hungry} thread(s) that never sleep: about 100 % - {:.0} % = {:.0} %\nidle in this picture: {:.0} %",
            asked * 100.0,
            (1.0 - asked).max(0.0) * 100.0,
            trace.idle_share * 100.0
        ));
        widgets::formula(ui, lines.join("\n"));

        egui::CollapsingHeader::new(RichText::new("The same settings, written for Zephyr").strong()).default_open(true).show(ui, |ui| {
            widgets::formula(ui, self.as_code());
        });
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "no-slicing" => self.slicing.enabled = false,
            "long-slice" => self.slicing.slice_ms = 20.0,
            "coop" => {
                self.threads[1].priority = -1;
                self.threads[1].work_ms = 8.0;
            }
            "hog" => {
                self.threads[1].priority = -1;
                self.threads[1].sleep_ms = 0.0;
            }
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "zephyr-threads",
        title: "Zephyr threads: who runs when",
        blurb: "Four threads and Zephyr's scheduling rules: priorities, cooperative and preemptible threads, and time slices.",
        path: &["Zephyr"],
        make: || Box::new(Threads::default()),
        explain: Explanation {
            what: "[[Zephyr]] is an [[RTOS]]. What other systems call a task, Zephyr calls a [[thread]]. Each coloured row is one \
                thread, with its [[priority]] in brackets. A thick bar means the thread has the processor. A thin pale bar means \
                it is ready to run but has to wait. A small triangle marks the moment a thread wakes up from a sleep.\n\n\
                The four threads show the typical mix. Radio does a short, urgent job every few milliseconds. Sensor does a \
                slightly longer job less often. Maths A and Maths B are long calculations that never sleep: they take whatever \
                processor time the others leave over.\n\n\
                At the bottom, the same settings are written out the way a Zephyr application states them.",
            why: "Every Zephyr thread is a function with an endless loop: do some work, then sleep or wait for something, then \
                start over. Zephyr's [[scheduler]] decides which thread gets the processor, and its rules are short.\n\n\
                Rule 1: of all the threads that are ready, the one with the highest priority runs. In Zephyr a lower number is a \
                higher priority, so -2 beats 1 and 1 beats 5.\n\n\
                Rule 2: the sign of the priority decides how a thread can lose the processor. A thread with a priority of zero \
                or above is a [[preemptible thread]]: the moment a more urgent thread becomes ready, the scheduler takes the \
                processor away from it and gives it back later. That is [[preemption]], and it is why the Maths bars are cut \
                wherever Radio or Sensor wakes up. A thread with a negative priority is a [[cooperative thread]]: once it runs, \
                no other thread can take the processor from it, however urgent. It runs until it sleeps or waits by its own \
                choice. Zephyr recommends cooperative threads for device drivers and other time-critical work; such a \
                thread also need not worry about another thread cutting in halfway through its work. The price: a cooperative \
                thread must be short and well-behaved, or everything else stops.\n\n\
                Rule 3: among ready threads of the same priority, the one that has waited longest goes first - and then keeps \
                the processor. Maths A would run for ever and Maths B never. Time slicing fixes that: after one [[time slice]] \
                the scheduler makes the running thread [[yield]], which sends it to the back of the queue, so that an equal \
                gets a turn. Time slicing only applies to preemptible threads.\n\n\
                There is a catch, and it is worth seeing once. Whenever a different thread is given the processor, the slice \
                timer starts again from zero. A Maths thread that was pushed aside by Radio carries on afterwards with a \
                whole new slice. If Radio comes more often than a slice lasts, the slice never runs out, and the other Maths \
                thread never gets its turn. Zephyr only promises that no thread runs longer than one slice without having to \
                yield - it does not promise equal shares.",
            formulas: &[
                ("cooperative: priority -16 ... -1        preemptible: priority 0 ... 14", "The ranges with Zephyr's default settings (16 cooperative and 15 preemptible levels). Lower number = more urgent."),
                ("share of the processor = work / (work + sleep)", "Radio works 1 ms and sleeps 9 ms: 1 / 10 = 10 %. Sensor works 3 ms and sleeps 17 ms: 3 / 20 = 15 %. That leaves 75 % for the Maths threads."),
            ],
            try_this: &[
                Experiment {
                    action: "Switch time slicing off.",
                    watch: "Maths A taking all 75 % of the left-over time and Maths B showing a pale bar from start to end: ready, same priority, never run.",
                    preset: Some("no-slicing"),
                },
                Experiment {
                    action: "Switch it on again, but set the time slice to Zephyr's default of 20 ms.",
                    watch: "the same starvation. Radio pushes Maths A aside every 10 ms, and each time Maths A comes back with a new 20 ms slice that never runs out. Shorten the slice step by step and see when Maths B gets a turn.",
                    preset: Some("long-slice"),
                },
                Experiment {
                    action: "Make the Sensor thread cooperative (priority -1) and give it 8 ms of work.",
                    watch: "Radio, although more urgent, waiting up to 5 ms behind a Sensor bar that is never cut. Then drag Sensor's priority to 0: it is preemptible again and Radio's wait drops to zero.",
                    preset: Some("coop"),
                },
                Experiment {
                    action: "Make that cooperative Sensor thread never sleep.",
                    watch: "one bar across the whole picture. Radio runs once and never again, the Maths threads not at all. A cooperative thread that does not sleep or wait freezes every other thread.",
                    preset: Some("hog"),
                },
                Experiment {
                    action: "Start from the opening state and raise Maths B to priority 4.",
                    watch: "Maths A never running, with or without time slicing: slicing shares the processor among equals only.",
                    preset: None,
                },
            ],
            leaves_out: "Interrupts are missing: an interrupt handler cuts into any thread, a cooperative one too - \"cooperative\" \
                only protects a thread from other threads. Threads here only work and sleep; real ones mostly wait for things \
                (data from a queue, a semaphore, a button), which is planned as separate demos. Switching between threads is \
                drawn as free and time as continuous; the real kernel counts in clock ticks and a switch costs a few \
                microseconds. This is one processor; with several, the rules get more involved. Left out as well: the option \
                that exempts the more urgent preemptible priorities from time slicing (CONFIG_TIMESLICE_PRIORITY), deadline \
                scheduling, locking the scheduler, and Zephyr's own threads - main runs as a thread of priority 0 by default, \
                and when nothing is ready an idle thread of the lowest priority runs. The behaviour that a thread which was \
                pushed aside continues ahead of its equals with a fresh slice is not in the documentation; it is how the \
                kernel source behaves on a single processor today, and could change between versions.",
            refs: &[
                "Zephyr Project documentation, Kernel Services > Scheduling: choice of the current thread, cooperative and preemptible threads, preemptive time slicing, k_yield, k_sleep (docs.zephyrproject.org/latest/kernel/services/scheduling/index.html).",
                "Zephyr Project documentation, Kernel Services > Threads: thread priorities, K_THREAD_DEFINE (docs.zephyrproject.org/latest/kernel/services/threads/index.html).",
                "Zephyr source, kernel/Kconfig: TIMESLICING (default y), TIMESLICE_SIZE (ms, default 20), NUM_COOP_PRIORITIES (default 16), NUM_PREEMPT_PRIORITIES (default 15), MAIN_THREAD_PRIORITY (default 0).",
                "Zephyr source, kernel/sched.c (main branch, November 2026): the running thread stays in the ready queue on a single processor, and update_cache resets the time slice whenever a different thread is chosen.",
            ],
        },
    }
}
