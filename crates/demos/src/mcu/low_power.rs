use crate::prelude::*;
use egui_plot::{HLine, Points};
use sim_core::mcu::SleepCycle;

/// Example sleep modes: (name, sleep current in A - or as a share of the run current
/// if below zero -, wake-up time in s, description). The deep-sleep and off currents
/// are in the range a current low-power ARM Cortex-M datasheet gives; the rest is
/// illustrative. All of them can be overridden with the sliders.
const MODES: [(&str, f64, f64, &str); 3] = [
    ("Light sleep", -0.3, 2e-6, "Only the processor's clock is stopped; everything else keeps running. Wakes up almost instantly, but still draws about a third of the run current."),
    ("Deep sleep", 1.3e-6, 5e-6, "Nearly all clocks are stopped. Memory is kept, and a slow low-power timer stays on to wake the chip up."),
    ("Off", 0.4e-6, 2e-3, "Almost everything is switched off and memory is lost. On wake-up the firmware starts from the beginning, which takes time."),
];

const PERIOD_RANGE: (f64, f64) = (0.01, 3600.0);

pub struct LowPower {
    cycle: SleepCycle,
    capacity_ah: f64,
}

impl Default for LowPower {
    fn default() -> Self {
        Self { cycle: SleepCycle { period_s: 10.0, work_s: 5e-3, wake_s: MODES[1].2, run_a: 4e-3, sleep_a: MODES[1].1 }, capacity_ah: 0.225 }
    }
}

fn mode_values(mode: usize, run_a: f64) -> (f64, f64) {
    let (_, sleep, wake, _) = MODES[mode];
    (if sleep < 0.0 { -sleep * run_a } else { sleep }, wake)
}

/// "45 s", "2.3 days", "7.8 years".
fn human(seconds: f64) -> String {
    const DAY: f64 = 86_400.0;
    let (v, unit) = match seconds {
        s if s < 120.0 => (s, "s"),
        s if s < 7200.0 => (s / 60.0, "min"),
        s if s < 2.0 * DAY => (s / 3600.0, "hours"),
        s if s < 730.0 * DAY => (s / DAY, "days"),
        s => (s / (365.25 * DAY), "years"),
    };
    if v >= 100.0 || (v - v.round()).abs() < 0.05 { format!("{v:.0} {unit}") } else { format!("{v:.1} {unit}") }
}

