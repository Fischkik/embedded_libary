use crate::prelude::*;
use egui_plot::Polygon;
use sim_core::zephyr::{WorkItem, simulate_workqueue};

const DURATION_MS: f64 = 100.0;

pub struct Workqueue {
    items: Vec<WorkItem>,
}

impl Default for Workqueue {
    fn default() -> Self {
        let item = |name: &str, period_ms, offset_ms, handler_ms| WorkItem { name: name.to_owned(), period_ms, offset_ms, handler_ms, enabled: true };
        Self { items: vec![item("Sensor data ready", 10.0, 0.0, 1.0), item("Button pressed", 30.0, 5.0, 2.0), item("Save settings", 50.0, 12.0, 4.0)] }
    }
}

impl Demo for Workqueue {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "Three interrupts, each handing a job to the workqueue", |ui| {
            for (i, item) in self.items.iter_mut().enumerate() {
                ui.push_id(i, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.allocate_ui(vec2(150.0, ui.spacing().interact_size.y), |ui| {
                            ui.checkbox(&mut item.enabled, RichText::new(item.name.as_str()).color(pal.trace[(i + 1) % 6]).strong()).on_hover_text("Untick to remove this interrupt.");
                        });
                        widgets::wrap(ui, "period", |ui| {
                            ui.add(egui::Slider::new(&mut item.period_ms, 1.0..=100.0).logarithmic(true).text("every").suffix(" ms").fixed_decimals(0)).on_hover_text("How often this interrupt happens. Each time, its handler submits the work item.")
                        });
                        widgets::wrap(ui, "handler", |ui| {
                            ui.add(egui::Slider::new(&mut item.handler_ms, 0.2..=40.0).logarithmic(true).text("the job takes").suffix(" ms").fixed_decimals(1)).on_hover_text("How long the job's function runs on the workqueue thread.")
                        });
                    });
                });
            }
        });
        let tr = simulate_workqueue(&self.items, DURATION_MS);

        ui.label(RichText::new("Interrupts submit, one thread works through the jobs in order").strong());
        let names: Vec<String> = std::iter::once("workqueue thread".to_owned()).chain(self.items.iter().map(|i| i.name.clone())).collect();
        plots::timeline("lanes", names, "time [ms]", DURATION_MS).show(ui, |p| {
            for &(i, t, merged) in &tr.submits {
                let fill = if merged { pal.bad } else { pal.trace[(i + 1) % 6] };
                p.polygon(Polygon::new("submitted", plots::bar(t - 0.15, t + 0.15, i as f64 + 0.7, i as f64 + 1.3)).fill_color(fill).stroke((0.0, fill)));
            }
            for &(i, from, to) in &tr.running {
                let fill = pal.trace[(i + 1) % 6].gamma_multiply(0.8);
                p.polygon(Polygon::new("job running", plots::bar(from, to.min(DURATION_MS), -0.3, 0.3)).fill_color(fill).stroke((1.0, pal.canvas)));
            }
        });
        widgets::key(ui, &[(pal.trace[1], "thin bar: an interrupt submits its work item"), (pal.bad, "submitted while already queued: no effect"), (pal.trace[1].gamma_multiply(0.8), "wide bar: the job runs, in the colour of its interrupt")]);

        widgets::group(ui, "How each job fared", |ui| {
            egui::Grid::new("stats").striped(true).num_columns(4).show(ui, |ui| {
                for h in ["job", "times it ran", "submissions without effect", "longest wait before it started"] {
                    ui.label(RichText::new(h).strong());
                }
                ui.end_row();
                for (i, item) in self.items.iter().enumerate().filter(|(_, it)| it.enabled) {
                    ui.label(RichText::new(item.name.as_str()).color(pal.trace[(i + 1) % 6]));
                    ui.monospace(format!("{}", tr.runs[i]));
                    ui.label(RichText::new(format!("{}", tr.merged[i])).monospace().color(if tr.merged[i] > 0 { pal.bad } else { pal.good }));
                    ui.monospace(format!("{:.1} ms", tr.worst_delay_ms[i]));
                    ui.end_row();
                }
            });
        });
        let merged: u32 = tr.merged.iter().sum();
        widgets::status(
            ui,
            merged == 0,
            if merged == 0 { "every submission led to its own run of the job".to_owned() } else { format!("{merged} submission(s) had no effect: the work item was still waiting in the queue, so the job ran once for several interrupts") },
        );
        let enabled: Vec<&WorkItem> = self.items.iter().filter(|i| i.enabled).collect();
        let terms: Vec<String> = enabled.iter().map(|i| format!("{:.1}/{:.0}", i.handler_ms, i.period_ms)).collect();
        let load: f64 = enabled.iter().map(|i| i.handler_ms / i.period_ms).sum();
        let longest = enabled.iter().map(|i| i.handler_ms).fold(0.0, f64::max);
        widgets::formula(
            ui,
            format!(
                "load on the workqueue thread = sum of (job time / interval) = {} = {:.0} %   (measured: {:.0} %)\n\
                 a job can wait for every job queued ahead of it; the longest single job is {:.1} ms\n\n\
                 K_WORK_DEFINE(button_work, button_job);     /* a work item and the function it runs */\n\
                 void button_isr(const void *arg)\n{{\n    k_work_submit(&button_work);            /* hand it to the system workqueue and return */\n}}",
                if terms.is_empty() { "0".to_owned() } else { terms.join(" + ") },
                load * 100.0,
                tr.busy_share * 100.0,
                longest,
            ),
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "slow-job" => self.items[2].handler_ms = 30.0,
            "fast-interrupt" => (self.items[0].period_ms, self.items[0].handler_ms) = (1.0, 2.0),
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "zephyr-workqueue",
        title: "Work queues: handing work to a helper thread",
        blurb: "Interrupt handlers stay short by passing their jobs to one shared thread. Why one slow job then delays all the others.",
        path: &["Zephyr"],
        make: || Box::new(Workqueue::default()),
        explain: Explanation {
            what: "The three upper rows are [[interrupt]]s. Each thin bar is the moment an interrupt [[handler]] submits a job \
                instead of doing it itself. The bottom row is a single [[thread]], the workqueue thread, running those jobs \
                one after the other; each wide bar has the colour of the interrupt it came from.",
            why: "An interrupt handler should take microseconds and is not allowed to wait for anything. But the reaction to \
                an interrupt is often real work: process the sensor data, write settings to flash, send a message. That work \
                has to happen in a thread.\n\n\
                You could create one thread per job, each waiting on its own [[semaphore]]. That works, but every thread \
                needs its own [[stack]], and memory is scarce. A [[workqueue]] is the economical alternative: one thread \
                with one stack, shared by many jobs. A job is described by a work item, which is little more than a pointer \
                to a function. Submitting a work item puts it at the end of the queue, and the thread calls the functions in \
                the order they were submitted. When the queue is empty, the thread sleeps.\n\n\
                [[Zephyr]] already provides such a thread, the system workqueue, and much of Zephyr itself uses it. By \
                default it has a cooperative priority, so its jobs run ahead of every [[preemptible thread]] in the \
                application.\n\n\
                Sharing has two consequences, and both are visible here. First, jobs wait for each other: one slow job \
                delays every job behind it, whatever interrupt it belongs to. A job on the system workqueue must therefore \
                be short and must never wait for long. Second, a work item is either in the queue or not. Submitting it \
                again while it is still waiting changes nothing: the job will run once. So a job must be written as \"deal \
                with everything that has come in\", never as \"deal with one event\".",
            formulas: &[("load on the workqueue = sum of (job time / interval)", "1 ms every 10 ms + 2 ms every 30 ms + 4 ms every 50 ms = 10 % + 6.7 % + 8 % = 25 %.")],
            try_this: &[
                Experiment {
                    action: "Make \"Save settings\" slow: 30 ms, as writing to flash can be.",
                    watch: "the sensor's bars turning red while the long job runs: the interrupts keep coming, but the work item is already queued, and afterwards its job runs once for all of them.",
                    preset: Some("slow-job"),
                },
                Experiment {
                    action: "Let the sensor interrupt come every millisecond with a job of 2 ms.",
                    watch: "many submissions having no effect, and the other two jobs squeezed in between. The thread hardly ever sleeps.",
                    preset: Some("fast-interrupt"),
                },
                Experiment { action: "Untick the two other interrupts and compare the sensor's \"longest wait\".", watch: "the wait dropping to zero: all of its delay came from sharing the thread.", preset: None },
            ],
            leaves_out: "The interrupts are perfectly regular here. Submitting a work item whose job is running at that moment \
                does queue it again; only an item that is still waiting is unaffected. Work can also be scheduled to run after \
                a delay (k_work_schedule), which is how timeouts and \"wait until the button has stopped bouncing\" are \
                written. An application can create its own workqueues with their own priority for jobs that are slow or \
                must wait. Other threads and the interrupts' own run time are not drawn.",
            refs: &[
                "Zephyr documentation, Kernel Services > Threads > Workqueue Threads: \"a dedicated thread to process work items in a first in, first out manner\"; \"If an ISR or a thread attempts to submit a work item that is already queued the work item is not affected\"; system workqueue (\"the lowest cooperative priority by default\"); K_WORK_DEFINE, k_work_submit.",
            ],
        },
    }
}
