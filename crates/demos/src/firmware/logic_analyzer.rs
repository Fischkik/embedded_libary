use crate::prelude::*;
use egui_plot::{PlotPoint, Points, Text};
use sim_core::firmware::{sample_wave, shortest_pulse, uart_decode, uart_wave};

const BAUDS: [f64; 5] = [9600.0, 19_200.0, 38_400.0, 57_600.0, 115_200.0];

pub struct LogicAnalyzer {
    text: String,
    baud: f64,
    sample_hz: f64,
    decoder_baud: f64,
}

impl Default for LogicAnalyzer {
    fn default() -> Self {
        Self { text: "Hi!".to_owned(), baud: 9600.0, sample_hz: 100_000.0, decoder_baud: 9600.0 }
    }
}

fn printable(b: u8) -> String {
    if b.is_ascii_graphic() || b == b' ' { format!("'{}'", b as char) } else { format!("0x{b:02X}") }
}

impl Demo for LogicAnalyzer {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "The device under test", |ui| {
            ui.horizontal_wrapped(|ui| {
                let label = ui.label("It sends the text:");
                ui.add(egui::TextEdit::singleline(&mut self.text).desired_width(90.0).char_limit(6)).labelled_by(label.id).on_hover_text("Up to six characters, sent as bytes over a serial line.");
                ui.label("at");
                for baud in [9600.0, 115_200.0] {
                    ui.selectable_value(&mut self.baud, baud, si(baud, "bit/s")).on_hover_text("The speed the device really sends at. Suppose you do not know it.");
                }
            });
        });
        self.text.retain(|c| c.is_ascii_graphic() || c == ' ');
        if self.text.is_empty() {
            self.text.push('?');
        }
        widgets::group(ui, "The logic analyzer", |ui| {
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "rate", |ui| {
                    ui.add(widgets::si_slider(&mut self.sample_hz, 5e3..=2e6, "sample rate", "Hz").logarithmic(true)).on_hover_text("How many times per second the analyzer looks at the wire. It only records high or low at those instants.")
                });
                ui.label("Decode as serial data at:");
                for baud in BAUDS {
                    ui.selectable_value(&mut self.decoder_baud, baud, si(baud, "bit/s")).on_hover_text("The speed you tell the analyzer's decoder to assume. It must match the real one.");
                }
            });
        });

        let (edges, duration) = uart_wave(self.text.as_bytes(), self.baud, 3.0);
        let samples = sample_wave(&edges, self.sample_hz, duration);
        let decoded = uart_decode(&samples, self.sample_hz, self.decoder_baud);
        let ms = 1e3;

        ui.label(RichText::new("Top: the real signal on the wire. Bottom: what the analyzer recorded, with the decoded bytes underneath").strong());
        plots::plot_bare("wave", 230.0).x_axis_label("time [ms]").include_x(0.0).include_x(duration * ms).include_y(-0.6).include_y(3.3).show_axes([true, false]).show(ui, |p| {
            let ms_edges: Vec<(f64, bool)> = edges.iter().map(|&(t, l)| (t * ms, l)).collect();
            p.line(plots::line("the real signal", plots::digital(&ms_edges, true, 0.0, duration * ms, 1.6, 1.0), pal.dim));
            // What the analyzer shows: the level held from one sample to the next.
            let mut seen = Vec::with_capacity(samples.len() * 2);
            for (i, &s) in samples.iter().enumerate() {
                let y = if s { 1.0 } else { 0.0 };
                seen.push([i as f64 / self.sample_hz * ms, y]);
                seen.push([(i + 1) as f64 / self.sample_hz * ms, y]);
            }
            p.line(plots::line("as recorded", seen, pal.trace[0]).width(2.0));
            if samples.len() <= 400 {
                let dots: Vec<[f64; 2]> = samples.iter().enumerate().map(|(i, &s)| [i as f64 / self.sample_hz * ms, if s { 1.0 } else { 0.0 }]).collect();
                p.points(Points::new("sample", dots).radius(2.5).color(pal.trace[1]));
            }
            for d in &decoded {
                let color = if d.stop_ok { pal.good } else { pal.bad };
                p.text(Text::new("decoded byte", PlotPoint::new((d.start_s + 5.0 / self.decoder_baud) * ms, -0.35), printable(d.value)).color(color));
            }
        });
        widgets::key(ui, &[(pal.dim, "the real signal on the wire"), (pal.trace[0], "the recording"), (pal.trace[1], "one sample (shown when there are few)"), (pal.good, "decoded byte"), (pal.bad, "decoded byte with a broken stop bit")]);

        let got: Vec<u8> = decoded.iter().map(|d| d.value).collect();
        let ok = got == self.text.as_bytes() && decoded.iter().all(|d| d.stop_ok);
        let got_text: Vec<String> = got.iter().map(|&b| printable(b)).collect();
        widgets::status(
            ui,
            ok,
            if ok { format!("the analyzer reads exactly what was sent: {}", got_text.join(" ")) } else { format!("the analyzer reads {} - not what was sent (\"{}\")", if got_text.is_empty() { "nothing".to_owned() } else { got_text.join(" ") }, self.text) },
        );
        let per_bit = self.sample_hz / self.baud;
        let pulse = shortest_pulse(&samples);
        widgets::formula(
            ui,
            format!(
                "samples per bit      = sample rate / line speed = {} / {} = {:.1}\n\
                 \x20                      (aim for 10; below about 3 decoding fails)\n\
                 shortest pulse seen  = {}\n\
                 one byte on the wire = 1 start bit + 8 data bits + 1 stop bit = 10 bits = {}",
                si(self.sample_hz, "Hz"),
                si(self.baud, "bit/s"),
                per_bit,
                pulse.map_or("none".to_owned(), |n| format!("{n} sample(s) = {}  ->  line speed about {}", si(n as f64 / self.sample_hz, "s"), si(self.sample_hz / n as f64, "bit/s"))),
                si(10.0 / self.baud, "s"),
            ),
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "undersampled" => self.sample_hz = 12_000.0,
            "wrong-baud" => self.decoder_baud = 19_200.0,
            "fast-line" => (self.baud, self.decoder_baud) = (115_200.0, 115_200.0),
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "firmware-logic-analyzer",
        title: "Debugging with a logic analyzer",
        blurb: "Clip a probe on a wire and read the bytes off it - if you sample fast enough and guess the speed right.",
        path: &["Firmware craft"],
        make: || Box::new(LogicAnalyzer::default()),
        explain: Explanation {
            what: "The upper trace is a serial line while a device sends the text \"Hi!\": the voltage on one wire over time, high \
                or low. The lower trace is what a [[logic analyzer]] connected to that wire recorded. The analyzer then \
                decodes the recording into bytes and writes them underneath.\n\n\
                With the opening settings the recording follows the real signal closely and the decoded text is right.",
            why: "When two chips do not understand each other, reading the code only gets you so far. Sooner or later you have \
                to look at what is really on the wire. The tool for that is a logic analyzer: a small box, often no bigger \
                than a matchbox, with a handful of probes.\n\n\
                It works very simply. Many times per second it notes for each probe whether the wire is high or low - \
                nothing more. From this row of [[sample]]s, software on the PC redraws the signal and, if you tell it which \
                protocol to expect, decodes it into bytes.\n\n\
                Two settings decide whether what you see is the truth. The first is the sample rate. The analyzer knows \
                nothing about what happens between two samples, so each bit on the wire must be hit by several of them. \
                With ten samples per bit the picture is faithful. With two or three, edges land in the wrong place and \
                bits are misread; with fewer than one, whole bits vanish. The treacherous part is that the analyzer still \
                shows a clean, plausible-looking signal - just a wrong one.\n\n\
                The second is the decoder's idea of the line speed. A serial line carries no clock. After the falling edge \
                of the start bit, the receiver simply counts time and looks at the middle of each following bit. A decoder \
                that assumes the wrong speed looks in the wrong places and produces confident nonsense.\n\n\
                If you do not know the speed, the recording tells you: the shortest pulse on the line is one bit long, and \
                one divided by its length is the speed.",
            formulas: &[
                ("samples per bit = sample rate / line speed", "100 kHz sampling of a 9600 bit/s line: 10.4 samples per bit."),
                ("line speed = 1 / shortest pulse", "The shortest pulse is 104 µs long: 1 / 104 µs = 9600 bit/s."),
            ],
            try_this: &[
                Experiment {
                    action: "Lower the sample rate to 12 kHz.",
                    watch: "the recording still looking like a tidy digital signal, but with edges in the wrong places - and the decoded bytes turning into something else. Raise it again slowly and note where the text becomes right.",
                    preset: Some("undersampled"),
                },
                Experiment {
                    action: "Sample fast again, but tell the decoder the line runs at 19 200 bit/s.",
                    watch: "a perfect recording decoded into rubbish: more bytes than were sent, none of them right.",
                    preset: Some("wrong-baud"),
                },
                Experiment {
                    action: "Let the device send at 115 200 bit/s without touching the sample rate.",
                    watch: "the same analyzer that was fine for the slow line now failing: there is not even one sample per bit. Find the sample rate this line needs.",
                    preset: Some("fast-line"),
                },
            ],
            leaves_out: "A real analyzer records several wires at once, starts recording on a trigger (for example the first \
                falling edge), and decodes many protocols besides this one - SPI, I2C, CAN - which do carry a clock and are \
                easier to decode. It sees only high and low: a signal that is too weak, too slow to rise or ringing looks \
                perfect on it, and for those problems you need an oscilloscope. The serial format shown is the common one \
                (eight data bits, no parity bit, one stop bit).",
            refs: &[
                "Asynchronous serial (UART) framing 8N1: idle high, start bit low, eight data bits least significant first, stop bit high; receiver samples at mid-bit.",
                "Sampling a digital signal for protocol decoding: commonly recommended minimum of four samples per bit, ten for comfortable margins (e.g. Saleae logic analyzer documentation).",
            ],
        },
    }
}
