use crate::prelude::*;
use egui_plot::Polygon;
use sim_core::zephyr::{LogCall, LogFate, LogLevel, run_logging};

const DURATION_MS: f64 = 100.0;
const LEVEL_NAMES: [&str; 5] = ["NONE", "ERR", "WRN", "INF", "DBG"];

/// 100 ms in the life of a small application: a chatty sensor module and a few events.
fn calls() -> Vec<LogCall> {
    let call = |at_ms, module, level, text| LogCall { at_ms, module, level, text };
    let mut v = Vec::new();
    for k in 0..10 {
        v.push(call(3.0 + 10.0 * k as f64, "sensor", LogLevel::Dbg, "raw 0x0191"));
    }
    for k in 0..5 {
        v.push(call(4.0 + 20.0 * k as f64, "sensor", LogLevel::Inf, "temperature 25.06 C"));
    }
    v.push(call(31.0, "app", LogLevel::Inf, "button pressed"));
    v.push(call(52.0, "app", LogLevel::Wrn, "queue almost full (7 of 8)"));
    v.push(call(73.0, "app", LogLevel::Err, "sensor read failed (-5)"));
    v.sort_by(|a, b| a.at_ms.total_cmp(&b.at_ms));
    v
}

pub struct Logging {
    sensor_level: u8,
    app_level: u8,
    immediate: bool,
    baud: f64,
    buffer: usize,
}

impl Default for Logging {
    fn default() -> Self {
        Self { sensor_level: 4, app_level: 3, immediate: true, baud: 115_200.0, buffer: 4 }
    }
}

