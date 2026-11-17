use crate::prelude::*;
use egui_plot::{PlotPoint, Points, Polygon, Text};
use sim_core::bus::can_arbitrate;

const NAMES: [&str; 3] = ["Engine", "Brakes", "Radio"];

pub struct Can {
    ids: [u16; 3],
}

impl Default for Can {
    fn default() -> Self {
        Self { ids: [0x123, 0x0A5, 0x7C0] }
    }
}

impl Demo for Can {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "Three nodes start a message at the same instant", |ui| {
            ui.horizontal_wrapped(|ui| {
                for (i, id) in self.ids.iter_mut().enumerate() {
                    widgets::wrap(ui, NAMES[i], |ui| {
                        ui.add(egui::Slider::new(id, 0..=0x7FF).text(format!("{} sends identifier", NAMES[i])).hexadecimal(3, false, true))
                            .on_hover_text("The 11-bit identifier of the message this node wants to send. It names the content of the message and is also its priority: lower wins.")
                    });
                }
            });
        });
        let a = can_arbitrate(&self.ids);

        ui.label(RichText::new("The first twelve bits: every node sends and listens at the same time").strong());
        let mut names: Vec<String> = vec!["the bus".to_owned()];
        names.extend((0..3).rev().map(|i| format!("{} (0x{:03X})", NAMES[i], self.ids[i])));
        plots::timeline("bits", names, "bit number", 12.0).show(ui, |p| {
            let mut wave = |row: f64, levels: &[Option<bool>], color: Color32| {
                for (k, level) in levels.iter().enumerate() {
                    let x = k as f64;
                    match level {
                        Some(l) => {
                            let y = row - 0.28 + if *l { 0.56 } else { 0.0 };
                            p.line(plots::line("level", vec![[x, y], [x + 1.0, y]], color).width(2.5));
                            p.text(Text::new("bit", PlotPoint::new(x + 0.5, row), if *l { "1" } else { "0" }).color(pal.text));
                        }
                        None => {
                            let fill = pal.dim.gamma_multiply(0.25);
                            p.polygon(Polygon::new("listening", plots::bar(x, x + 1.0, row - 0.28, row + 0.28)).fill_color(fill).stroke((0.0, fill)));
                        }
                    }
                }
            };
            for i in 0..3 {
                wave(3.0 - i as f64, &a.sent[i], pal.trace[(i + 1) % 6]);
            }
            let bus: Vec<Option<bool>> = a.bus.iter().map(|b| Some(*b)).collect();
            wave(0.0, &bus, pal.text);
            let lost: Vec<[f64; 2]> = a.lost_at.iter().enumerate().filter_map(|(i, l)| l.map(|k| [k as f64 + 0.5, 3.0 - i as f64 + 0.4])).collect();
            p.points(Points::new("loses here", lost).shape(egui_plot::MarkerShape::Cross).radius(7.0).color(pal.bad));
            p.text(Text::new("label", PlotPoint::new(0.5, 3.6), "start").color(pal.text));
            p.text(Text::new("label", PlotPoint::new(6.5, 3.6), "identifier, most significant bit first").color(pal.text));
        });
        widgets::key(ui, &[(pal.text, "high line: recessive (1), low line: dominant (0)"), (pal.bad, "cross: sent a 1, saw a 0 on the bus, and stops"), (pal.dim.gamma_multiply(0.4), "grey: lost, now only listening")]);

        let winner_id = a.bus[1..].iter().fold(0u16, |v, &b| v << 1 | b as u16);
        widgets::status(
            ui,
            a.winners.len() == 1,
            match a.winners.as_slice() {
                [w] => format!("{} wins with identifier 0x{:03X} and carries on with its message, undisturbed. The others try again afterwards", NAMES[*w], self.ids[*w]),
                _ => "two nodes sent the same identifier: neither notices the other, both carry on, and the message is wrecked a few bits later. Identifiers must be unique".to_owned(),
            },
        );
        let order: Vec<String> = {
            let mut v: Vec<(u16, &str)> = self.ids.iter().copied().zip(NAMES).collect();
            v.sort();
            v.iter().map(|(id, n)| format!("{n} 0x{id:03X}")).collect()
        };
        widgets::formula(
            ui,
            format!(
                "bus level = 0 if anyone sends 0, otherwise 1        (a 0 always overwrites a 1)\n\
                 what ended up on the bus: 0x{winner_id:03X} = the lowest identifier\n\
                 order of priority: {}",
                order.join("  <  ")
            ),
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "close" => self.ids = [0x124, 0x125, 0x7C0],
            "same" => self.ids = [0x123, 0x123, 0x7C0],
            "urgent" => self.ids[2] = 0x001,
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "bus-can",
        title: "CAN: who may speak?",
        blurb: "Three devices start talking on the same wire at the same moment - and nothing is lost. The most important message simply wins.",
        path: &["Wired buses"],
        make: || Box::new(Can::default()),
        explain: Explanation {
            what: "Three devices on one [[CAN]] bus want to send a message at exactly the same moment. The upper three rows show \
                what each one puts on the bus, bit by bit; the bottom row is what is really on the wire. A high line is a 1, a \
                low line a 0.\n\n\
                Follow the columns from left to right. At some point a device sends a 1 but the bus shows a 0 - marked with \
                a cross. From there on that device is silent.",
            why: "On I2C and SPI one chip is in charge and the others speak only when asked. In a car or a machine that is too \
                slow and too fragile: dozens of control units must be able to speak up by themselves the moment something \
                happens, and none of them may be a single point of failure. So on a CAN bus anyone may start a message \
                whenever the bus is quiet. The question is what happens when two start at once.\n\n\
                The answer lies in the electrics. A CAN node can do two things: actively pull the bus into the state that \
                means 0, or do nothing, which leaves it in the state that means 1. If even one node sends a 0, the bus is \
                0. That is why 0 is called dominant and 1 recessive.\n\n\
                Every message starts with an identifier, 11 bits long, sent highest bit first. And every node listens to \
                the bus while it sends. As long as the bus shows what a node has sent, it carries on. The moment it has \
                sent a 1 but sees a 0, it knows that someone else is sending a message with a lower identifier - and stops \
                at once, before it has disturbed anything. The other node has not even noticed that there was a contest.\n\n\
                So the message with the lowest identifier always gets through, complete and without delay, and the losers \
                simply start again when it is finished. The identifier is therefore two things at once: the name of the \
                message's content, and its priority. Designers hand out the low numbers to what matters most: brakes before \
                radio.",
            formulas: &[
                ("bus = 0 if any node sends 0, otherwise 1", "A logical AND of everything that is sent. 0 is dominant, 1 is recessive."),
                ("the winner is the lowest identifier", "0x0A5 beats 0x123 beats 0x7C0, whoever started first within the same bit."),
            ],
            try_this: &[
                Experiment {
                    action: "Give two nodes identifiers that differ only in the last bit: 0x124 and 0x125.",
                    watch: "the two staying together for eleven bits and parting only at the very last one. Arbitration takes exactly as long as the identifiers are equal.",
                    preset: Some("close"),
                },
                Experiment {
                    action: "Give the radio identifier 0x001.",
                    watch: "the radio now winning: the engine drops out at the third identifier bit, the brakes at the fourth. The bus does not know what is important; it only compares numbers.",
                    preset: Some("urgent"),
                },
                Experiment {
                    action: "Give two nodes the same identifier.",
                    watch: "no cross for either of them: both believe they have won. Their data bits will collide a moment later. This is why each identifier belongs to exactly one sender.",
                    preset: Some("same"),
                },
            ],
            leaves_out: "Only the start of a frame is shown. After the identifier come a few control bits, up to 8 data bytes, a \
                15-bit CRC, and a slot in which every node that received the frame correctly answers with a dominant bit; a \
                sender that sees no answer knows nobody heard it. To keep the nodes' clocks in step, a bit of the opposite \
                value is inserted after five equal bits in a row. Electrically the bus is a pair of wires, like RS-485, \
                ended with 120 ohm at each end, and at 1 Mbit/s it can be about 40 m long: every node must see every bit \
                before the next one starts. Newer variants (29-bit identifiers, CAN FD) keep the same arbitration.",
            refs: &[
                "Bosch, \"CAN Specification 2.0\" / ISO 11898-1: dominant (0) and recessive (1) bus levels, bitwise arbitration on the identifier, base frame format with 11-bit identifier.",
                "Arbitration example with identifiers 15 and 16 as in Wikipedia, \"CAN bus\", section on data transmission.",
            ],
        },
    }
}
