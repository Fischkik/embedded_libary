use crate::prelude::*;
use sim_core::mcu::{ClockTree, uart_mismatch_limit};

/// Highest processor clock of the example chip.
const MAX_CPU_HZ: f64 = 100e6;
const RC: (f64, f64) = (16e6, 10_000.0);
const CRYSTAL: (f64, f64) = (8e6, 20.0);

pub struct Clocks {
    c: ClockTree,
}

impl Default for Clocks {
    fn default() -> Self {
        Self { c: ClockTree { source_hz: CRYSTAL.0, tolerance_ppm: CRYSTAL.1, pll_on: true, pll_div: 1, pll_mul: 12, sys_div: 2, bus_div: 2 } }
    }
}

impl Demo for Clocks {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        let c = &mut self.c;
        widgets::group(ui, "The clock tree's settings", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Source:");
                let is_rc = c.source_hz == RC.0;
                if ui.selectable_label(is_rc, "built-in RC oscillator, 16 MHz").on_hover_text("An oscillator made of a resistor and a capacitor on the chip itself. Free and instantly ready, but only accurate to about 1 %.").clicked() {
                    (c.source_hz, c.tolerance_ppm) = RC;
                }
                if ui.selectable_label(!is_rc, "external crystal, 8 MHz").on_hover_text("A quartz crystal next to the chip. An extra part, but accurate to a few millionths.").clicked() {
                    (c.source_hz, c.tolerance_ppm) = CRYSTAL;
                }
                widgets::wrap(ui, "tol", |ui| {
                    ui.add(egui::Slider::new(&mut c.tolerance_ppm, 1.0..=50_000.0).logarithmic(true).text("accuracy (plus or minus)").suffix(" ppm").fixed_decimals(0))
                        .on_hover_text("How far the source may be off, in parts per million. 10 000 ppm is 1 %.")
                });
            });
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut c.pll_on, "use the PLL").on_hover_text("A circuit that turns one frequency into a higher one: it first divides the source, then multiplies the result.");
                widgets::wrap(ui, "div", |ui| ui.add_enabled(c.pll_on, egui::Slider::new(&mut c.pll_div, 1..=8).text("divide by")).on_hover_text("The PLL first divides the source frequency by this."));
                widgets::wrap(ui, "mul", |ui| ui.add_enabled(c.pll_on, egui::Slider::new(&mut c.pll_mul, 2..=50).text("multiply by")).on_hover_text("Then it multiplies by this."));
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("System divider:");
                for d in [1, 2, 4, 8] {
                    ui.selectable_value(&mut c.sys_div, d, format!("/{d}")).on_hover_text("Divides the clock for the processor. A slower processor draws less current.");
                }
                ui.add_space(12.0);
                ui.label("Bus divider:");
                for d in [1, 2, 4] {
                    ui.selectable_value(&mut c.bus_div, d, format!("/{d}")).on_hover_text("Divides the processor clock once more for the timers, serial ports and other built-in hardware.");
                }
            });
        });
        let c = self.c;

        // The tree as a picture: five boxes joined by arrows.
        let stages = [
            ("source", if c.source_hz == RC.0 { "RC oscillator" } else { "crystal" }.to_owned(), c.source_hz),
            ("PLL", if c.pll_on { format!("/{}  x{}", c.pll_div, c.pll_mul) } else { "bypassed".to_owned() }, c.pll_hz()),
            ("system divider", format!("/{}", c.sys_div), c.cpu_hz()),
            ("processor", "runs the program".to_owned(), c.cpu_hz()),
            ("bus divider", format!("/{}", c.bus_div), c.bus_hz()),
            ("peripherals", "timers, serial ...".to_owned(), c.bus_hz()),
        ];
        let total = ui.available_width().min(780.0);
        let (rect, _) = ui.allocate_exact_size(vec2(total, 86.0), egui::Sense::hover());
        let gap = 16.0;
        let bw = (total - gap * (stages.len() as f32 - 1.0)) / stages.len() as f32;
        let painter = ui.painter();
        for (i, (title, detail, hz)) in stages.iter().enumerate() {
            let b = egui::Rect::from_min_size(pos2(rect.left() + i as f32 * (bw + gap), rect.top() + 4.0), vec2(bw, 78.0));
            let hot = *hz > MAX_CPU_HZ && i >= 2;
            painter.rect_filled(b, 4.0, pal.canvas);
            painter.rect_stroke(b, 4.0, (1.5, if hot { pal.bad } else { pal.trace[0] }), egui::StrokeKind::Inside);
            painter.text(b.center_top() + vec2(0.0, 14.0), egui::Align2::CENTER_CENTER, *title, egui::FontId::proportional(13.0), pal.text);
            painter.text(b.center(), egui::Align2::CENTER_CENTER, detail, egui::FontId::proportional(12.0), pal.dim);
            painter.text(b.center_bottom() - vec2(0.0, 14.0), egui::Align2::CENTER_CENTER, si(*hz, "Hz"), egui::FontId::monospace(14.0), if hot { pal.bad } else { pal.trace[1] });
            if i + 1 < stages.len() {
                painter.arrow(pos2(b.right() + 1.0, b.center().y), vec2(gap - 2.0, 0.0), (1.5, pal.dim));
            }
        }

        let tol = c.tolerance_ppm * 1e-6;
        let limit = uart_mismatch_limit(10);
        widgets::formula(
            ui,
            format!(
                "PLL output   = source / divide x multiply = {}\n\
                 processor    = PLL output / system divider = {} / {} = {}\n\
                 peripherals  = processor clock / bus divider = {} / {} = {}\n\
                 accuracy     = +/- {:.0} ppm = +/- {:.4} %  ->  a clock built on it is off by up to {:.1} s per day\n\
                 serial link  : two such devices can differ by {:.3} %; the link breaks at {:.1} %\n\
                 current      : roughly 0.1 mA per MHz -> about {:.1} mA for the processor",
                if c.pll_on { format!("{} / {} x {} = {}", si(c.source_hz, "Hz"), c.pll_div, c.pll_mul, si(c.pll_hz(), "Hz")) } else { format!("{} (PLL not used)", si(c.source_hz, "Hz")) },
                si(c.pll_hz(), "Hz"),
                c.sys_div,
                si(c.cpu_hz(), "Hz"),
                si(c.cpu_hz(), "Hz"),
                c.bus_div,
                si(c.bus_hz(), "Hz"),
                c.tolerance_ppm,
                tol * 100.0,
                c.drift_s_per_day(),
                2.0 * tol * 100.0,
                limit * 100.0,
                c.cpu_hz() / 1e6 * 0.1,
            ),
        );
        widgets::status(
            ui,
            c.cpu_hz() <= MAX_CPU_HZ,
            if c.cpu_hz() <= MAX_CPU_HZ { format!("the processor clock is within this example chip's limit of {}", si(MAX_CPU_HZ, "Hz")) } else { format!("the processor clock is above this example chip's limit of {}: it would compute wrong results", si(MAX_CPU_HZ, "Hz")) },
        );
        widgets::status(
            ui,
            2.0 * tol < limit / 2.0,
            if 2.0 * tol < limit / 2.0 { "accurate enough for a serial link to another device like it".to_owned() } else { "too inaccurate for a reliable serial link: more than half of the tolerable mismatch is used up by the clocks alone".to_owned() },
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        let c = &mut self.c;
        match preset {
            "rc" => (c.source_hz, c.tolerance_ppm, c.pll_div) = (RC.0, RC.1, 2),
            "slow" => (c.pll_on, c.sys_div) = (false, 8),
            "too-fast" => c.pll_mul = 30,
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "mcu-clocks",
        title: "Clock sources and the clock tree",
        blurb: "One oscillator, a multiplier and a few dividers set the pace for everything on the chip.",
        path: &["Inside the microcontroller"],
        make: || Box::new(Clocks::default()),
        explain: Explanation {
            what: "The row of boxes is the path the [[clock]] takes through a microcontroller, from left to right. It starts at a \
                source, an [[oscillator]]. A [[PLL]] can multiply its frequency. Dividers then slow it down again, once for the \
                processor and once more for the built-in hardware such as timers and serial ports. The number in each box is \
                the frequency at that point.\n\n\
                Below, the same numbers as formulas, together with what the accuracy of the source means in practice.",
            why: "Everything a microcontroller does happens in step with a clock: the processor executes one small step per \
                clock pulse, a timer counts clock pulses, a serial port measures the length of a bit in clock pulses. So two \
                things matter about a clock: how fast it is, and how exact.\n\n\
                Speed is a trade. A faster clock means more computing per second, but the current the chip draws rises almost \
                in proportion. Firmware therefore picks the slowest clock that gets the job done, and can change it while \
                running.\n\n\
                Accuracy depends on the source. The oscillator built into the chip costs nothing and starts at once, but it \
                is typically only accurate to about one percent, and drifts with temperature. A quartz [[crystal]] next to \
                the chip is accurate to a few parts per million. For blinking an LED one percent is fine. A clock that shows \
                the time of day would be off by many minutes per day. And two devices talking over a serial line must agree \
                on the length of a bit: over a ten-bit frame their clocks may differ by five percent at the very most, and \
                in practice by much less. Radio needs far better still, which is why every Bluetooth chip has a crystal.\n\n\
                Crystals for more than a few tens of megahertz are impractical, so a high clock is made from a low one: the \
                PLL divides the source down to a comparison frequency and then multiplies it up. The output is as accurate, \
                in percent, as the source it is made from.",
            formulas: &[
                ("processor clock = source / PLL divide x PLL multiply / system divider", "8 MHz / 1 x 12 / 2 = 48 MHz."),
                ("drift per day = accuracy x 86 400 s", "20 ppm: 20 / 1 000 000 x 86 400 s = 1.7 s per day. 1 %: 864 s, over 14 minutes."),
                ("largest mismatch a serial link survives = 0.5 bit / 9.5 bits = 5.3 %", "The receiver samples each bit in its middle; by the last of 10 bits the error must still be under half a bit."),
            ],
            try_this: &[
                Experiment {
                    action: "Switch the source to the built-in RC oscillator.",
                    watch: "the same 48 MHz for the processor, but the accuracy line: off by up to 864 s per day, and 2 % of mismatch between two such devices on a serial link.",
                    preset: Some("rc"),
                },
                Experiment {
                    action: "Switch the PLL off and set the system divider to /8.",
                    watch: "the processor dropping to 1 MHz and its current to a tenth of a milliampere. Slow, but enough for a device that only reads a sensor now and then.",
                    preset: Some("slow"),
                },
                Experiment {
                    action: "Raise the PLL multiplier to 30.",
                    watch: "the boxes turning red: 120 MHz is more than this chip is built for. Then find two different divider settings that bring it back under the limit.",
                    preset: Some("too-fast"),
                },
            ],
            leaves_out: "A real clock tree has more branches: several bus dividers, separate clocks for USB or the analog \
                converter, and a second, very slow oscillator (32.768 kHz) that keeps a clock running during sleep. PLLs only \
                work within limits for their input and internal frequencies, and take time to settle after switching on. \
                Faster clocks may also need extra wait cycles for flash memory. The 100 MHz limit, the 1 % and the 20 ppm are \
                typical example values; the datasheet of a chip gives its own.",
            refs: &[
                "Generic clock tree of ARM Cortex-M microcontrollers: oscillator sources, PLL with input divider and multiplier, system and bus prescalers.",
                "Asynchronous serial timing: sampling at mid-bit, so the accumulated error over a 10-bit frame must stay below half a bit (0.5 / 9.5).",
            ],
        },
    }
}
