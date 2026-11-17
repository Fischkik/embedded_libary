use crate::prelude::*;
use egui_plot::{Points, Polygon, VLine};
use sim_core::bus::{SpiMode, level_at, spi_transfer};

pub struct Spi {
    controller: u8,
    peripheral: u8,
    mosi: u8,
    miso: u8,
}

impl Default for Spi {
    fn default() -> Self {
        Self { controller: 0, peripheral: 0, mosi: 0xA5, miso: 0x3C }
    }
}

fn mode_buttons(ui: &mut Ui, label: &str, mode: &mut u8) {
    ui.horizontal_wrapped(|ui| {
        ui.label(label);
        for n in 0..4u8 {
            let m = SpiMode::from_number(n);
            ui.selectable_value(mode, n, format!("mode {n}")).on_hover_text(format!(
                "The clock rests {} between bytes, and data is read on the {} edge of each clock pulse.",
                if m.cpol { "high" } else { "low" },
                if m.samples_on_rising() { "rising" } else { "falling" }
            ));
        }
    });
}

impl Demo for Spi {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "Controller and peripheral", |ui| {
            mode_buttons(ui, "The controller generates the clock in", &mut self.controller);
            mode_buttons(ui, "The peripheral chip expects", &mut self.peripheral);
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "mosi", |ui| ui.add(egui::Slider::new(&mut self.mosi, 0..=255).text("controller sends").hexadecimal(2, false, true)).on_hover_text("The byte the controller shifts out on MOSI."));
                widgets::wrap(ui, "miso", |ui| ui.add(egui::Slider::new(&mut self.miso, 0..=255).text("peripheral sends").hexadecimal(2, false, true)).on_hover_text("The byte the peripheral shifts out on MISO during the same eight clock pulses."));
            });
        });
        let (cm, pm) = (SpiMode::from_number(self.controller), SpiMode::from_number(self.peripheral));
        let t = spi_transfer(cm, pm, self.mosi, self.miso);

        ui.label(RichText::new("One byte each way: four wires over time").strong());
        let names = ["MISO: data to the controller", "MOSI: data to the peripheral", "SCK: clock", "CS: chip select"].map(str::to_owned).to_vec();
        plots::timeline("wires", names, "time [clock periods]", t.duration).show(ui, |p| {
            let band = pal.trace[0].gamma_multiply(0.12);
            p.polygon(Polygon::new("selected", plots::bar(t.cs.0, t.cs.1, -0.5, 3.5)).fill_color(band).stroke((0.0, band)));
            let wave = |edges: &[(f64, bool)], initial: bool, row: f64| plots::digital(edges, initial, 0.0, t.duration, row - 0.28, 0.56);
            p.line(plots::line("cs", wave(&[(t.cs.0, false), (t.cs.1, true)], true, 3.0), pal.text).width(2.0));
            p.line(plots::line("sck", wave(&t.sck, cm.cpol, 2.0), pal.trace[1]).width(2.0));
            p.line(plots::line("mosi", wave(&t.mosi, true, 1.0), pal.trace[0]).width(2.0));
            p.line(plots::line("miso", wave(&t.miso, true, 0.0), pal.trace[2]).width(2.0));
            for &s in &t.controller_samples {
                p.vline(VLine::new("controller samples", s).color(pal.grid));
            }
            let dots = |edges: &[(f64, bool)], times: &[f64], row: f64| -> Vec<[f64; 2]> { times.iter().map(|&s| [s, row - 0.28 + 0.56 * level_at(edges, true, s - 1e-6) as u8 as f64]).collect() };
            p.points(Points::new("controller reads", dots(&t.miso, &t.controller_samples, 0.0)).radius(4.5).color(if t.controller_read == self.miso && t.controller_margin > 0.2 { pal.good } else { pal.bad }));
            p.points(Points::new("peripheral reads", dots(&t.mosi, &t.peripheral_samples, 1.0)).radius(4.5).color(if t.peripheral_read == self.mosi && t.peripheral_margin > 0.2 { pal.good } else { pal.bad }));
        });
        widgets::key(ui, &[(pal.trace[0].gamma_multiply(0.3), "shaded: the peripheral is selected"), (pal.good, "dot: a bit is read here, safely"), (pal.bad, "dot: a bit is read here, at the wrong moment")]);

        let safe = t.controller_margin > 0.2 && t.peripheral_margin > 0.2;
        let right = t.controller_read == self.miso && t.peripheral_read == self.mosi;
        widgets::formula(
            ui,
            format!(
                "controller sent 0x{:02X}  ->  peripheral read 0x{:02X}        peripheral sent 0x{:02X}  ->  controller read 0x{:02X}\n\
                 mode number = 2 x CPOL + CPHA:   controller {} = 2 x {} + {}        peripheral {} = 2 x {} + {}\n\
                 time between a read and the nearest change of the data: controller {:.2}, peripheral {:.2} clock periods",
                self.mosi, t.peripheral_read, self.miso, t.controller_read, self.controller, cm.cpol as u8, cm.cpha as u8, self.peripheral, pm.cpol as u8, pm.cpha as u8, t.controller_margin, t.peripheral_margin
            ),
        );
        widgets::status(
            ui,
            safe && right,
            if safe && right {
                "both sides read each bit half a clock period away from any change: the transfer is safe".to_owned()
            } else if !safe && right {
                "the bytes happen to come out right here, but one side reads exactly while the data changes: on a real board this fails at random".to_owned()
            } else if !safe {
                "the modes do not match: one side reads exactly while the other changes the data, and the bytes arrive shifted".to_owned()
            } else {
                "both sides read on the same edges, but disagree about when the first bit appears: every bit arrives one place late".to_owned()
            },
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "mode3" => (self.controller, self.peripheral) = (3, 3),
            "mismatch" => self.peripheral = 1,
            "sneaky" => self.peripheral = 2,
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "bus-spi",
        title: "SPI: a clock and a ring of bits",
        blurb: "Four wires, and with every clock pulse one bit goes each way. The catch: four ways to read the clock.",
        path: &["Wired buses"],
        make: || Box::new(Spi::default()),
        explain: Explanation {
            what: "Four wires between a microcontroller (the controller) and one other chip (the peripheral), drawn over time. \
                From the top: chip select, which the controller pulls low to address this chip; the clock, eight pulses for \
                eight bits; and two data wires, one in each direction.\n\n\
                Each dot marks an instant at which a bit is read: the controller reads the bottom wire, the peripheral the \
                one above it. The numbers underneath show which bytes arrived.",
            why: "A [[UART]] has to guess where each bit is from an agreed speed. [[SPI]] removes the guessing by adding a wire \
                that says so: the [[clock]]. The controller produces the clock pulses, and both sides use its edges - one \
                edge to put the next bit on the data wire, the other edge to read the bit that the other side has put there. \
                Speed no longer needs agreeing on; the peripheral simply follows the clock, as fast or slow as it comes.\n\n\
                Data flows in both directions at once. Inside each chip is a shift register, and the two are joined in a \
                ring: with every clock pulse the controller pushes one bit out on MOSI and takes one bit in from MISO. After \
                eight pulses the two chips have exchanged a byte. Even a pure read is an exchange: the controller sends a \
                dummy byte to get the clock running.\n\n\
                Several chips can share the clock and data wires. Each gets its own chip-select wire, and only the one \
                whose select is low takes part.\n\n\
                The one thing the two sides must agree on is how to read the clock, and SPI has no standard for it. Two \
                choices give four modes: whether the clock rests low or high between bytes (called CPOL), and whether data \
                is read on the first or the second edge of each pulse (CPHA). Every chip's datasheet says which mode it \
                wants. If the controller uses another one, one side reads on the very edge on which the other changes the \
                data - and what it gets is shifted by a bit, or depends on nanoseconds.",
            formulas: &[
                ("mode = 2 x CPOL + CPHA", "Mode 0: clock rests low, read on the rising edge. Mode 3: clock rests high, read on the rising edge. Modes 1 and 2 read on the falling edge."),
                ("time for one byte = 8 / clock frequency", "At a 1 MHz clock a byte takes 8 µs: a megabit per second in each direction, with no start or stop bits."),
            ],
            try_this: &[
                Experiment {
                    action: "Set both sides to mode 3.",
                    watch: "the clock now resting high, with everything else in step: the dots still sit in the middle of each bit. Any mode works, as long as it is the same on both sides.",
                    preset: Some("mode3"),
                },
                Experiment {
                    action: "Leave the controller in mode 0 and set the peripheral to mode 1.",
                    watch: "the peripheral's bits changing exactly at the controller's dots. The controller reads 0x9E instead of 0x3C: every bit one place late.",
                    preset: Some("mismatch"),
                },
                Experiment {
                    action: "Now set the peripheral to mode 2.",
                    watch: "the nastiest case: the bytes come out right in this ideal picture, yet the dots are red, because each side reads at the instant the other changes. On a real board this works on the bench and fails in the field.",
                    preset: Some("sneaky"),
                },
            ],
            leaves_out: "Bits are sent most significant first here, as nearly all chips do, but that too is a per-chip choice. \
                Reading \"on an edge\" is drawn as reading just before it; real chips need the data to be steady for some \
                nanoseconds before and after, which limits the clock speed and the cable length. Only one peripheral is \
                shown. Most chips expect more than one byte per transfer - a command, then data - while chip select stays \
                low. Variants with two or four data wires in one direction exist for fast memory chips.",
            refs: &["Serial Peripheral Interface as introduced by Motorola (e.g. \"SPI Block Guide\", S12SPIV4): clock polarity CPOL and clock phase CPHA define the four modes; data is shifted most significant bit first."],
        },
    }
}