impl Demo for Logging {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "How logging is built in", |ui| {
            for (name, level) in [("sensor", &mut self.sensor_level), ("app", &mut self.app_level)] {
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("Messages kept in module \"{name}\":"));
                    for (l, text) in LEVEL_NAMES.iter().enumerate() {
                        ui.selectable_value(&mut *level, l as u8, *text).on_hover_text("The module's compiled-in level. Messages less important than this are removed when the firmware is built: they cost no flash and no time. DBG keeps everything, NONE nothing.");
                    }
                });
            }
            ui.horizontal_wrapped(|ui| {
                ui.label("Mode:");
                ui.selectable_value(&mut self.immediate, true, "immediate").on_hover_text("CONFIG_LOG_MODE_IMMEDIATE: the line is formatted and sent out inside the log call. The calling code waits until the last character has left.");
                ui.selectable_value(&mut self.immediate, false, "deferred").on_hover_text("CONFIG_LOG_MODE_DEFERRED: the log call only stores the message. A separate logging thread formats and sends it later.");
                ui.add_space(12.0);
                ui.label("Serial line:");
                for baud in [115_200.0, 1_000_000.0] {
                    ui.selectable_value(&mut self.baud, baud, si(baud, "bit/s")).on_hover_text("The speed of the serial port the log is printed on. Each character takes 10 bits.");
                }
                widgets::wrap(ui, "buffer", |ui| ui.add_enabled(!self.immediate, egui::Slider::new(&mut self.buffer, 1..=16).text("buffer").suffix(" messages")).on_hover_text("Deferred mode: how many messages can wait to be printed. A message that finds the buffer full is dropped."));
            });
        });

        let calls = calls();
        let levels = [("sensor", self.sensor_level), ("app", self.app_level)];
        let run = run_logging(&calls, &levels, self.immediate, self.baud, self.buffer);
        let held: f64 = run.held_ms.iter().sum();
        let dropped = run.fates.iter().filter(|f| **f == LogFate::Dropped).count();

        ui.label(RichText::new("100 ms of the application").strong());
        let names = ["serial line", "calling code held up", "log calls"].map(str::to_owned).to_vec();
        plots::timeline("lanes", names, "time [ms]", DURATION_MS).show(ui, |p| {
            for (c, fate) in calls.iter().zip(&run.fates) {
                let (at, fill) = match *fate {
                    LogFate::CompiledOut => (c.at_ms, pal.dim.gamma_multiply(0.5)),
                    LogFate::Dropped => (c.at_ms, pal.bad),
                    LogFate::Printed(from, _) if from >= DURATION_MS => continue,
                    LogFate::Printed(from, to) => {
                        let line = pal.trace[0].gamma_multiply(0.8);
                        p.polygon(Polygon::new("printing", plots::bar(from, to.min(DURATION_MS), -0.3, 0.3)).fill_color(line).stroke((1.0, pal.canvas)));
                        if self.immediate {
                            p.polygon(Polygon::new("held up", plots::bar(from, to.min(DURATION_MS), 0.7, 1.3)).fill_color(pal.bad.gamma_multiply(0.8)).stroke((1.0, pal.canvas)));
                        }
                        (if self.immediate { from } else { c.at_ms }, pal.trace[1])
                    }
                };
                if at < DURATION_MS {
                    p.polygon(Polygon::new("call", plots::bar(at - 0.15, at + 0.15, 1.7, 2.3)).fill_color(fill).stroke((0.0, fill)));
                }
            }
        });
        widgets::key(ui, &[(pal.trace[1], "log call"), (pal.dim, "call removed at build time"), (pal.bad, "message dropped / code waiting for the serial line"), (pal.trace[0], "line being sent")]);

        ui.label(RichText::new("What appears on the serial console").strong());
        let mut printed: Vec<(f64, String)> = calls
            .iter()
            .zip(&run.fates)
            .filter_map(|(c, f)| match *f {
                LogFate::Printed(from, _) => Some((from, LogCall { at_ms: if self.immediate { from } else { c.at_ms }, ..*c }.line())),
                _ => None,
            })
            .collect();
        printed.sort_by(|a, b| a.0.total_cmp(&b.0));
        let console: Vec<String> = printed.iter().take(9).map(|(_, l)| l.clone()).collect();
        widgets::formula(ui, if console.is_empty() { "(nothing: every message was removed at build time)".to_owned() } else { format!("{}{}", console.join("\n"), if printed.len() > 9 { "\n..." } else { "" }) });

        let example = calls.iter().find(|c| c.level == LogLevel::Inf).map_or(0, |c| c.line().len());
        widgets::formula(
            ui,
            format!(
                "time to send one line  = characters x 10 bits / line speed = {example} x 10 / {} = {}\n\
                 calling code held up   : {:.1} ms of {DURATION_MS:.0} ms      messages dropped: {dropped}\n\
                 flash for message text : {} bytes\n\n\
                 LOG_MODULE_REGISTER(sensor, LOG_LEVEL_{});   /* module name, compiled-in level */\n\
                 LOG_INF(\"temperature %d.%02d C\", whole, hundredths);",
                si(self.baud, "bit/s"),
                si(example as f64 * 10.0 / self.baud, "s"),
                held,
                run.text_bytes,
                LEVEL_NAMES[self.sensor_level as usize],
            ),
        );
        widgets::status(
            ui,
            held < 1.0 && dropped == 0,
            if held >= 1.0 {
                format!("logging itself takes {:.0} % of the time: the code that logs is waiting for the serial line", held / DURATION_MS * 100.0)
            } else if dropped > 0 {
                format!("{dropped} message(s) were dropped: the buffer was full because the serial line is slower than the messages arrive")
            } else {
                "the code that logs is not slowed down, and no message is lost".to_owned()
            },
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "deferred" => (self.immediate, self.buffer) = (false, 16),
            "small-buffer" => (self.immediate, self.buffer) = (false, 1),
            "quiet" => self.sensor_level = 2,
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "zephyr-logging",
        title: "Logging: what a log line costs",
        blurb: "A harmless-looking log message can stop your code for five milliseconds. See where the time goes, and how to get it back.",
        path: &["Zephyr"],
        make: || Box::new(Logging::default()),
        explain: Explanation {
            what: "The top row marks every place in 100 ms where the firmware wants to write a log message. The bottom row \
                shows the serial line sending the text. The middle row shows the price: the stretches in which the code that \
                logged is standing still, waiting for the line.\n\n\
                Below is the text as it would appear on a serial console, and the cost in time and flash.",
            why: "Firmware has no screen. To see what it is doing, you let it print lines of text over a serial port: a log. \
                [[Zephyr]]'s logging system adds the time, a level and the name of the module to each message. There are \
                four levels, from serious to chatty: error, warning, info and debug.\n\n\
                Printing is slow, and that is easy to forget. At 115 200 bits per second a character takes 87 microseconds, \
                and a typical line of fifty characters takes more than four milliseconds - an eternity for a processor that \
                executes tens of thousands of instructions in that time. In immediate mode all of that happens inside the \
                log call: the calling code stands still until the last character is out. Code that logs a lot then spends \
                most of its time logging, and its timing no longer resembles that of the real product.\n\n\
                Deferred mode separates the two. The log call only stores the message, which takes microseconds. A logging \
                [[thread]] of low priority formats and sends the messages when nothing more important is running. The price \
                is a buffer: if messages arrive faster than the line can send them, the buffer fills up and messages are \
                dropped. And when the firmware crashes, the last messages may still be in the buffer - which is exactly when \
                you want them, and the reason immediate mode exists.\n\n\
                The second control is what gets built in at all. Each module is compiled with a level, and every message \
                less important than that is removed by the compiler: no text in flash, no time spent. So a debug message \
                costs nothing in a build that is compiled at info level.",
            formulas: &[("time to send a line = characters x 10 bits / line speed", "52 characters at 115 200 bit/s: 52 x 10 / 115 200 = 4.5 ms.")],
            try_this: &[
                Experiment {
                    action: "Switch to deferred mode with a buffer of 16 messages.",
                    watch: "the red row emptying: the calling code is no longer held up. The serial line is just as busy as before, but now in the background.",
                    preset: Some("deferred"),
                },
                Experiment {
                    action: "Keep deferred mode, but with room for only 1 message.",
                    watch: "red marks in the top row: messages that found the buffer full and were dropped. Note which ones survive - it is a matter of timing, not of importance.",
                    preset: Some("small-buffer"),
                },
                Experiment {
                    action: "Back in immediate mode, compile the sensor module at WRN.",
                    watch: "its fifteen messages turning grey and the time lost to logging falling from about 73 ms to about 13 ms - and the flash use with it.",
                    preset: Some("quiet"),
                },
                Experiment { action: "In immediate mode, switch the serial line to 1 Mbit/s.", watch: "every bar shrinking to a ninth. A faster line is the cheapest cure, if the other end can keep up.", preset: None },
            ],
            leaves_out: "Storing a message in deferred mode is drawn as free; it takes a few microseconds. Formatting the text \
                costs processor time too, on top of the sending. With CONFIG_LOG_MODE_OVERFLOW the oldest message is discarded \
                instead of the newest. Levels can also be changed while the firmware runs, and the log can go to other \
                places than a serial port (a debug probe, flash, Bluetooth). Zephyr's shell - an interactive command line on \
                the same serial port - is not shown.",
            refs: &[
                "Zephyr documentation, Logging: four severity levels; LOG_MODULE_REGISTER; compile-time filtering; \"Deferred logging reduces the time needed to log a message by shifting time consuming operations to a known context\"; immediate mode \"in the calling context\"; \"the new log is dropped if space for a message cannot be allocated\".",
                "Zephyr source, subsys/logging/log_output.c: line format \"[%02u:%02u:%02u.%03u,%03u] <%s> %s: \" with the level names err, wrn, inf, dbg.",
            ],
        },
    }
}
