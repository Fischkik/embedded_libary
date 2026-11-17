use crate::prelude::*;
use egui_plot::{MarkerShape, Points, Polygon};
use sim_core::mcu::{RtosTask, rate_monotonic_bound, simulate_rtos, utilisation};

const DURATION_MS: f64 = 100.0;

pub struct Rtos {
    tasks: Vec<RtosTask>,
}

impl Default for Rtos {
    fn default() -> Self {
        let task = |name: &str, priority, period_ms, work_ms| RtosTask { name: name.to_owned(), priority, period_ms, work_ms, enabled: true };
        Self { tasks: vec![task("Motor control", 1, 5.0, 1.0), task("Sensor reading", 2, 20.0, 4.0), task("Display update", 3, 50.0, 12.0)] }
    }
}

/// "1", "0.5", "12.3": milliseconds without trailing zeros.
fn num(v: f64) -> String {
    format!("{}", (v * 10.0).round() / 10.0)
}

impl Demo for Rtos {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "Tasks", |ui| {
            for (i, t) in self.tasks.iter_mut().enumerate() {
                ui.push_id(i, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.allocate_ui(vec2(150.0, ui.spacing().interact_size.y), |ui| {
                            ui.checkbox(&mut t.enabled, RichText::new(t.name.as_str()).color(pal.trace[(i + 1) % 6]).strong()).on_hover_text("Untick to remove this task.");
                        });
                        widgets::wrap(ui, "priority", |ui| {
                            ui.add(egui::Slider::new(&mut t.priority, 0..=7).text("priority")).on_hover_text("Lower number = more urgent. The most urgent task that has work to do always gets the processor.")
                        });
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.add_space(24.0);
                        widgets::wrap(ui, "period", |ui| {
                            ui.add(egui::Slider::new(&mut t.period_ms, 1.0..=100.0).logarithmic(true).text("wakes every").suffix(" ms").fixed_decimals(1))
                                .on_hover_text("The task's period: how often it wakes up to do its job. It should be finished before it wakes up the next time.")
                        });
                        widgets::wrap(ui, "work", |ui| {
                            ui.add(egui::Slider::new(&mut t.work_ms, 0.2..=50.0).logarithmic(true).text("works for").suffix(" ms").fixed_decimals(1))
                                .on_hover_text("How much processor time the job takes each time, if nothing interrupts it.")
                        });
                    });
                });
            }
        });

        let trace = simulate_rtos(&self.tasks, DURATION_MS);

        ui.label(RichText::new("Who has the processor, for 100 milliseconds").strong());
        let lanes = self.tasks.len();
        let names: Vec<String> = std::iter::once("idle".to_owned()).chain(self.tasks.iter().map(|t| t.name.clone())).collect();
        plots::plot_bare("gantt", 90.0 + 60.0 * lanes as f32)
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
                for job in &trace.jobs {
                    let y = job.task as f64 + 1.0;
                    let fill = pal.trace[(job.task + 1) % 6].gamma_multiply(0.3);
                    let end = job.finished_ms.unwrap_or(DURATION_MS);
                    p.polygon(Polygon::new("ready: has work, waits for the processor", plots::bar(job.released_ms, end, y - 0.1, y + 0.1)).fill_color(fill).stroke((0.0, fill)));
                }
                for s in &trace.slices {
                    let y = s.who as f64;
                    let (name, fill) = if s.who == 0 { ("idle: nothing to do", pal.dim.gamma_multiply(0.6)) } else { ("running", pal.trace[s.who % 6].gamma_multiply(0.8)) };
                    p.polygon(Polygon::new(name, plots::bar(s.start_ms, s.end_ms, y - 0.32, y + 0.32)).fill_color(fill).stroke((0.0, fill)));
                }
                let wakes: Vec<[f64; 2]> = trace.jobs.iter().map(|j| [j.released_ms, j.task as f64 + 1.45]).collect();
                p.points(Points::new("task wakes up", wakes).shape(MarkerShape::Down).radius(4.5).color(pal.text));
                let missed: Vec<[f64; 2]> = trace.jobs.iter().filter(|j| trace.is_missed(j)).map(|j| [j.deadline_ms, j.task as f64 + 1.45]).collect();
                p.points(Points::new("deadline missed", missed).shape(MarkerShape::Cross).radius(7.0).color(pal.bad));
            });

        widgets::key(
            ui,
            &[
                (pal.trace[1], "thick bar: running"),
                (pal.trace[1].gamma_multiply(0.3), "thin pale bar: ready, but has to wait"),
                (pal.dim.gamma_multiply(0.6), "idle: nothing to do"),
                (pal.text, "triangle: task wakes up"),
                (pal.bad, "cross: deadline missed"),
            ],
        );

        let enabled: Vec<(usize, &RtosTask)> = self.tasks.iter().enumerate().filter(|(_, t)| t.enabled).collect();
        widgets::group(ui, "How each task fared", |ui| {
            egui::Grid::new("stats").striped(true).num_columns(5).show(ui, |ui| {
                for h in ["task", "priority", "share of the processor it needs", "slowest run, wake-up to finished", "deadlines missed"] {
                    ui.label(RichText::new(h).strong());
                }
                ui.end_row();
                for &(i, t) in &enabled {
                    ui.label(RichText::new(t.name.as_str()).color(pal.trace[(i + 1) % 6]));
                    ui.monospace(format!("{}", t.priority));
                    ui.monospace(format!("{:.0} %", 100.0 * t.work_ms / t.period_ms));
                    ui.monospace(trace.worst_response_ms(i).map_or("never finished".to_owned(), |r| format!("{} ms of {} ms", num(r), num(t.period_ms))));
                    let missed = trace.missed(i);
                    ui.label(RichText::new(format!("{missed}")).color(if missed > 0 { pal.bad } else { pal.good }).strong());
                    ui.end_row();
                }
            });
        });
        let missed: usize = (0..self.tasks.len()).map(|i| trace.missed(i)).sum();
        widgets::status(
            ui,
            missed == 0,
            if missed == 0 { "every task finished each job before its next wake-up".to_owned() } else { format!("{missed} job(s) were not finished before the task's next wake-up (its deadline)") },
        );

        let load = utilisation(&self.tasks);
        let terms: Vec<String> = enabled.iter().map(|(_, t)| format!("{}/{}", num(t.work_ms), num(t.period_ms))).collect();
        // "Rate-monotonic": whenever one task has a shorter period than another, it is also more urgent.
        let by_period = enabled.iter().all(|(_, a)| enabled.iter().all(|(_, b)| a.period_ms >= b.period_ms || a.priority < b.priority));
        let n = enabled.len();
        let bound = rate_monotonic_bound(n);
        let advice = if !by_period {
            "priorities do not follow the periods: deadlines can be missed even at a low load".to_owned()
        } else if load <= bound {
            format!("priorities follow the periods (shorter period = more urgent) and the load is below\n{n} x (2^(1/{n}) - 1) = {:.0} %: no deadline can ever be missed", bound * 100.0)
        } else if load <= 1.0 {
            format!("priorities follow the periods, but the load is above {n} x (2^(1/{n}) - 1) = {:.0} %:\nit may or may not work out - check the picture", bound * 100.0)
        } else {
            "the tasks ask for more than 100 % of the processor: no choice of priorities can save this".to_owned()
        };
        widgets::formula(
            ui,
            format!(
                "processor load = sum of (time needed / period) = {} = {:.0} %        idle in this picture: {:.0} %\n{advice}",
                if terms.is_empty() { "0".to_owned() } else { terms.join(" + ") },
                load * 100.0,
                trace.idle_share * 100.0,
            ),
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "overload" => self.tasks[2].work_ms = 35.0,
            "wrong-priority" => self.tasks[2].priority = 0,
            "equal" => self.tasks.iter_mut().for_each(|t| t.priority = 2),
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "mcu-rtos",
        title: "RTOS: tasks and scheduling",
        blurb: "One processor, three jobs with different rhythms. Watch the scheduler hand the processor back and forth.",
        path: &["Inside the microcontroller"],
        make: || Box::new(Rtos::default()),
        explain: Explanation {
            what: "Each coloured row is a [[task]]: a piece of the firmware with a job of its own. A small triangle marks the \
                moment a task wakes up because its job is due. A thick bar means the task is running on the processor right \
                now. A thin pale bar means it is ready - it has work to do - but has to wait because a more urgent task has \
                the processor. The grey row at the bottom is idle time: no task needs the processor.\n\n\
                Only one row has a thick bar at any moment, because there is only one processor. A red cross marks a missed \
                [[deadline]]: the task was still busy with its previous job when the next one was already due.",
            why: "Simple firmware is one endless loop that does everything in turn. That breaks down when the jobs have \
                different rhythms: a motor needs attention every few milliseconds, while redrawing a display takes many \
                milliseconds in one go. In a single loop the motor would have to wait for the display.\n\n\
                An [[RTOS]], a real-time operating system, solves this by letting you write each job as its own task, as if \
                it had the processor to itself. A part of the RTOS called the [[scheduler]] then decides, over and over, \
                which task really gets the processor. Its rule is simple: of all the tasks that have work to do, the one with \
                the most urgent [[priority]] runs.\n\n\
                So when the motor task wakes up while the display task is in the middle of its work, the scheduler stops the \
                display task on the spot, saves its place, runs the motor task, and afterwards lets the display task continue \
                exactly where it was. This is [[preemption]], the same idea as with an [[interrupt]]. The display task never \
                notices, it just takes longer from start to finish. You can see that as a thick bar chopped into pieces.\n\n\
                Two things can go wrong. The tasks can simply ask for more processor time than there is: the load is above \
                100 % and the least urgent task falls further and further behind. Or the priorities can be chosen badly: if a \
                long job is more urgent than a short, frequent one, the frequent one has to sit through the whole long job \
                and is late, even though the processor is idle a third of the time.\n\n\
                A rule of thumb that is proven to be the best choice for jobs like these: the shorter a task's [[period]], \
                the more urgent its priority. Engineers call it rate-monotonic priority assignment.",
            formulas: &[
                ("processor load = sum of (time needed / period)", "1 ms every 5 ms, 4 ms every 20 ms and 12 ms every 50 ms: 20 % + 20 % + 24 % = 64 %."),
                (
                    "time to finish = own work + all the work of more urgent tasks that wake up meanwhile",
                    "The display task needs 12 ms itself, but is finished only after 20 ms: in that time the motor task ran four times (4 ms) and the sensor task once (4 ms).",
                ),
                (
                    "safe load = n x (2^(1/n) - 1)",
                    "With shorter period = more urgent, n tasks never miss a deadline below this load: 83 % for two tasks, 78 % for three, 69 % for very many. Above it, it depends on the periods.",
                ),
            ],
            try_this: &[
                Experiment {
                    action: "Make the display update heavy: 35 ms of work every 50 ms.",
                    watch: "the load rising to 110 % and the idle row emptying. The two urgent tasks are untouched; the display task, the least urgent, pays for everything and misses its deadlines.",
                    preset: Some("overload"),
                },
                Experiment {
                    action: "Back to normal work, but give the display task priority 0, the most urgent.",
                    watch: "red crosses on the motor row: it has to wait 12 ms for the display although it is due every 5 ms. The load is still only 64 %.",
                    preset: Some("wrong-priority"),
                },
                Experiment {
                    action: "Give all three tasks the same priority.",
                    watch: "no bar being chopped up any more: equals do not interrupt each other, they queue. The motor task is late again.",
                    preset: Some("equal"),
                },
                Experiment {
                    action: "Start from the opening state and shorten the motor task's period to 2 ms.",
                    watch: "the other two tasks being cut into more pieces and taking longer from wake-up to finished, although their own work has not changed.",
                    preset: None,
                },
            ],
            leaves_out: "Every job here takes exactly the same time each run and all tasks wake up together at time zero, which is \
                the worst case; real tasks vary. Switching from one task to another is drawn as free; in reality it costs a few \
                microseconds. Tasks of equal priority simply queue here; many RTOSes can instead let them take turns in short \
                time slices. Real tasks also wait for things other than time - a message, a button, a lock on shared data - \
                and that is where the difficult bugs come from (one of them, priority inversion, is planned as its own demo). \
                Which way round the priority numbers count depends on the RTOS: in Zephyr a lower number is more urgent, as \
                shown here; in FreeRTOS a higher number is more urgent.",
            refs: &[
                "Fixed-priority preemptive scheduling, as used by FreeRTOS, Zephyr and most other RTOSes.",
                "C. L. Liu and J. W. Layland, \"Scheduling Algorithms for Multiprogramming in a Hard-Real-Time Environment\", Journal of the ACM, 1973: shortest period = highest priority is optimal among fixed priorities; utilisation bound n(2^(1/n) - 1).",
                "Worst-case response time of a fixed-priority task: R = C + sum over more urgent tasks of ceil(R / T) x C (standard response-time analysis).",
            ],
        },
    }
}
