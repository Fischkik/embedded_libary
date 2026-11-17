use crate::prelude::*;
use egui_plot::Polygon;
use sim_core::mcu::{LockState, LockTask, simulate_lock};

const DURATION_MS: f64 = 60.0;
const HIGH_ARRIVES_MS: f64 = 5.0;

pub struct PriorityInversion {
    medium_ms: f64,
    critical_ms: f64,
    inheritance: bool,
    deadline_ms: f64,
}

impl Default for PriorityInversion {
    fn default() -> Self {
        Self { medium_ms: 20.0, critical_ms: 6.0, inheritance: false, deadline_ms: 12.0 }
    }
}

impl PriorityInversion {
    fn tasks(&self) -> Vec<LockTask> {
        let t = |name: &str, priority, arrive_ms, script: &[(f64, bool)]| LockTask { name: name.to_owned(), priority, arrive_ms, script: script.to_vec() };
        vec![
            t("Low: logging", 3, 0.0, &[(2.0, false), (self.critical_ms, true), (1.0, false)]),
            t("Medium: number work", 2, 7.0, &[(self.medium_ms, false)]),
            t("High: motor control", 1, HIGH_ARRIVES_MS, &[(1.0, false), (2.0, true), (1.0, false)]),
        ]
    }
}

impl Demo for PriorityInversion {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "The situation", |ui| {
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "critical", |ui| {
                    ui.add(egui::Slider::new(&mut self.critical_ms, 5.0..=12.0).step_by(1.0).fixed_decimals(0).text("Low holds the lock for").suffix(" ms"))
                        .on_hover_text("How long the least urgent task works on the shared data. While it does, it holds the lock.")
                });
                widgets::wrap(ui, "medium", |ui| {
                    ui.add(egui::Slider::new(&mut self.medium_ms, 1.0..=40.0).step_by(1.0).fixed_decimals(0).text("Medium works for").suffix(" ms"))
                        .on_hover_text("How long the middle task needs. It never touches the shared data or the lock.")
                });
                widgets::wrap(ui, "deadline", |ui| {
                    ui.add(egui::Slider::new(&mut self.deadline_ms, 5.0..=40.0).step_by(1.0).fixed_decimals(0).text("High must be done within").suffix(" ms"))
                        .on_hover_text("The deadline of the most urgent task, counted from the moment it becomes ready.")
                });
            });
            ui.checkbox(&mut self.inheritance, "priority inheritance")
                .on_hover_text("The cure: while a task holds a lock that a more urgent task is waiting for, it runs at that task's priority.");
        });

        let tasks = self.tasks();
        let tr = simulate_lock(&tasks, self.inheritance, DURATION_MS);
        ui.label(RichText::new("Who has the processor, and who holds the lock").strong());
        let names: Vec<String> = tasks.iter().map(|t| format!("{} ({})", t.name, t.priority)).collect();
        plots::timeline("gantt", names, "time [ms]", DURATION_MS).show(ui, |p| {
            for &(task, state, from, to) in &tr.spans {
                let y = task as f64;
                let color = pal.trace[(task + 1) % 6];
                let (name, fill, half) = match state {
                    LockState::Running => ("running", color.gamma_multiply(0.8), 0.3),
                    LockState::Ready => ("ready", color.gamma_multiply(0.3), 0.1),
                    LockState::Blocked => ("blocked", pal.bad, 0.1),
                };
                p.polygon(Polygon::new(name, plots::bar(from, to, y - half, y + half)).fill_color(fill).stroke((0.0, fill)));
            }
            for &(task, from, to) in &tr.held {
                let y = task as f64 + 0.4;
                p.polygon(Polygon::new("holds the lock", plots::bar(from, to, y - 0.05, y + 0.05)).fill_color(pal.text).stroke((0.0, pal.text)));
            }
        });
        widgets::key(
            ui,
            &[
                (pal.trace[1], "thick bar: running"),
                (pal.trace[1].gamma_multiply(0.3), "thin pale bar: ready, a more urgent task runs"),
                (pal.bad, "thin red bar: blocked, waiting for the lock"),
                (pal.text, "line above a bar: this task holds the lock"),
            ],
        );

        let response = tr.finished_ms[2].map(|f| f - HIGH_ARRIVES_MS);
        let ok = response.is_some_and(|r| r <= self.deadline_ms + 1e-6);
        widgets::status(
            ui,
            ok,
            match response {
                Some(r) => format!("the most urgent task needed {r:.0} ms (allowed: {:.0} ms)", self.deadline_ms),
                None => "the most urgent task never finished".to_owned(),
            },
        );
        // At 6 ms High asks for the lock; Low has then done 3 ms of its critical section.
        let rest = self.critical_ms - 3.0;
        widgets::formula(
            ui,
            if self.inheritance {
                format!("High waits for:  the rest of Low's critical section = {rest:.0} ms\nMedium's {:.0} ms do not matter: Low runs at High's priority until it lets go of the lock", self.medium_ms)
            } else {
                format!(
                    "High waits for:  the rest of Low's critical section + ALL of Medium's work\n                 = {rest:.0} ms + {:.0} ms = {:.0} ms     (Low gets only 1 ms done before Medium takes over)",
                    self.medium_ms,
                    rest + self.medium_ms
                )
            },
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "inheritance" => self.inheritance = true,
            "long-medium" => self.medium_ms = 40.0,
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "mcu-priority-inversion",
        title: "Priority inversion",
        blurb: "The most urgent task waits for a task that is not urgent at all. A famous bug, and its cure.",
        path: &["Inside the microcontroller"],
        make: || Box::new(PriorityInversion::default()),
        explain: Explanation {
            what: "Three tasks share one processor. The most urgent one is at the top. The top and the bottom task both use the \
                same data, protected by a [[lock]]; the line above a bar shows who holds it. The middle task has nothing to \
                do with that data.\n\n\
                Follow the top row: the urgent task starts at 5 ms, runs for a moment, then turns red - it needs the lock, \
                which the bottom task still holds. And then the middle task takes over the processor for a long time.",
            why: "When two tasks use the same data - a message buffer, a bus, a list - they must not be in the middle of \
                changing it at the same time. So each one takes a lock first (in an [[RTOS]] this is called a mutex) and gives \
                it back afterwards. Whoever finds the lock taken has to wait.\n\n\
                A short wait would be fine: the low task only needs the lock briefly. The trouble is the middle task. It is \
                more urgent than the low task, so it pushes it aside - [[preemption]] working as designed. But the low task is \
                the only one who can give the lock back. As long as the middle task runs, the low task cannot finish, and so \
                the high task cannot continue either. The most urgent task in the system is in effect waiting for the middle \
                one. Their priorities have been turned upside down: this is priority inversion, and the wait is as long as \
                the middle task cares to run.\n\n\
                The cure is called priority inheritance. While a task holds a lock that a more urgent task is waiting for, it \
                borrows that task's [[priority]]. Now the middle task cannot push it aside, it finishes quickly, hands over \
                the lock, drops back to its own priority, and the urgent task runs.\n\n\
                In 1997 exactly this bug kept resetting the Mars Pathfinder lander: an urgent task waited for a lock held by \
                a low-priority task, a watchdog noticed the urgent task was late and reset the computer. The fix, sent from \
                Earth, was to switch on priority inheritance for that lock.",
            formulas: &[
                ("without inheritance: wait = rest of the low task's critical section + all work of every middle task", "3 ms + 20 ms = 23 ms here - and it grows with whatever the middle tasks do."),
                ("with inheritance: wait = rest of the low task's critical section", "3 ms, however busy the middle task is."),
            ],
            try_this: &[
                Experiment {
                    action: "Make the middle task work for 40 ms.",
                    watch: "the red bar of the urgent task growing with it. A task that has nothing to do with the lock decides how late the urgent one is.",
                    preset: Some("long-medium"),
                },
                Experiment {
                    action: "Tick \"priority inheritance\".",
                    watch: "the bottom task continuing straight through the moment the middle task becomes ready, the lock changing hands early, and the urgent task meeting its deadline.",
                    preset: Some("inheritance"),
                },
                Experiment {
                    action: "With inheritance on, lengthen the time Low holds the lock.",
                    watch: "the urgent task still waiting for the rest of that critical section. Inheritance removes the middle task from the wait, not the lock itself: keep critical sections short.",
                    preset: None,
                },
            ],
            leaves_out: "One lock and three tasks that each run once; real systems have many locks, and taking several at once \
                can also lead to deadlock, where two tasks wait for each other for ever. An alternative cure, the priority \
                ceiling protocol, raises a task's priority as soon as it takes the lock. Semaphores usually have no \
                inheritance, only mutexes do. Switching between tasks is drawn as free.",
            refs: &[
                "L. Sha, R. Rajkumar, J. P. Lehoczky, \"Priority Inheritance Protocols: An Approach to Real-Time Synchronization\", IEEE Transactions on Computers, 1990.",
                "Mars Pathfinder (1997) priority inversion, as described by G. Reeves (JPL), \"What really happened on Mars\".",
            ],
        },
    }
}
