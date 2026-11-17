use crate::prelude::*;
use bits::FieldSpan;
use sim_core::firmware::{CRC8_SMBUS, CRC16_IBM_3740, CRC32_ISO_HDLC, crc, sum8, xor8};
use std::collections::BTreeSet;

pub struct Crc {
    text: String,
    sent: Vec<u8>,
    /// Bit positions (0 = first bit shown) damaged on the way.
    flipped: BTreeSet<usize>,
    swapped: bool,
}

impl Crc {
    fn with(bytes: &[u8], flipped: &[usize], swapped: bool) -> Self {
        Self { text: sim_core::units::hex(bytes), sent: bytes.to_vec(), flipped: flipped.iter().copied().collect(), swapped }
    }

    fn received(&self) -> Vec<u8> {
        let mut r = self.sent.clone();
        for &i in &self.flipped {
            if let Some(byte) = r.get_mut(i / 8) {
                *byte ^= 0x80 >> (i % 8);
            }
        }
        if self.swapped && r.len() >= 2 {
            r.swap(0, 1);
        }
        r
    }
}

impl Default for Crc {
    fn default() -> Self {
        Self::with(&[0x10, 0x21, 0x30, 0x40], &[13], false)
    }
}

impl Demo for Crc {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "The message that is sent", |ui| {
            bits::hex_edit(ui, "bytes (hex)", &mut self.text, &mut self.sent, 8);
        });
        widgets::group(ui, "What arrives - click a bit to damage it on the way", |ui| {
            let received = self.received();
            let rx_bits: Vec<bool> = received.iter().flat_map(|b| (0..8).map(move |i| b & (0x80 >> i) != 0)).collect();
            let fields: Vec<FieldSpan> = (0..received.len()).map(|i| FieldSpan::new(&format!("byte {i}"), 8, "One byte of the message, most significant bit first.")).collect();
            let strip = bits::bit_strip(ui, &rx_bits, &fields, None);
            if let Some(i) = strip.clicked {
                // With the swap active the first two bytes are shown exchanged.
                let i = if self.swapped && i < 16 { (i + 8) % 16 } else { i };
                if !self.flipped.remove(&i) {
                    self.flipped.insert(i);
                }
            }
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut self.swapped, "the first two bytes arrive in the wrong order").on_hover_text("A typical software mistake rather than line noise: two bytes exchanged.");
                if ui.button("Undo all damage").on_hover_text("The message arrives exactly as it was sent.").clicked() {
                    self.flipped.clear();
                    self.swapped = false;
                }
            });
            ui.monospace(format!("sent     {}\nreceived {}", sim_core::units::hex(&self.sent), sim_core::units::hex(&received)));
        });

        let received = self.received();
        let damaged = received != self.sent;
        let checks: [(&str, &str, u32, u32, usize); 5] = [
            ("Sum of all bytes", "8 bits", sum8(&self.sent) as u32, sum8(&received) as u32, 2),
            ("XOR of all bytes", "8 bits", xor8(&self.sent) as u32, xor8(&received) as u32, 2),
            (CRC8_SMBUS.name, "8 bits", crc(&CRC8_SMBUS, &self.sent), crc(&CRC8_SMBUS, &received), 2),
            (CRC16_IBM_3740.name, "16 bits", crc(&CRC16_IBM_3740, &self.sent), crc(&CRC16_IBM_3740, &received), 4),
            (CRC32_ISO_HDLC.name, "32 bits", crc(&CRC32_ISO_HDLC, &self.sent), crc(&CRC32_ISO_HDLC, &received), 8),
        ];
        let mut missed = Vec::new();
        widgets::group(ui, "Five ways to check: the sender appends the value, the receiver recalculates it", |ui| {
            egui::Grid::new("checks").striped(true).num_columns(5).show(ui, |ui| {
                for h in ["check", "size", "sent along", "recalculated", "verdict"] {
                    ui.label(RichText::new(h).strong());
                }
                ui.end_row();
                for (name, size, tx, rx, digits) in checks {
                    ui.add(egui::Label::new(name).extend());
                    ui.label(size);
                    ui.monospace(format!("0x{tx:0digits$X}"));
                    ui.monospace(format!("0x{rx:0digits$X}"));
                    let (text, color) = match (damaged, tx == rx) {
                        (false, _) => ("matches: message accepted", pal.good),
                        (true, false) => ("differs: damage detected", pal.good),
                        (true, true) => ("MATCHES: damage not noticed", pal.bad),
                    };
                    if damaged && tx == rx {
                        missed.push(name);
                    }
                    ui.add(egui::Label::new(RichText::new(text).color(color).strong()).extend());
                    ui.end_row();
                }
            });
        });
        widgets::status(
            ui,
            missed.is_empty(),
            if !damaged {
                "the message arrived intact, and every check agrees".to_owned()
            } else if missed.is_empty() {
                "the message was damaged, and every check noticed".to_owned()
            } else {
                format!("the message was damaged, but would be accepted as good by: {}", missed.join(", "))
            },
        );
        widgets::formula(
            ui,
            format!(
                "chance that random damage slips through an n-bit check = 1 in 2^n\n\
                 \x20 8 bits: 1 in 256        16 bits: 1 in 65 536        32 bits: 1 in 4 294 967 296\n\n\
                 {}: polynomial 0x{:04X}, start value 0x{:04X}\n\
                 its published test value, the CRC of the text \"123456789\": 0x{:04X}",
                CRC16_IBM_3740.name,
                CRC16_IBM_3740.poly,
                CRC16_IBM_3740.init,
                crc(&CRC16_IBM_3740, b"123456789"),
            ),
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = match preset {
            "swap" => Self::with(&[0x10, 0x21, 0x30, 0x40], &[], true),
            "cancel" => Self::with(&[0x10, 0x21, 0x30, 0x40], &[7, 15], false),
            "burst" => Self::with(&[0x10, 0x21, 0x30, 0x40], &[10, 11, 12, 13, 14, 15, 16, 17], false),
            _ => Self::default(),
        };
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "firmware-crc",
        title: "Checksums and CRCs",
        blurb: "How a receiver knows that a message arrived undamaged - and which kinds of damage a simple checksum cannot see.",
        path: &["Firmware craft"],
        make: || Box::new(Crc::default()),
        explain: Explanation {
            what: "At the top is a short message of four [[byte]]s. Below it is the same message as it arrives, bit by bit. \
                Click any bit to damage it on the way; in the opening state one bit has already been flipped.\n\n\
                The table compares five ways of checking. \"Sent along\" is the value the sender calculated from the \
                original message. \"Recalculated\" is what the receiver gets from the message that actually arrived. If the \
                two differ, the receiver knows the message is damaged.",
            why: "Every way of moving data makes mistakes now and then: a radio packet hit by interference, a spike on a \
                cable, a memory cell that has lost its charge. The receiver cannot see that a bit is wrong; a wrong bit \
                looks exactly like a right one. So the sender adds a few extra bits, calculated from the message, and the \
                receiver repeats the calculation. Damage is discovered when the results disagree.\n\n\
                The simplest [[checksum]] adds up all the bytes. It catches any single wrong bit, but it is blind to \
                anything that leaves the total unchanged: two bytes in the wrong order, or one byte too high and another \
                too low by the same amount. XOR-ing all bytes together has the same kind of weakness.\n\n\
                A [[CRC]], cyclic redundancy check, is built differently. The message is treated as one very long number \
                and divided by a fixed constant, the polynomial; the remainder of that division is the check value. A \
                remainder depends on where each bit is, not just on how many there are, so reordering or compensating \
                changes do alter it. A CRC of n bits is guaranteed to catch any burst of damage up to n bits long, and \
                lets other damage through only about once in 2 to the power n cases.\n\n\
                In hardware a CRC is nothing but a shift register and a few XOR gates, which is why it is everywhere: 16 \
                bits protect small frames on serial buses, 24 bits every Bluetooth LE packet, 32 bits every Ethernet frame \
                and many firmware images.\n\n\
                A CRC protects against accidents, not against people. Anyone who changes a message on purpose can simply \
                recalculate the CRC. Protection against tampering needs a secret key, as in the mesh packet demo.",
            formulas: &[
                ("check value = remainder of (message as one long number) divided by (polynomial)", "The division is done bit by bit with XOR instead of subtraction."),
                ("chance that random damage goes unnoticed = 1 in 2^n", "An 8-bit check misses 1 damaged message in 256, a 16-bit CRC 1 in 65 536."),
            ],
            try_this: &[
                Experiment {
                    action: "Let the first two bytes arrive in the wrong order.",
                    watch: "the sum and the XOR matching although the message is wrong: adding does not care about order. All three CRCs notice.",
                    preset: Some("swap"),
                },
                Experiment {
                    action: "Flip the last bit of the first byte and the last bit of the second byte.",
                    watch: "one byte going up by one and the other down by one. The total is unchanged, so the sum is fooled again, and so is the XOR.",
                    preset: Some("cancel"),
                },
                Experiment {
                    action: "Damage eight bits in a row, as a burst of interference would.",
                    watch: "every CRC catching it. Then keep clicking bits and try to find damage that the 8-bit CRC misses but the 16-bit one catches: it exists, but it is hard to hit by hand.",
                    preset: Some("burst"),
                },
            ],
            leaves_out: "The check value itself can also be damaged in transit; the receiver then rejects a good message, which \
                is harmless. Three CRCs out of dozens in use are shown; they differ in polynomial, start value and bit \
                order, and both ends must use exactly the same one. Real code calculates a CRC a byte at a time with a \
                table, or lets the microcontroller's CRC hardware do it. Some systems go further and add enough extra bits \
                to repair damage, not just detect it (error-correcting codes).",
            refs: &[
                "G. Cook, \"Catalogue of parametrised CRC algorithms\" (reveng.sourceforge.io/crc-catalogue): parameters and check values of CRC-8/SMBUS (0xF4), CRC-16/IBM-3740 (0x29B1), CRC-32/ISO-HDLC (0xCBF43926).",
                "W. W. Peterson, D. T. Brown, \"Cyclic Codes for Error Detection\", Proceedings of the IRE, 1961: an n-bit CRC detects all bursts of length n or less.",
            ],
        },
    }
}
