use crate::prelude::*;
use sim_core::firmware::{LightEvent, LightFsm, LightState, light_transition};

pub struct StateMachine {
    fsm: LightFsm,
    clock: Clock,
}

impl Default for StateMachine {
    fn default() -> Self {
        let mut fsm = LightFsm::new(10.0, 3.0);
        fsm.fire(LightEvent::Motion);
        fsm.advance(3.0);
        Self { fsm, clock: Clock { speed: 1.0, ..Default::default() } }
    }
}

impl Demo for StateMachine {
    fn ui(&mut self, ui: &mut Ui, cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        let dt = self.clock.tick(ui, cx.dt);
        self.fsm.advance(dt);

        widgets::group(ui, "A lamp with a motion sensor and a wall button", |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Motion detected").on_hover_text("The sensor reports movement. Fires the event \"motion\".").clicked() {
                    self.fsm.fire(LightEvent::Motion);
                }
                if ui.button("Button pressed").on_hover_text("Someone presses the wall button. Fires the event \"button\".").clicked() {
                    self.fsm.fire(LightEvent::Button);
                }
                widgets::wrap(ui, "hold", |ui| ui.add(egui::Slider::new(&mut self.fsm.hold_s, 2.0..=30.0).text("hold time").suffix(" s").fixed_decimals(0)).on_hover_text("How long the lamp stays fully on after the last movement."));
                widgets::wrap(ui, "warn", |ui| ui.add(egui::Slider::new(&mut self.fsm.warn_s, 1.0..=10.0).text("warning time").suffix(" s").fixed_decimals(0)).on_hover_text("How long the lamp stays dimmed, as a warning, before it goes out."));
            });
            if self.clock.controls(ui) {
                self.fsm = LightFsm::new(self.fsm.hold_s, self.fsm.warn_s);
            }
        });
        let fsm = &self.fsm;

        // The state diagram: three boxes and the arrows between them.
        ui.label(RichText::new("The state diagram: the machine is always in exactly one box").strong());
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width().min(760.0), 190.0), egui::Sense::hover());
        let painter = ui.painter();
        let (bw, bh) = (130.0, 50.0);
        let xs = [rect.left() + 20.0, rect.center().x - bw / 2.0, rect.right() - bw - 20.0];
        let y = rect.top() + 60.0;
        let boxes: Vec<egui::Rect> = xs.iter().map(|&x| egui::Rect::from_min_size(pos2(x, y), vec2(bw, bh))).collect();
        let small = egui::FontId::proportional(12.5);
        let arrow = |from: Pos2, to: Pos2, label: &str, above: bool| {
            let dir = (to - from).normalized();
            painter.line_segment([from, to - dir * 6.0], (1.5, pal.dim));
            painter.arrow(to - dir * 12.0, dir * 12.0, (1.5, pal.dim));
            let mid = pos2((from.x + to.x) / 2.0, from.y + if above { -9.0 } else { 10.0 });
            painter.text(mid, egui::Align2::CENTER_CENTER, label, small.clone(), pal.text);
        };
        arrow(boxes[0].right_top() + vec2(4.0, 12.0), boxes[1].left_top() + vec2(-4.0, 12.0), "motion, button", true);
        arrow(boxes[1].left_bottom() + vec2(-4.0, -12.0), boxes[0].right_bottom() + vec2(4.0, -12.0), "button", false);
        arrow(boxes[1].right_top() + vec2(4.0, 12.0), boxes[2].left_top() + vec2(-4.0, 12.0), "timer expired", true);
        arrow(boxes[2].left_bottom() + vec2(-4.0, -12.0), boxes[1].right_bottom() + vec2(4.0, -12.0), "motion", false);
        // WARNING -> OFF runs underneath the middle box.
        let (a, b) = (boxes[2].center_bottom() + vec2(0.0, 4.0), boxes[0].center_bottom() + vec2(0.0, 4.0));
        painter.line_segment([a, a + vec2(0.0, 36.0)], (1.5, pal.dim));
        painter.line_segment([a + vec2(0.0, 36.0), b + vec2(0.0, 36.0)], (1.5, pal.dim));
        painter.line_segment([b + vec2(0.0, 36.0), b + vec2(0.0, 6.0)], (1.5, pal.dim));
        painter.arrow(b + vec2(0.0, 12.0), vec2(0.0, -12.0), (1.5, pal.dim));
        painter.text(pos2((a.x + b.x) / 2.0, a.y + 46.0), egui::Align2::CENTER_CENTER, "timer expired, button", small.clone(), pal.text);
        painter.text(boxes[1].center_top() + vec2(0.0, -30.0), egui::Align2::CENTER_CENTER, "motion: restart the timer (stays ON)", small.clone(), pal.text);
        for (state, b) in LightState::ALL.iter().zip(&boxes) {
            let current = *state == fsm.state;
            painter.rect_filled(*b, 8.0, if current { pal.trace[2].gamma_multiply(0.6) } else { pal.canvas });
            painter.rect_stroke(*b, 8.0, (if current { 3.0 } else { 1.5 }, if current { pal.trace[2] } else { pal.dim }), egui::StrokeKind::Inside);
            painter.text(b.center() - vec2(0.0, 8.0), egui::Align2::CENTER_CENTER, state.name(), egui::FontId::monospace(15.0), ui.visuals().strong_text_color());
            painter.text(b.center() + vec2(0.0, 11.0), egui::Align2::CENTER_CENTER, format!("lamp at {:.0} %", state.brightness() * 100.0), small.clone(), pal.text);
        }

        ui.horizontal(|ui| {
            let (lamp, _) = ui.allocate_exact_size(vec2(56.0, 56.0), egui::Sense::hover());
            let seen = (fsm.state.brightness() as f32).powf(1.0 / 2.2);
            ui.painter().circle_filled(lamp.center(), 24.0, Color32::from_rgb((255.0 * seen) as u8, (220.0 * seen) as u8, (90.0 * seen) as u8));
            ui.painter().circle_stroke(lamp.center(), 24.0, (1.5, pal.dim));
            ui.vertical(|ui| {
                ui.label(RichText::new(format!("State: {}     time: {:.1} s", fsm.state.name(), fsm.now_s)).strong());
                let total = if fsm.state == LightState::Warning { fsm.warn_s } else { fsm.hold_s };
                if fsm.state == LightState::Off {
                    ui.label("No timer running. The machine waits for an event.");
                } else {
                    ui.add(egui::ProgressBar::new((fsm.timer_s / total).clamp(0.0, 1.0) as f32).desired_width(260.0).text(format!("timer: {:.1} s left", fsm.timer_s.max(0.0))));
                }
            });
        });

        widgets::group(ui, "The same behaviour as a table: state (row) x event (column) -> next state, action", |ui| {
            egui::Grid::new("table").striped(true).num_columns(4).show(ui, |ui| {
                ui.label("");
                for e in LightEvent::ALL {
                    ui.label(RichText::new(e.name()).strong());
                }
                ui.end_row();
                for s in LightState::ALL {
                    let current = s == fsm.state;
                    ui.label(RichText::new(s.name()).monospace().strong().color(if current { pal.trace[2] } else { pal.text }));
                    for e in LightEvent::ALL {
                        let text = match light_transition(s, e) {
                            Some((next, action)) => format!("-> {}: {action}", next.name()),
                            None => "ignored".to_owned(),
                        };
                        ui.add(egui::Label::new(RichText::new(text).small().color(if current { pal.text } else { pal.dim })).extend());
                    }
                    ui.end_row();
                }
            });
        });

        let last = fsm.log.last().map_or("nothing has happened yet".to_owned(), |(t, from, e, to)| format!("at {t:.1} s:  table[{}][{}] = {}", from.name(), e.name(), to.name()));
        widgets::formula(ui, format!("next state = table[current state][event]\nlast step  : {last}"));
        let history: Vec<String> = fsm.log.iter().rev().take(5).map(|(t, from, e, to)| format!("{t:>6.1} s   {:<8} --{}-->  {}", from.name(), e.name(), to.name())).collect();
        if !history.is_empty() {
            widgets::formula(ui, format!("history, newest first:\n{}", history.join("\n")));
        }
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "warning" => {
                self.fsm.advance(7.5);
                self.clock.running = false;
            }
            "off" => {
                self.fsm = LightFsm::new(10.0, 3.0);
                self.clock.running = false;
            }
            "short" => {
                self.fsm = LightFsm::new(3.0, 2.0);
                self.fsm.fire(LightEvent::Motion);
            }
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "firmware-state-machine",
        title: "State machines",
        blurb: "A lamp, a motion sensor and a button: three states and a table instead of a tangle of ifs and flags.",
        path: &["Firmware craft"],
        make: || Box::new(StateMachine::default()),
        explain: Explanation {
            what: "The three boxes are the [[state]]s a motion-controlled lamp can be in: off, fully on, and dimmed as a warning \
                that it is about to go out. The highlighted box is the state it is in right now. The arrows are the only ways \
                to get from one state to another, each labelled with the event that causes it.\n\n\
                Time is running: the bar shows the timer of the current state. Press the two buttons at the top to cause \
                events yourself. The table underneath is the same diagram written as rows and columns.",
            why: "Most firmware spends its life waiting for things to happen and reacting to them. The reaction usually depends \
                on what happened before: movement while the lamp is off switches it on, movement while it is on only \
                restarts a timer. The first idea is to remember the past in a handful of true/false variables - lamp_is_on, \
                timer_running, warning_given - and to test combinations of them everywhere. That works for a while. Then \
                someone presses the button during the warning, a combination nobody thought of, and the lamp does \
                something strange.\n\n\
                A [[state machine]] replaces the handful of flags with one variable: the state. The device is always in \
                exactly one state, and for each state you write down what every possible event does: which state comes \
                next, and what to do on the way. Nothing else can happen. Every combination is a cell in the table, and an \
                empty cell is a visible gap, not a hidden bug.\n\n\
                This has practical consequences. The firmware can be read like the diagram. A missing case is found by \
                looking at the table, not by testing for weeks. Adding a feature means adding a state or a column, not \
                revisiting every if. And the state can be printed in a log, which makes odd behaviour in the field \
                explainable.\n\n\
                Timers fit in naturally: a timer running out is just one more event. Almost every communication protocol, \
                every menu, every charger and every motor controller is written this way.",
            formulas: &[("next state, action = table[current state][event]", "table[ON][timer expired] = WARNING, with the action \"lamp 30 %, start warning timer\".")],
            try_this: &[
                Experiment {
                    action: "With time stopped in WARNING, press \"Motion detected\", then \"Play\".",
                    watch: "the machine going back to ON with a full hold timer. That one arrow is the whole feature \"wave your arm and the light stays on\".",
                    preset: Some("warning"),
                },
                Experiment {
                    action: "Start from OFF with time stopped, and press \"Button pressed\" twice.",
                    watch: "the same event doing two different things, on and then off, because the machine was in a different state each time. Check both in the table.",
                    preset: Some("off"),
                },
                Experiment {
                    action: "Set a hold time of 3 s and keep pressing \"Motion detected\" every second or two.",
                    watch: "the machine staying in ON for as long as you keep it up: each motion event restarts the timer, and the history fills with ON -> ON steps.",
                    preset: Some("short"),
                },
            ],
            leaves_out: "Three states and three events; real machines have dozens, and large ones are split into several small \
                machines or into states that contain sub-states. Events here are handled the instant they happen; in real \
                firmware they come from interrupts and are queued for the machine. In code the table is often a switch \
                statement per state rather than an actual table - the idea is the same. A real lamp controller would also \
                look at the daylight level and fade instead of jumping.",
            refs: &["Finite state machine (Mealy type: the action belongs to the transition), as taught in any embedded-software textbook, for example M. Samek, \"Practical UML Statecharts in C/C++\"."],
        },
    }
}
