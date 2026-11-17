use crate::prelude::*;
use egui_plot::{Points, Polygon};
use sim_core::mcu::{Activity, IrqSource, simulate_nvic};

const DURATION_US: f64 = 2000.0;
/// Clock cycles an ARM Cortex-M3/M4 needs to save its registers and enter a handler.
const ENTRY_CYCLES: f64 = 12.0;

pub struct Interrupts {
    sources: Vec<IrqSource>,
    cpu_mhz: f64,
}

impl Default for Interrupts {
    fn default() -> Self {
        let src = |name: &str, priority, period_us, offset_us, handler_us| IrqSource { name: name.to_owned(), priority, period_us, offset_us, handler_us, enabled: true };
        Self {
            sources: vec![src("Serial byte received", 1, 260.0, 60.0, 12.0), src("1 ms system tick", 2, 1000.0, 200.0, 60.0), src("Sensor timer", 3, 700.0, 150.0, 180.0)],
            cpu_mhz: 48.0,
        }
    }
}

impl Demo for Interrupts {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "Interrupt sources", |ui| {
            for (i, s) in self.sources.iter_mut().enumerate() {
                ui.push_id(i, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.checkbox(&mut s.enabled, RichText::new(s.name.as_str()).color(pal.trace[(i + 1) % 6]).strong());
                        widgets::wrap(ui, "priority", |ui| ui.add(egui::Slider::new(&mut s.priority, 0..=7).text("priority")).on_hover_text("Lower number = more urgent (this is how ARM Cortex-M counts)."));
                        widgets::wrap(ui, "period", |ui| ui.add(egui::Slider::new(&mut s.period_us, 50.0..=2000.0).logarithmic(true).text("period").suffix(" µs").fixed_decimals(0)).on_hover_text("How often this interrupt is requested."));
                        widgets::wrap(ui, "handler", |ui| ui.add(egui::Slider::new(&mut s.handler_us, 2.0..=600.0).logarithmic(true).text("handler time").suffix(" µs").fixed_decimals(0)).on_hover_text("How long its handler function takes."));
                    });
                });
            }
        });
        ui.add(egui::Slider::new(&mut self.cpu_mhz, 1.0..=120.0).logarithmic(true).text("processor clock").suffix(" MHz").fixed_decimals(0))
            .on_hover_text("Entering a handler costs 12 clock cycles for saving registers. At a slow clock that becomes visible.");

        let entry_us = ENTRY_CYCLES / self.cpu_mhz;
        let trace = simulate_nvic(&self.sources, DURATION_US, entry_us, entry_us);

        ui.label(RichText::new("What the processor is doing, for 2 milliseconds").strong());
        let lanes = self.sources.len();
        let names: Vec<String> = std::iter::once("main loop".to_owned()).chain(self.sources.iter().map(|s| s.name.clone())).collect();
        plots::plot_bare("gantt", 90.0 + 60.0 * lanes as f32)
            .x_axis_label("time [µs]")
            .include_x(0.0)
            .include_x(DURATION_US)
            .include_y(-0.6)
            .include_y(lanes as f64 + 0.6)
            .y_grid_spacer(egui_plot::uniform_grid_spacer(|_| [1.0, 1.0, 1.0]))
            .y_axis_formatter(move |mark, _| {
                let i = mark.value.round();
                if (mark.value - i).abs() < 0.01 && i >= 0.0 { names.get(i as usize).cloned().unwrap_or_default() } else { String::new() }
            })
            .show(ui, |p| {
                for seg in &trace.segments {
                    let y = seg.who as f64;
                    let color = if seg.who == 0 { pal.dim } else { pal.trace[seg.who % 6] };
                    let (name, fill) = match seg.activity {
                        Activity::Run => ("running", color.gamma_multiply(0.75)),
                        _ => ("saving / restoring registers", pal.text.gamma_multiply(0.6)),
                    };
                    p.polygon(Polygon::new(name, plots::bar(seg.start_us, seg.end_us, y - 0.32, y + 0.32)).fill_color(fill).stroke((0.0, fill)));
                }
                let marks = |lost: bool| -> Vec<[f64; 2]> { trace.requests.iter().filter(|r| r.lost == lost).map(|r| [r.raised_us, r.irq as f64 + 1.0 + 0.45]).collect() };
                p.points(Points::new("interrupt requested", marks(false)).shape(egui_plot::MarkerShape::Down).radius(4.5).color(pal.text));
                p.points(Points::new("request lost", marks(true)).shape(egui_plot::MarkerShape::Cross).radius(6.0).color(pal.bad));
            });

        widgets::key(
            ui,
            &[
                (pal.trace[1], "coloured bar: handler running"),
                (pal.dim, "grey bar: main loop running"),
                (pal.text.gamma_multiply(0.6), "pale sliver: saving / restoring registers"),
                (pal.text, "triangle: interrupt requested"),
                (pal.bad, "cross: request lost"),
            ],
        );

        widgets::group(ui, "How each interrupt fared", |ui| {
            egui::Grid::new("stats").striped(true).num_columns(4).show(ui, |ui| {
                for h in ["interrupt", "priority", "longest wait before its handler started", "requests lost"] {
                    ui.label(RichText::new(h).strong());
                }
                ui.end_row();
                for (i, s) in self.sources.iter().enumerate().filter(|(_, s)| s.enabled) {
                    ui.label(RichText::new(s.name.as_str()).color(pal.trace[(i + 1) % 6]));
                    ui.monospace(format!("{}", s.priority));
                    ui.monospace(trace.worst_latency_us(i).map_or("never ran".to_owned(), |l| format!("{l:.1} µs")));
                    let lost = trace.lost(i);
                    ui.label(RichText::new(format!("{lost}")).color(if lost > 0 { pal.bad } else { pal.good }).strong());
                    ui.end_row();
                }
            });
        });
        let lost: usize = (0..self.sources.len()).map(|i| trace.lost(i)).sum();
        widgets::status(ui, lost == 0, if lost == 0 { "every request was served".to_owned() } else { format!("{lost} request(s) were lost: the same interrupt was requested again while it was still waiting") });
        widgets::formula(
            ui,
            format!(
                "time left for the main loop: {:.0} %        entering a handler: 12 clock cycles = {:.2} µs at {:.0} MHz",
                trace.main_share * 100.0,
                entry_us,
                self.cpu_mhz
            ),
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        match preset {
            "slow-handler" => self.sources[2].handler_us = 500.0,
            "inverted" => {
                self.sources[2].handler_us = 500.0;
                self.sources[2].priority = 0;
            }
            "starved" => {
                self.sources[0].period_us = 50.0;
                self.sources[0].handler_us = 40.0;
            }
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "mcu-interrupts",
        title: "Interrupts and priorities",
        blurb: "Three things want the processor's attention at once. See who gets it, who waits, and what gets lost.",
        path: &["Inside the microcontroller"],
        make: || Box::new(Interrupts::default()),
        explain: Explanation {
            what: "Each row is something the processor can be busy with: the ordinary program at the bottom (the [[main loop]]), \
                and above it one row per [[interrupt]]. A coloured bar means \"the processor is running this right now\". Only one \
                row can have a bar at any moment, because there is only one processor.\n\n\
                A small triangle marks the instant an interrupt is requested. The gap between a triangle and the start of its bar \
                is how long that request had to wait.",
            why: "A microcontroller has to react to things that happen at unpredictable moments: a byte arrives on the serial \
                port, a timer runs out, a button is pressed. Checking for all of them over and over in the main loop would be slow \
                and wasteful. Instead the hardware raises an interrupt: the processor stops what it is doing, saves its place on \
                the [[stack]], runs a short function called a [[handler]], then restores its place and carries on as if nothing had \
                happened.\n\n\
                When several interrupts want to run, each one's [[priority]] decides. On ARM Cortex-M processors this is handled \
                by a piece of hardware called the [[NVIC]], and a lower number means more urgent. Three rules explain everything \
                in the picture:\n\n\
                A more urgent interrupt cuts into a less urgent handler that is already running. This is [[preemption]]: the bar \
                of the lower row is split, with the urgent one in between.\n\n\
                A less urgent or equally urgent interrupt has to wait until the running handler finishes. While it waits it is \
                [[pending]].\n\n\
                Each interrupt can only be pending once. If it is requested again while still waiting, the second request is \
                simply lost. That is how serial bytes go missing and why timekeeping drifts when handlers are too slow.\n\n\
                From this follows the first rule of interrupt handlers: keep them short. A handler should grab the data, set a \
                flag, and leave the real work to the main loop. Every microsecond spent in a handler is a microsecond in which \
                everything less urgent is deaf.",
            formulas: &[
                ("worst wait = longest handler of equal or higher urgency (+ 12 clock cycles to enter)", "An interrupt's reaction time is set by other people's handlers, not its own."),
                ("lost request: period shorter than the time it has to wait", "If a source fires every 260 µs but can be blocked for 500 µs, requests will be lost."),
            ],
            try_this: &[
                Experiment {
                    action: "Make the \"Sensor timer\" handler slow: 500 µs.",
                    watch: "its long bar being chopped up by the two more urgent interrupts. They are unaffected - but the main loop gets very little time.",
                    preset: Some("slow-handler"),
                },
                Experiment {
                    action: "Now also give that slow handler priority 0, the most urgent.",
                    watch: "red crosses on the serial row: bytes arriving during the long handler are lost, because nothing can interrupt it.",
                    preset: Some("inverted"),
                },
                Experiment {
                    action: "Make the serial interrupt fire every 50 µs with a 40 µs handler.",
                    watch: "the top row taking almost all the time, the other interrupts served late, and the main loop barely running at all.",
                    preset: Some("starved"),
                },
                Experiment {
                    action: "Lower the processor clock to 1 MHz.",
                    watch: "pale slivers appearing before and after every handler: the time spent saving and restoring registers, normally too short to see.",
                    preset: None,
                },
            ],
            leaves_out: "Interrupt sources fire at perfectly regular intervals here; real ones are irregular. The model has only \
                preemption priorities - real Cortex-M chips can also split the priority number into groups and sub-priorities, and \
                can mask interrupts for short critical sections. Handlers do not share data with the main loop here, which in real \
                firmware is where most interrupt bugs come from.",
            refs: &["ARMv7-M exception model: 12-cycle interrupt entry on Cortex-M3/M4, lower priority value = higher urgency, tail-chaining."],
        },
    }
}
