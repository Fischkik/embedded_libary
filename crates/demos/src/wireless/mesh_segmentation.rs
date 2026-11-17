use crate::prelude::*;
use sim_core::mesh::{MAX_SEGMENTS, MAX_UNSEGMENTED, SEGMENT_OCTETS, parse_segment_header, sar_rounds, segment};
use sim_core::units::hex;
use std::collections::BTreeSet;

pub struct MeshSegmentation {
    /// Length of the Upper Transport Access PDU (encrypted message + 4-octet MIC).
    length: usize,
    /// Segments lost in the first transmission round.
    lost: BTreeSet<usize>,
}

impl Default for MeshSegmentation {
    fn default() -> Self {
        Self { length: 30, lost: [1].into_iter().collect() }
    }
}

impl Demo for MeshSegmentation {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "A message too long for one packet", |ui| {
            ui.add(egui::Slider::new(&mut self.length, 1..=MAX_SEGMENTS * SEGMENT_OCTETS + 20).text("message length (encrypted, with its 4-octet check)").suffix(" octets"))
                .on_hover_text("The upper transport PDU: the encrypted access message plus its 4-octet integrity check. A sensor reading is a few octets; a firmware chunk or a long name is more.");
        });
        let upper: Vec<u8> = (0..self.length).map(|i| (i as u8).wrapping_mul(37).wrapping_add(0x11)).collect();
        let seq = 0x3129AB;
        let Some(segs) = segment(&upper, true, 0x26, false, seq) else {
            widgets::status(ui, false, format!("{} octets would need more than {MAX_SEGMENTS} segments: the largest segmented message is {} octets", self.length, MAX_SEGMENTS * SEGMENT_OCTETS));
            return;
        };
        let n = segs.len();
        self.lost.retain(|&i| i < n);
        let unsegmented = self.length <= MAX_UNSEGMENTED;

        if unsegmented {
            widgets::status(ui, true, format!("{} octets fit into one unsegmented message (up to {MAX_UNSEGMENTED}): no segmentation, no acknowledgment", self.length));
        } else {
            ui.label(RichText::new(format!("The message is cut into {n} segments. Click a segment to lose it on the first try")).strong());
            for s in &segs {
                let (seq_zero, seg_o, seg_n) = parse_segment_header(&s.header);
                let lost = self.lost.contains(&(s.seg_o as usize));
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if ui.selectable_label(lost, if lost { "lost on the way" } else { "arrives" }).on_hover_text("Click to make this segment get lost the first time it is sent.").clicked() {
                            if !self.lost.remove(&(s.seg_o as usize)) {
                                self.lost.insert(s.seg_o as usize);
                            }
                        }
                        ui.label(RichText::new(format!("segment {} of {}", seg_o, seg_n)).strong().color(if lost { pal.bad } else { pal.text }));
                        ui.label(RichText::new(format!("SeqZero 0x{seq_zero:04X}  SegO {seg_o}  SegN {seg_n}")).small().color(pal.dim));
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(hex(&s.header)).monospace().color(pal.trace[1]));
                        ui.label(RichText::new(hex(&s.payload)).monospace().color(pal.trace[0]));
                    });
                });
            }
            widgets::key(ui, &[(pal.trace[1], "4-octet segment header"), (pal.trace[0], "up to 12 octets of the message")]);

            ui.label(RichText::new("Sending, acknowledging, sending again").strong());
            let loss_plan = vec![self.lost.iter().copied().collect::<Vec<_>>()];
            let rounds = sar_rounds(n, (seq & 0x1FFF) as u16, &loss_plan, 4);
            for (r, round) in rounds.iter().enumerate() {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(format!("round {}", r + 1)).strong());
                        ui.label(format!("sender sends segment(s) {:?}", round.sent));
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.label("receiver has:");
                        for (i, got) in round.received_after.iter().enumerate() {
                            let (rect, _) = ui.allocate_exact_size(vec2(18.0, 18.0), egui::Sense::hover());
                            ui.painter().rect_filled(rect, 2.0, if *got { pal.good } else { pal.bad.gamma_multiply(0.5) });
                            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, format!("{i}"), egui::FontId::monospace(10.0), ui.visuals().strong_text_color());
                        }
                        let acked = u32::from_be_bytes([round.ack[3], round.ack[4], round.ack[5], round.ack[6]]);
                        ui.label(format!("and answers with a Segment Acknowledgment: {}   AckedSegments = 0b{:0n$b}", hex(&round.ack), acked, n = n));
                    });
                });
            }
            let done = rounds.last().is_some_and(|r| r.received_after.iter().all(|g| *g));
            widgets::status(
                ui,
                done,
                if done { format!("complete after {} round(s): {} segment transmissions for {n} segments", rounds.len(), rounds.iter().map(|r| r.sent.len()).sum::<usize>()) } else { "not complete".to_owned() },
            );
        }
        widgets::formula(
            ui,
            format!(
                "segments = message length / 12, rounded up = {} / 12 -> {}   (15 octets or less: 1 packet)\n\
                 segment header: SEG 1 | AKF 1 | AID 6 | SZMIC 1 | SeqZero 13 | SegO 5 | SegN 5 bits\n\
                 largest message = 32 segments x 12 octets = 384 octets",
                self.length,
                if unsegmented { 1 } else { n },
            ),
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "short" => self.length = 12,
            "none-lost" => self.lost.clear(),
            "many" => {
                self.length = 120;
                self.lost = [0, 3, 4, 7].into_iter().collect();
            }
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "mesh-segmentation",
        title: "Segmented messages",
        blurb: "A mesh packet carries at most 15 octets of message. Longer ones travel in numbered pieces, and the receiver says which pieces are missing.",
        path: &["Wireless", "Bluetooth", "Bluetooth Mesh"],
        make: || Box::new(MeshSegmentation::default()),
        explain: Explanation {
            what: "A message of 30 octets is to be sent across a [[Bluetooth Mesh]] network. The boxes at the top are the pieces \
                it is cut into: each has a 4-octet header (orange) and up to 12 octets of the message (blue). The boxes below \
                are the rounds of sending: what the sender sends, which pieces the receiver then has, and the \
                acknowledgment it sends back.\n\n\
                Click a piece at the top to lose it on the first try, and watch the rounds change.",
            why: "Mesh messages travel inside Bluetooth advertising packets, and those are small. After the network header and \
                its checks, room is left for at most 15 octets of the actual (encrypted) message. Most messages are shorter \
                - \"light on\", \"21.5 degrees\" - but some are not: a list of settings, a long name, a chunk of a firmware \
                update.\n\n\
                Such a message is cut into [[segment]]s of 12 octets, the last possibly shorter, and each segment travels as a \
                packet of its own with a small header. The header carries three numbers: SeqZero, which ties all segments \
                of one message together (it is taken from the sequence number of the first segment); SegO, this segment's \
                position; and SegN, the number of the last segment. With these, the receiver can put the pieces back in \
                order, whatever order they arrive in, and knows when it has them all.\n\n\
                Radio packets get lost, and a message with one piece missing is useless. So for a message to a single \
                device, the receiver answers with a Segment Acknowledgment: a 32-bit field with one bit per segment, set \
                for each piece it has. The sender looks at the bits and sends only the missing pieces again, and repeats \
                this until the bits are all set or it gives up.\n\n\
                The cost is real: every segment is a separate trip through the network, plus the acknowledgments. Mesh \
                designs therefore try hard to keep messages within one packet.",
            formulas: &[
                ("segments = message length / 12, rounded up (only if the length is over 15)", "30 octets: 12 + 12 + 6, three segments, SegN = 2."),
                ("AckedSegments: bit n = 1 if segment n has arrived", "Segments 0 and 2 arrived, 1 lost: 0b101. The sender resends segment 1 only."),
            ],
            try_this: &[
                Experiment { action: "Shorten the message to 12 octets.", watch: "the segments disappearing: up to 15 octets travel in one ordinary packet, without any of this machinery.", preset: Some("short") },
                Experiment {
                    action: "Lose nothing.",
                    watch: "a single round, and an acknowledgment with every bit set. Even then the acknowledgment costs a trip back through the network.",
                    preset: Some("none-lost"),
                },
                Experiment {
                    action: "Send 120 octets and lose four of the ten segments.",
                    watch: "the second round carrying exactly the four missing ones. Each acknowledgment tells the sender precisely what to repeat, so nothing that arrived is sent twice.",
                    preset: Some("many"),
                },
            ],
            leaves_out: "Losses happen only in the first round here. Real senders wait for the acknowledgment with timers, give up after \
                a number of retries, and do not wait for acknowledgments at all when sending to a group (there could be many \
                receivers); then they simply send every segment several times. The message bytes shown are made up; the \
                segment headers are computed exactly as the specification defines them. Control messages (from the mesh \
                itself, not the application) are segmented the same way, with 8-octet segments.",
            refs: &[
                "Bluetooth Mesh Protocol 1.1, 3.5.2.2 (Segmented Access message: SEG, AKF, AID, SZMIC, SeqZero 13 bits, SegO, SegN 5 bits; segments of 12 octets), 3.5.2.3.1 (Segment Acknowledgment, AckedSegments), 3.5.3 (segmentation and reassembly).",
                "Checked bit for bit against the specification's sample message #6 (section 8.3.6, as reproduced in BlueZ unit/test-mesh-crypto.c): both segments match.",
            ],
        },
    }
}
