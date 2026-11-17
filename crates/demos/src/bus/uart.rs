use crate::prelude::*;
use egui_plot::{PlotPoint, Points, Polygon, Text, VLine};
use sim_core::bus::{Parity, UartBit, uart_frame, uart_receive};

pub struct Uart {
    byte: u8,
    parity: Parity,
    clock_error_pct: f64,
}

impl Default for Uart {
    fn default() -> Self {
        Self { byte: b'A', parity: Parity::None, clock_error_pct: 0.0 }
    }
}

fn show(b: u8) -> String {
    if b.is_ascii_graphic() { format!("0x{b:02X} '{}'", b as char) } else { format!("0x{b:02X}") }
}

impl Demo for Uart {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "Sender and receiver", |ui| {
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "byte", |ui| {
                    ui.add(egui::Slider::new(&mut self.byte, 0..=255).text("byte to send").custom_formatter(|v, _| show(v as u8))).on_hover_text("The byte the sender puts on the line. 0x41 is the letter A.")
                });
                ui.label("Parity bit:");
                for (p, name) in [(Parity::None, "none"), (Parity::Even, "even"), (Parity::Odd, "odd")] {
                    ui.selectable_value(&mut self.parity, p, name).on_hover_text("An optional extra bit that makes the number of ones even (or odd), so that the receiver can notice a single wrong bit.");
                }
            });
            ui.add(egui::Slider::new(&mut self.clock_error_pct, -10.0..=10.0).text("the receiver's clock is off by").suffix(" %").fixed_decimals(1))
                .on_hover_text("Sender and receiver each measure the length of a bit with their own clock. Positive: the receiver's bits are too long, so it looks later and later.");
        });
        let frame = uart_frame(self.byte, self.parity);
        let rx = uart_receive(&frame, self.clock_error_pct / 100.0);
        let n = frame.len() as f64;
        // One idle bit before the frame and two after it.
        let (x0, span) = (1.0, n + 3.0);

        ui.label(RichText::new("One byte on the line, and where the receiver looks").strong());
        plots::timeline("frame", vec!["receiver looks here".to_owned(), "the line".to_owned()], "time [bit periods of the sender]", span).show(ui, |p| {
            let mut edges = Vec::new();
            for (k, &(role, level)) in frame.iter().enumerate() {
                let (fill, label) = match role {
                    UartBit::Start => (pal.warn, "start".to_owned()),
                    UartBit::Data(i) => (pal.trace[0], format!("bit {i}")),
                    UartBit::Parity => (pal.trace[4], "parity".to_owned()),
                    UartBit::Stop => (pal.trace[2], "stop".to_owned()),
                };
                let x = x0 + k as f64;
                let band = fill.gamma_multiply(0.25);
                p.polygon(Polygon::new("bit", plots::bar(x, x + 1.0, 0.62, 1.38)).fill_color(band).stroke((0.5, pal.canvas)));
                p.text(Text::new("role", PlotPoint::new(x + 0.5, 1.5), label).color(pal.text));
                edges.push((x, level));
            }
            edges.push((x0 + n, true));
            p.line(plots::line("line", plots::digital(&edges, true, 0.0, span, 0.72, 0.56), pal.text).width(2.0));
            for (k, &(t, seen)) in rx.samples.iter().enumerate() {
                let right = frame.get(t.floor() as usize).is_some_and(|_| t.floor() as usize == k);
                let color = if right { pal.good } else { pal.bad };
                p.vline(VLine::new("sample", x0 + t).color(color.gamma_multiply(0.6)));
                p.points(Points::new("sample", vec![[x0 + t, 0.0]]).radius(5.0).color(color));
                p.text(Text::new("seen", PlotPoint::new(x0 + t, -0.3), if seen { "1" } else { "0" }).color(color));
            }
        });
        widgets::key(ui, &[(pal.warn, "start bit: always low"), (pal.trace[0], "data bits, least significant first"), (pal.trace[4], "parity bit"), (pal.trace[2], "stop bit: always high"), (pal.good, "receiver looks inside the right bit"), (pal.bad, "receiver looks inside a neighbouring bit")]);

        let ok = rx.byte == self.byte && rx.stop_ok && rx.parity_ok != Some(false);
        widgets::status(
            ui,
            ok,
            if ok {
                format!("sent {}, received {}", show(self.byte), show(rx.byte))
            } else {
                format!(
                    "sent {}, received {}{}{}",
                    show(self.byte),
                    show(rx.byte),
                    if rx.parity_ok == Some(false) { " - the parity bit does not fit, so the receiver knows something is wrong" } else if rx.byte != self.byte { " - and nothing tells the receiver that it is wrong" } else { "" },
                    if rx.stop_ok { "" } else { "; the stop bit was read as low (a framing error)" },
                )
            },
        );
        let last = (n - 0.5) * (1.0 + self.clock_error_pct / 100.0);
        widgets::formula(
            ui,
            format!(
                "the receiver looks at bit k at   (k + 0.5) x (1 + error)   bit periods after the start edge\n\
                 last bit (the stop bit)        : ({n:.0} - 0.5) x (1 {} {:.1} %) = {last:.2}        it lies between {:.0} and {n:.0}\n\
                 largest error that still works : 0.5 / ({n:.0} - 0.5) = {:.1} %",
                if self.clock_error_pct < 0.0 { "-" } else { "+" },
                self.clock_error_pct.abs(),
                n - 1.0,
                50.0 / (n - 0.5),
            ),
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "drift" => self.clock_error_pct = 7.0,
            "parity" => (self.parity, self.clock_error_pct) = (Parity::Odd, 7.0),
            "bits" => self.byte = 0x55,
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "bus-uart",
        title: "UART: bytes on a wire",
        blurb: "One wire, no clock: how a byte is sent as a row of bits, and why both ends must agree on the speed.",
        path: &["Wired buses"],
        make: || Box::new(Uart::default()),
        explain: Explanation {
            what: "The upper row is the voltage on one wire while a single [[byte]] is sent. The coloured bands mark what each \
                stretch of time means: a start bit, the eight data bits, and a stop bit. The lower row shows the receiver: \
                each dot is a moment at which it looks at the wire, with the value it sees written underneath.\n\n\
                In the opening state the dots sit in the middle of each band, and the receiver reads the letter A.",
            why: "The simplest way to send data between two chips is one wire per direction and an agreement about time. This \
                is what a [[UART]] does, and it is the serial port behind every debug console.\n\n\
                While nothing is sent, the line rests high. To send a byte, the sender pulls the line low for exactly one \
                bit period: the start bit. Then it puts the eight bits of the byte on the line one after the other, each for \
                one bit period, the least significant first. Finally it lets the line go high for at least one bit period: \
                the stop bit. Ten bits on the wire for eight bits of data.\n\n\
                There is no clock wire. The receiver finds its bearings from the falling edge of the start bit alone. From \
                that instant it counts time with its own clock and looks at the line in the middle of where each bit \
                should be. That only works if both sides were told the same speed beforehand - the baud rate, in bits per \
                second - and if their clocks really are close to each other.\n\n\
                Small differences are forgiven, because the receiver starts afresh with every start bit. But within one \
                byte the error adds up: by the tenth bit a clock that is 5 % off has drifted by half a bit, and the \
                receiver is looking at the neighbour. The result is not an error message. It is simply a different byte.\n\n\
                To catch at least some such mistakes, a parity bit can be added. And if the stop bit turns out low, the \
                receiver reports a framing error: a hint that the speeds do not match.",
            formulas: &[
                ("bits on the wire per byte = 1 start + 8 data (+ 1 parity) + 1 stop", "10 bits per byte without parity: at 115 200 bit/s that is 11 520 bytes per second."),
                ("largest clock difference = 0.5 bit / 9.5 bits = 5.3 %", "Counted for both ends together; each end should stay within about 2 %."),
            ],
            try_this: &[
                Experiment {
                    action: "Send 0x55 instead of the letter A.",
                    watch: "the line alternating with every bit: 0x55 is 0101 0101. Engineers send it to measure a line's speed, because every bit is one clean pulse.",
                    preset: Some("bits"),
                },
                Experiment {
                    action: "Make the receiver's clock 7 % slow.",
                    watch: "the dots sliding to the right until the last data bit is read inside the stop bit. The letter A arrives as a different byte, and the stop bit still looks fine.",
                    preset: Some("drift"),
                },
                Experiment {
                    action: "The same 7 %, with an odd parity bit.",
                    watch: "the status line: the byte is wrong again, but now the parity bit does not fit and the receiver can at least throw the byte away.",
                    preset: Some("parity"),
                },
            ],
            leaves_out: "Real receivers sample each bit several times and take a majority, and their clocks do not only run fast \
                or slow but also jitter. The voltages are left out: between chips the levels are those of the supply (3.3 V \
                or 5 V); the old RS-232 port uses plus and minus several volts, inverted; RS-485 uses a pair of wires. Other \
                formats exist (7 or 9 data bits, two stop bits). A parity bit catches one wrong bit, not two. And a UART has \
                no addresses and no notion of a message: where a message begins and ends is up to the software.",
            refs: &["Asynchronous serial communication, 8N1 and 8E1 framing, as implemented by every UART (e.g. the 16550 and all microcontroller serial ports): idle high, start bit low, data least significant bit first, mid-bit sampling."],
        },
    }
}
