use crate::prelude::*;
use egui_plot::{PlotPoint, Polygon, Text};
use sim_core::bus::{I2cSlot, i2c_transfer};

pub struct I2c {
    address: u8,
    read: bool,
    data: u8,
    device_address: u8,
}

impl Default for I2c {
    fn default() -> Self {
        Self { address: 0x48, read: false, data: 0xA5, device_address: 0x48 }
    }
}

impl Demo for I2c {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "The controller and one device on the bus", |ui| {
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "address", |ui| ui.add(egui::Slider::new(&mut self.address, 0x08..=0x77).text("the controller calls address").hexadecimal(2, false, true)).on_hover_text("The 7-bit address the controller puts on the bus. Every device compares it with its own."));
                widgets::wrap(ui, "device", |ui| ui.add(egui::Slider::new(&mut self.device_address, 0x08..=0x77).text("the device's own address").hexadecimal(2, false, true)).on_hover_text("The address of the one device connected here, for example a temperature sensor. It is fixed by the chip, sometimes adjustable with a pin or two."));
            });
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut self.read, false, "write a byte to it").on_hover_text("The controller sends the data byte.");
                ui.selectable_value(&mut self.read, true, "read a byte from it").on_hover_text("The device sends the data byte; the controller only supplies the clock.");
                widgets::wrap(ui, "data", |ui| ui.add(egui::Slider::new(&mut self.data, 0..=255).text("data byte").hexadecimal(2, false, true)).on_hover_text("The byte that is written to the device, or that the device returns."));
            });
        });
        let t = i2c_transfer(self.address, self.read, self.data, self.device_address);

        ui.label(RichText::new("One transaction: two wires over time").strong());
        plots::timeline("wires", vec!["SDA: data".to_owned(), "SCL: clock".to_owned()], "time [clock periods]", t.duration).show(ui, |p| {
            for (k, &(slot, level, by_device)) in t.slots.iter().enumerate() {
                let x = 1.0 + k as f64;
                let color = match slot {
                    I2cSlot::Address(_) => pal.trace[0],
                    I2cSlot::ReadWrite => pal.trace[1],
                    I2cSlot::Ack(true) => pal.good,
                    I2cSlot::Ack(false) => pal.bad,
                    I2cSlot::Data(_) => pal.trace[4],
                };
                let band = color.gamma_multiply(if by_device { 0.45 } else { 0.2 });
                p.polygon(Polygon::new("bit", plots::bar(x, x + 1.0, -0.42, 0.42)).fill_color(band).stroke((0.5, pal.canvas)));
                p.text(Text::new("value", PlotPoint::new(x + 0.5, -0.52), if level { "1" } else { "0" }).color(pal.text));
            }
            let label = |p: &mut egui_plot::PlotUi, from: usize, to: usize, text: String| {
                p.text(Text::new("label", PlotPoint::new(1.0 + (from + to) as f64 / 2.0, 0.55), text).color(pal.text));
            };
            p.text(Text::new("label", PlotPoint::new(0.5, 0.55), "START").color(pal.text));
            label(p, 0, 7, format!("address 0x{:02X}", self.address));
            label(p, 7, 8, if self.read { "R".to_owned() } else { "W".to_owned() });
            label(p, 8, 9, if t.address_acked { "ACK".to_owned() } else { "NACK".to_owned() });
            if t.address_acked {
                label(p, 9, 17, format!("data 0x{:02X}", self.data));
                label(p, 17, 18, if self.read { "NACK".to_owned() } else { "ACK".to_owned() });
            }
            p.text(Text::new("label", PlotPoint::new(t.stop_at, 0.55), "STOP").color(pal.text));
            let wave = |edges: &[(f64, bool)], row: f64| plots::digital(edges, true, 0.0, t.duration, row - 0.28, 0.56);
            p.line(plots::line("scl", wave(&t.scl, 1.0), pal.trace[1]).width(2.0));
            p.line(plots::line("sda", wave(&t.sda, 0.0), pal.text).width(2.0));
        });
        widgets::key(
            ui,
            &[
                (pal.trace[0], "address bits"),
                (pal.trace[1], "read/write bit"),
                (pal.good, "acknowledge: the line is pulled low"),
                (pal.bad, "no acknowledge: the line stays high"),
                (pal.trace[4], "data bits"),
                (pal.dim, "stronger colour: the device drives the data line"),
            ],
        );

        widgets::status(
            ui,
            t.address_acked,
            if t.address_acked {
                format!("the device at 0x{:02X} answered, and one byte was {}", self.device_address, if self.read { "read from it" } else { "written to it" })
            } else {
                format!("nobody answered at address 0x{:02X}: the acknowledge bit stayed high, and the controller ends the transaction", self.address)
            },
        );
        let first = self.address << 1 | self.read as u8;
        widgets::formula(
            ui,
            format!(
                "first byte on the wire = address x 2 + read bit = 0x{:02X} x 2 + {} = 0x{first:02X}\n\
                 clock pulses needed    = {}  ({} byte(s) of 8 bits, each followed by 1 acknowledge bit)",
                self.address,
                self.read as u8,
                t.slots.len(),
                t.slots.len() / 9,
            ),
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "nobody" => self.address = 0x49,
            "read" => (self.read, self.data) = (true, 0x3C),
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "bus-i2c",
        title: "I2C: two wires and an address",
        blurb: "Many chips on the same two wires. See how one is called by its address and how it says \"here\".",
        path: &["Wired buses"],
        make: || Box::new(I2c::default()),
        explain: Explanation {
            what: "The two rows are the two wires of an [[I2C]] bus over time: the clock on top, the data line below. The \
                coloured blocks mark what each clock pulse carries, with the bit value underneath. Read it from left to \
                right: a start signal, seven address bits, one bit that says read or write, an acknowledge bit, a data \
                byte, another acknowledge bit, and a stop signal.\n\n\
                Blocks in a stronger colour are the moments when the device, not the controller, is holding the data line.",
            why: "[[SPI]] needs a separate select wire for every chip. A board with ten sensors would need thirteen wires. I2C \
                manages with two, however many chips there are: one clock line and one data line, shared by all.\n\n\
                Sharing a data line only works if nobody can force it high against someone else. So on I2C a chip can only \
                pull a line low or let go of it; a resistor pulls each line back up when everyone lets go. Any one chip can \
                make the line low, and no two chips can ever fight.\n\n\
                Because every chip hears everything, each byte needs a recipient. The controller opens a transaction with \
                a start signal - the data line falls while the clock is high, which never happens inside normal data - \
                and then sends seven bits of address, followed by one bit that says whether it wants to write or read. \
                Every chip on the bus compares the address with its own.\n\n\
                Then comes the idea that makes I2C pleasant to work with. During a ninth clock pulse the controller lets \
                go of the data line, and the addressed chip pulls it low: \"here, I got that\". This is the acknowledge. If \
                the line stays high, no chip with that address exists - or it is not powered, or not soldered properly. \
                The controller learns that immediately, which is why the first thing anyone does with a new board is \
                call every address and see who answers.\n\n\
                After that, data bytes follow, each with its own acknowledge from whoever received it, and a stop signal \
                releases the bus.",
            formulas: &[
                ("first byte = address x 2 + read bit", "Address 0x48, writing: 0x48 x 2 + 0 = 0x90. A logic analyzer shows 0x90 where the datasheet says 0x48."),
                ("clock pulses per byte = 8 bits + 1 acknowledge = 9", "Address and one data byte: 18 pulses. At the standard 100 kHz that takes 180 µs."),
            ],
            try_this: &[
                Experiment {
                    action: "Call address 0x49, where no device lives.",
                    watch: "the acknowledge block turning red with the line left high, and the transaction ending right there: no data byte is sent into the void.",
                    preset: Some("nobody"),
                },
                Experiment {
                    action: "Back at 0x48, read a byte instead of writing one.",
                    watch: "the read/write bit turning to 1 and the data blocks changing to the stronger colour: now the device drives the line and the controller listens. At the end the controller answers \"no acknowledge\", which here means \"that was the last byte I wanted\".",
                    preset: Some("read"),
                },
                Experiment { action: "Change the device's own address until it differs from the one being called.", watch: "the device falling silent. Two chips with the same address on one bus would both answer - which is why the address pins exist.", preset: None },
            ],
            leaves_out: "One device and one data byte; real transactions usually send a register number first and then read or \
                write several bytes, and several controllers can share a bus. The pull-up resistors make the lines rise \
                slowly, which together with the wires' capacitance limits the speed (100 kHz and 400 kHz are the common \
                rates) and the length of the bus to roughly a board or a short cable. A device may hold the clock low to ask \
                for more time. Addresses 0x00 to 0x07 and 0x78 to 0x7F are reserved, and a 10-bit address format exists.",
            refs: &["NXP Semiconductors, \"I2C-bus specification and user manual\", UM10204: START and STOP conditions, 7-bit address followed by the R/W bit (0 = write, 1 = read), acknowledge on the ninth clock pulse, data most significant bit first."],
        },
    }
}