impl Demo for LowPower {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        let c = &mut self.cycle;
        widgets::group(ui, "What the device does", |ui| {
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "period", |ui| ui.add(widgets::si_slider(&mut c.period_s, PERIOD_RANGE.0..=PERIOD_RANGE.1, "wakes up every", "s").logarithmic(true))
                    .on_hover_text("How often the device wakes up to do its job, for example to read a sensor and send the value."));
                widgets::wrap(ui, "work", |ui| ui.add(widgets::si_slider(&mut c.work_s, 100e-6..=1.0, "works for", "s").logarithmic(true)).on_hover_text("How long it is busy each time before it can go back to sleep."));
                widgets::wrap(ui, "run", |ui| ui.add(widgets::si_slider(&mut c.run_a, 0.5e-3..=50e-3, "current while awake", "A").logarithmic(true))
                    .on_hover_text("What the chip draws while the processor runs. Roughly proportional to the clock frequency: around 0.1 mA per MHz for a modern low-power chip."));
            });
        });
        widgets::group(ui, "How it sleeps in between", |ui| {
            ui.horizontal_wrapped(|ui| {
                for (i, mode) in MODES.iter().enumerate() {
                    let (sleep_a, wake_s) = mode_values(i, c.run_a);
                    let selected = (c.sleep_a / sleep_a - 1.0).abs() < 1e-6 && (c.wake_s / wake_s - 1.0).abs() < 1e-6;
                    if ui.selectable_label(selected, mode.0).on_hover_text(mode.3).clicked() {
                        (c.sleep_a, c.wake_s) = (sleep_a, wake_s);
                    }
                }
                widgets::wrap(ui, "sleep", |ui| ui.add(widgets::si_slider(&mut c.sleep_a, 10e-9..=50e-3, "current while asleep", "A").logarithmic(true))
                    .on_hover_text("What the chip draws while it sleeps. The deeper the sleep, the more of the chip is switched off and the lower this is."));
                widgets::wrap(ui, "wake", |ui| ui.add(widgets::si_slider(&mut c.wake_s, 1e-6..=0.1, "time to wake up", "s").logarithmic(true))
                    .on_hover_text("How long it takes from the wake-up signal until the firmware can do useful work again. The chip already draws its full current during this time."));
            });
        });
        ui.add(widgets::si_slider(&mut self.capacity_ah, 0.01..=10.0, "battery capacity", "Ah").logarithmic(true))
            .on_hover_text("How much charge the battery holds. 225 mAh is a CR2032 coin cell: it can deliver 225 mA for one hour, or 1 mA for 225 hours.");

        let c = self.cycle;
        let [work_s, wake_s, asleep_s] = c.times_s();
        let avg = c.average_a();
        let life_s = c.battery_life_s(self.capacity_ah * 1e3);

        ui.label(RichText::new("Current over time, two wake-ups (each grid line is 10 times more current)").strong());
        let (scale, unit) = match c.period_s {
            p if p < 2e-3 => (1e6, "µs"),
            p if p < 2.0 => (1e3, "ms"),
            p if p < 300.0 => (1.0, "s"),
            _ => (1.0 / 60.0, "min"),
        };
        let (hi, lo) = (c.run_a.log10(), c.sleep_a.log10());
        let awake_s = work_s + wake_s;
        let mut profile = vec![[0.0, lo]];
        for k in 0..2 {
            let t0 = (k as f64 + 0.1) * c.period_s;
            let asleep_until = if asleep_s > 0.0 { (k as f64 + 1.1) * c.period_s } else { t0 + awake_s };
            profile.extend([[t0, lo], [t0, hi], [t0 + awake_s, hi], [t0 + awake_s, lo], [asleep_until, lo]].map(|[t, y]| [t.min(2.1 * c.period_s) * scale, y]));
        }
        if asleep_s <= 0.0 {
            profile = vec![[0.0, hi], [2.1 * c.period_s * scale, hi]];
        }
        let amps = |mark: egui_plot::GridMark, _: &std::ops::RangeInclusive<f64>| {
            let i = mark.value.round();
            if (mark.value - i).abs() < 0.01 { si(10f64.powf(i), "A") } else { String::new() }
        };
        plots::plot("current", 200.0)
            .x_axis_label(format!("time [{unit}]"))
            .y_axis_label("current")
            .include_y(hi.max(lo) + 0.4)
            .include_y(hi.min(lo) - 0.4)
            .y_grid_spacer(egui_plot::uniform_grid_spacer(|_| [1.0, 1.0, 1.0]))
            .y_axis_formatter(amps)
            .show(ui, |p| {
                p.line(plots::line("current drawn from the battery", profile, pal.trace[0]).width(2.0));
                p.hline(HLine::new("average current", avg.log10()).color(pal.trace[1]).style(egui_plot::LineStyle::dashed_dense()).width(2.0));
            });

        ui.label(RichText::new("Where the battery's charge goes").strong());
        let charge = c.charge_c();
        let total: f64 = charge.iter().sum();
        let parts = [("working", pal.trace[3]), ("waking up", pal.trace[1]), ("sleeping", pal.trace[0])];
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width().min(760.0), 22.0), egui::Sense::hover());
        let mut x = rect.left();
        for (q, (_, color)) in charge.iter().zip(parts) {
            let w = (q / total) as f32 * rect.width();
            ui.painter().rect_filled(egui::Rect::from_min_size(pos2(x, rect.top()), vec2(w, rect.height())), 0.0, color);
            x += w;
        }
        ui.horizontal_wrapped(|ui| {
            for (q, (name, color)) in charge.iter().zip(parts) {
                ui.label(RichText::new(format!("{name}: {:.1} %", 100.0 * q / total)).color(color).strong());
            }
        });

        widgets::formula(
            ui,
            format!(
                "average current = (awake current x time awake + sleep current x time asleep) / period\n\
                 \x20               = ({} x {} + {} x {}) / {} = {}\n\
                 battery life    = capacity / average current = {} / {} = {}",
                si(c.run_a, "A"),
                si(awake_s, "s"),
                si(c.sleep_a, "A"),
                si(asleep_s, "s"),
                si(c.period_s, "s"),
                si(avg, "A"),
                si(self.capacity_ah, "Ah"),
                si(avg, "A"),
                human(life_s),
            ),
        );
        widgets::status(
            ui,
            asleep_s > 0.0,
            if asleep_s > 0.0 {
                format!("asleep {:.3} % of the time", 100.0 * asleep_s / c.period_s)
            } else {
                "waking up and working take longer than the period: the device never sleeps".to_owned()
            },
        );

        ui.label(RichText::new("Battery life against how often the device wakes up").strong());
        let days = |cycle: SleepCycle| (cycle.battery_life_s(self.capacity_ah * 1e3) / 86_400.0).log10();
        let curve = |sleep_a: f64, wake_s: f64| -> Vec<[f64; 2]> {
            let (a, b) = (PERIOD_RANGE.0.log10(), PERIOD_RANGE.1.log10());
            (0..=120).map(|i| a + (b - a) * i as f64 / 120.0).map(|x| [x, days(SleepCycle { period_s: 10f64.powf(x), sleep_a, wake_s, ..c })]).collect()
        };
        plots::plot("life", 240.0)
            .legend(egui_plot::Legend::default().position(egui_plot::Corner::LeftTop))
            .x_axis_label("time between wake-ups")
            .y_axis_label("battery life")
            .x_grid_spacer(egui_plot::uniform_grid_spacer(|_| [1.0, 1.0, 1.0]))
            .y_grid_spacer(egui_plot::uniform_grid_spacer(|_| [1.0, 1.0, 1.0]))
            .x_axis_formatter(|mark, _| {
                let i = mark.value.round();
                if (mark.value - i).abs() > 0.01 {
                    String::new()
                } else if i < 0.0 {
                    si(10f64.powf(i), "s")
                } else {
                    human(10f64.powf(i))
                }
            })
            .y_axis_formatter(|mark, _| {
                let i = mark.value.round();
                if (mark.value - i).abs() < 0.01 { human(10f64.powf(i) * 86_400.0) } else { String::new() }
            })
            .show(ui, |p| {
                p.line(plots::line("never sleeping", curve(c.run_a, 0.0), pal.dim).style(egui_plot::LineStyle::dashed_dense()));
                for (i, mode) in MODES.iter().enumerate() {
                    let (sleep_a, wake_s) = mode_values(i, c.run_a);
                    p.line(plots::line(mode.0, curve(sleep_a, wake_s), pal.trace[[4, 2, 5][i]]));
                }
                p.line(plots::line("your settings", curve(c.sleep_a, c.wake_s), pal.trace[0]).width(2.5));
                p.points(Points::new("now", vec![[c.period_s.log10(), days(c)]]).radius(6.0).color(pal.text));
            });
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        let c = &mut self.cycle;
        match preset {
            "awake" => (c.sleep_a, c.wake_s) = (c.run_a, 1e-6),
            "frequent" => c.period_s = 0.1,
            "rare" => c.period_s = 600.0,
            "restart" => {
                c.period_s = 2.0;
                (c.sleep_a, c.wake_s) = mode_values(2, c.run_a);
            }
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "mcu-low-power",
        title: "Sleep modes and battery life",
        blurb: "A coin cell lasts two days or eight years. The difference is how well the microcontroller sleeps.",
        path: &["Inside the microcontroller"],
        make: || Box::new(LowPower::default()),
        explain: Explanation {
            what: "A battery-powered device - think of a wireless temperature sensor - wakes up now and then, does a short job, \
                and sleeps again. The top graph shows the current it draws from the battery over two such wake-ups: a tall \
                narrow spike while it is awake and a very low floor while it sleeps. The scale is squeezed so that both fit: \
                each grid line is ten times more current than the one below. The dashed line is the [[average current]].\n\n\
                The coloured bar shows where the battery's charge actually goes. The bottom graph shows how long the battery \
                lasts depending on how often the device wakes up, for three kinds of sleep. The dot is the current setting.",
            why: "A running processor draws a few milliamperes. A coin cell holds about 225 [[mAh]]: enough for 225 mA for one \
                hour, or 4 mA for about 56 hours. A sensor that should last for years on such a battery cannot stay awake.\n\n\
                The good news is that most devices have nothing to do most of the time. A microcontroller draws current mainly \
                because its [[clock]] makes millions of transistors switch every second. Stop the clock and the current \
                collapses. That is what a [[sleep mode]] does, and chips offer several depths:\n\n\
                Light sleep stops only the processor's clock. Timers and other built-in hardware keep running and the chip is \
                awake again within a few clock ticks, but it still draws a good part of its normal current.\n\n\
                Deep sleep stops nearly all clocks. The memory keeps its contents and only a slow, frugal timer stays on to \
                wake the chip. The current drops to around a microampere - thousands of times less than when running.\n\n\
                Off goes further and cuts the power to the memory too. The current is lower still, but on wake-up the firmware \
                starts from scratch, as after a reset, and that takes time at full current.\n\n\
                What the battery sees in the end is the average: the tall spike counts for little if it is short, and the tiny \
                sleep current counts for a lot because it flows nearly all the time. In the opening state the device is awake \
                for 5 ms every 10 s - one part in two thousand - and still the short bursts use more than half the charge.\n\n\
                This also tells you where to save. If working dominates the bar, a deeper sleep changes nothing: make the job \
                shorter or rarer. If sleeping dominates, working faster changes nothing: sleep deeper. And a slow [[wake-up \
                time]] is paid at every wake-up, so the deepest sleep only wins if wake-ups are rare.",
            formulas: &[
                (
                    "average current = (awake current x time awake + sleep current x time asleep) / period",
                    "4 mA for 5 ms and 1.3 µA for the rest of 10 s: (20 µAs + 13 µAs) / 10 s = 3.3 µA.",
                ),
                ("battery life = capacity / average current", "225 mAh / 3.3 µA = 68 000 hours, almost 8 years. Never sleeping: 225 mAh / 4 mA = 56 hours."),
            ],
            try_this: &[
                Experiment {
                    action: "Take the sleep away: set the current while asleep as high as the current while awake.",
                    watch: "the top graph becoming a flat line at 4 mA and the battery life falling from almost eight years to a little over two days.",
                    preset: Some("awake"),
                },
                Experiment {
                    action: "Wake up ten times per second instead of every ten seconds.",
                    watch: "the bar turning almost completely to \"working\". Now drag the current while asleep down as far as it goes: the battery life hardly changes, because sleep is no longer where the charge goes.",
                    preset: Some("frequent"),
                },
                Experiment {
                    action: "Wake up only every ten minutes.",
                    watch: "the bar turning almost completely to \"sleeping\" and the dot sitting on the flat part of its curve: waking up even more rarely would gain nothing. Only a lower sleep current helps now.",
                    preset: Some("rare"),
                },
                Experiment {
                    action: "Wake up every 2 seconds and click between \"Deep sleep\" and \"Off\".",
                    watch: "\"Off\" having the lower sleep current but the shorter battery life: restarting the firmware every time costs more than the deeper sleep saves. In the bottom graph the two curves cross at about 9 s.",
                    preset: Some("restart"),
                },
            ],
            leaves_out: "The currents and wake-up times of the three sleep modes are example values of a realistic size, not those \
                of a particular chip: look them up in the datasheet of yours, where the modes also have other names (Sleep, \
                Stop, Standby, System OFF and so on). The model has only two current levels; a real device draws far more \
                than the processor's run current while its radio transmits or a sensor is powered, and waking up is a ramp, \
                not a step. Everything else on the board draws current too: voltage regulators, pull-up resistors, sensors \
                left switched on. A real battery does not deliver its full capacity: a coin cell's voltage sags under current \
                pulses, cold reduces its capacity, and it slowly loses charge by itself, so results of many years are \
                optimistic. How the clock is generated and distributed inside the chip is a topic of its own.",
            refs: &[
                "Average current of a duty-cycled load: I_avg = (sum of current x time) / period; battery life = capacity / I_avg.",
                "Example currents in the range given for the STM32L4 series (STMicroelectronics product overview and power training): run about 100 µA/MHz, Stop 2 with RTC about 1.3 µA, Standby with RTC about 0.4 µA, wake-up from Stop about 5 µs. The light-sleep current and the 2 ms restart time are illustrative.",
                "CR2032 nominal capacity 225 mAh: Panasonic CR2032 datasheet.",
            ],
        },
    }
}
