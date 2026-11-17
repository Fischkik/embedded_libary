use crate::prelude::*;
use sim_core::mesh::{self, IV_STATE_MIN_H, IV_UPDATE_MIN_PERIOD_H, Key, NetworkPdu, ReplayList, SEQ_COUNT, iviseq, rx_iv_index, seq_lifetime_s, tx_iv_index};
use sim_core::units::{hex, unhex};

// The keys of the specification's sample data (Mesh Protocol 1.1, 8.2).
const NET_KEY: &str = "7dd7364cd842ad18c17c2b820c84c3d6";
const APP_KEY: &str = "63964771734fbd76e3b40519d1d94a48";
const SWITCH: u16 = 0x1201;
const GROUP: u16 = 0xC001;
/// Zephyr's default CONFIG_BT_MESH_SEQ_STORE_RATE: the sequence number is saved to flash
/// every 128 increments, and a restart continues 128 above the last saved value.
const STORE_RATE: u32 = 128;
/// The switch has been in use for a while when the demo opens.
const FIRST_SEQ: u32 = 4000;
const SHOWN: usize = 6;

fn key(h: &str) -> Key {
    unhex(h).and_then(|b| b.try_into().ok()).unwrap_or([0; 16])
}

/// One packet that went over the air.
struct Sent {
    n: usize,
    by: String,
    iv: u32,
    seq: u32,
    bytes: Vec<u8>,
    ok: bool,
    verdict: String,
}

pub struct MeshReplay {
    /// The current IV Index of every node (they all follow the same beacons here).
    iv: u32,
    in_progress: bool,
    /// The switch's next sequence number.
    seq: u32,
    switch_on: bool,
    store_seq: bool,
    lamp_on: bool,
    rpl: ReplayList,
    log: Vec<Sent>,
    count: usize,
    note: Option<String>,
    interval_s: f64,
}

impl Default for MeshReplay {
    fn default() -> Self {
        let mut d = Self { iv: 0x1234_5678, in_progress: false, seq: FIRST_SEQ, switch_on: false, store_seq: false, lamp_on: false, rpl: ReplayList::default(), log: Vec::new(), count: 0, note: None, interval_s: 5.0 };
        for _ in 0..3 {
            d.press();
        }
        d.replay(2);
        d
    }
}

impl MeshReplay {
    fn press(&mut self) {
        if self.seq >= SEQ_COUNT {
            self.note = Some("The switch may not send: its sequence numbers are used up, and it must never reuse one under the same IV Index.".into());
            return;
        }
        self.switch_on = !self.switch_on;
        let iv = tx_iv_index(self.iv, self.in_progress);
        let access = mesh::generic_onoff_set_unack(self.switch_on, (self.seq & 0xFF) as u8);
        let upper = mesh::encode_access(&access, &key(APP_KEY), self.seq, SWITCH, GROUP, iv);
        let pdu = NetworkPdu { ctl: false, ttl: 3, seq: self.seq, src: SWITCH, dst: GROUP, transport_pdu: upper.lower_transport_pdu };
        let bytes = mesh::encode_network(&pdu, &mesh::k2(&key(NET_KEY), &[0]), iv).pdu;
        self.seq += 1;
        self.deliver(format!("switch: \"{}\"", if self.switch_on { "on" } else { "off" }), iv, pdu.seq, bytes);
    }

    /// The attacker sends the recorded bytes of packet number `n` again, unchanged.
    fn replay(&mut self, n: usize) {
        if let Some(s) = self.log.iter().find(|s| s.n == n) {
            let (iv, seq, bytes) = (s.iv, s.seq, s.bytes.clone());
            self.deliver(format!("attacker: copy of #{n}"), iv, seq, bytes);
        }
    }

    /// What the lamp does with a packet: decrypt with the IV Index the IVI bit points to,
    /// check the replay list, then act on the message.
    fn deliver(&mut self, by: String, iv_sent: u32, seq_sent: u32, bytes: Vec<u8>) {
        let iv = rx_iv_index(self.iv, bytes[0] & 0x80 != 0);
        let (ok, verdict) = match mesh::decode_network(&bytes, &mesh::k2(&key(NET_KEY), &[0]), iv) {
            Err(e) => (false, format!("discarded: {e}")),
            Ok(p) => match self.rpl.check(p.src, iv, p.seq) {
                Err(last) => (false, format!("discarded as a replay: IVISeq {:014X} is not above {last:014X}", iviseq(iv, p.seq))),
                Ok(()) => match mesh::decode_access(&p.transport_pdu, &key(APP_KEY), p.seq, p.src, p.dst, iv) {
                    Some(a) if a.len() >= 3 => {
                        self.lamp_on = a[2] == 1;
                        (true, format!("accepted: lamp {}", if self.lamp_on { "on" } else { "off" }))
                    }
                    _ => (false, "discarded: the application key check failed".into()),
                },
            },
        };
        self.count += 1;
        self.note = None;
        self.log.push(Sent { n: self.count, by, iv: iv_sent, seq: seq_sent, bytes, ok, verdict });
        if self.log.len() > 40 {
            self.log.remove(0);
        }
    }

    fn restart(&mut self) {
        let before = self.seq;
        self.seq = if self.store_seq { before / STORE_RATE * STORE_RATE + STORE_RATE } else { 0 };
        self.note = Some(format!("The switch restarted. Its next sequence number is {} (before the restart: {before}).", self.seq));
    }

    fn iv_step(&mut self) {
        if self.in_progress {
            self.in_progress = false;
            self.seq = 0;
        } else {
            self.iv = self.iv.wrapping_add(1);
            self.in_progress = true;
        }
        self.note = None;
    }
}

fn human(seconds: f64) -> String {
    const DAY: f64 = 86_400.0;
    match seconds {
        s if s < 10.0 * DAY => format!("{:.0} hours", s / 3600.0),
        s if s < 730.0 * DAY => format!("{:.0} days", s / DAY),
        s => format!("{:.2} years", s / (365.25 * DAY)),
    }
}

impl Demo for MeshReplay {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "A light switch, a lamp, and someone recording", |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Press the switch").on_hover_text("The switch sends a new encrypted on/off message with its next sequence number. The attacker records it.").clicked() {
                    self.press();
                }
                if ui.button("Restart the switch").on_hover_text("Switch it off and on again, as after a battery change. Whether it remembers its sequence number depends on the box next to this button.").clicked() {
                    self.restart();
                }
                ui.checkbox(&mut self.store_seq, "the switch saves its sequence number in flash").on_hover_text("Saved every 128 messages, as Zephyr does by default. After a restart the switch continues 128 above the last saved value.");
            });
            ui.horizontal_wrapped(|ui| {
                let label = if self.in_progress { "Finish the IV Update" } else { "Start an IV Update" };
                if ui.button(label).on_hover_text("Moves the whole network one step through the IV Update procedure. In a real network each step takes at least 96 hours.").clicked() {
                    self.iv_step();
                }
                ui.label(format!("next sequence number of the switch: {}", self.seq));
            });
        });
        if let Some(note) = &self.note {
            ui.label(RichText::new(note).color(pal.trace[1]));
        }

        ui.horizontal_wrapped(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(22.0, 22.0), egui::Sense::hover());
            ui.painter().circle_filled(rect.center(), 9.0, if self.lamp_on { Color32::from_rgb(255, 214, 90) } else { pal.canvas });
            ui.painter().circle_stroke(rect.center(), 9.0, (1.2, pal.dim));
            ui.label(RichText::new(format!("lamp {}", if self.lamp_on { "on" } else { "off" })).strong());
            let entry = self.rpl.entries.get(&SWITCH).map_or("(empty)".to_owned(), |v| format!("0x{SWITCH:04X} -> IVISeq {:08X} {:06X}", v >> 24, v & 0xFF_FFFF));
            ui.label(format!("   its replay protection list: {entry}"));
        });

        ui.label(RichText::new("Everything that went over the air (the newest at the bottom)").strong());
        let mut play = None;
        for s in self.log.iter().rev().take(SHOWN).rev() {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.small_button("play it back").on_hover_text("The attacker sends exactly these recorded bytes again.").clicked() {
                        play = Some(s.n);
                    }
                    ui.label(RichText::new(format!("#{}  {}", s.n, s.by)).strong());
                    ui.label(RichText::new(format!("IV Index 0x{:08X}  SEQ {}", s.iv, s.seq)).small().color(pal.dim));
                    ui.label(RichText::new(&s.verdict).color(if s.ok { pal.good } else { pal.bad }));
                });
                ui.label(RichText::new(hex(&s.bytes)).monospace().small().color(pal.trace[0]));
            });
        }
        if let Some(n) = play {
            self.replay(n);
        }

        let (tx, accept_lo) = (tx_iv_index(self.iv, self.in_progress), self.iv.wrapping_sub(1));
        widgets::formula(
            ui,
            format!(
                "{}: current IV Index 0x{:08X}\n\
                 sends with 0x{tx:08X}, accepts 0x{accept_lo:08X} and 0x{:08X}\n\
                 the IVI bit (lowest bit of the IV Index, in every packet) says which of the two was used\n\
                 IVISeq of the next press = IV Index | SEQ = {:08X} | {:06X}: must be above the last one",
                if self.in_progress { "IV Update in progress" } else { "Normal operation" },
                self.iv,
                self.iv,
                tx,
                self.seq.min(SEQ_COUNT - 1),
            ),
        );

        widgets::group(ui, "How long do the sequence numbers last?", |ui| {
            ui.add(widgets::si_slider(&mut self.interval_s, 0.01..=3600.0, "the switch sends a message every", "s").logarithmic(true))
                .on_hover_text("The average time between two messages from one sender. Every message, also a relayed status or a repeat, uses up a sequence number.");
        });
        let life = seq_lifetime_s(self.interval_s);
        widgets::formula(
            ui,
            format!(
                "sequence numbers per IV Index = 2^24 = {SEQ_COUNT}\n\
                 lifetime = {SEQ_COUNT} x {} = {}\n\
                 one IV Update: at least {IV_STATE_MIN_H:.0} h normal + {IV_STATE_MIN_H:.0} h in progress -> at most every {IV_UPDATE_MIN_PERIOD_H:.0} h",
                si(self.interval_s, "s"),
                human(life),
            ),
        );
        widgets::status(
            ui,
            life >= IV_UPDATE_MIN_PERIOD_H * 3600.0,
            if life >= IV_UPDATE_MIN_PERIOD_H * 3600.0 {
                format!("enough: the numbers last {}, and an IV Update can renew them every 8 days", human(life))
            } else {
                format!("too busy: the numbers run out after {}, but an IV Update may start only every 192 hours", human(life))
            },
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "forget" => {
                self.restart();
                self.press();
            }
            "flash" => {
                self.store_seq = true;
                self.restart();
                self.press();
            }
            "iv" => {
                self.iv_step();
                self.press();
                self.iv_step();
                self.press();
                self.replay(2);
            }
            "busy" => self.interval_s = 0.02,
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "mesh-replay",
        title: "Replay protection and the IV index",
        blurb: "An attacker cannot forge a mesh message, but can record one and send it again. A counter in every packet stops that - until the counter runs out.",
        path: &["Wireless", "Bluetooth", "Bluetooth Mesh"],
        make: || Box::new(MeshReplay::default()),
        explain: Explanation {
            what: "A light switch (address 0x1201) controls a lamp through a [[Bluetooth Mesh]] network. Every press sends an \
                encrypted message. The list in the middle shows each packet that went over the air: the [[IV index]] and \
                [[sequence number]] it was sent with, its bytes, and what the lamp did with it. An attacker nearby records \
                every packet and can send any of them again, byte for byte: click \"play it back\".\n\n\
                The lamp keeps a replay protection list: for each sender, the highest value it has accepted. The second \
                group shows how long the sequence numbers last at a given message rate.",
            why: "Mesh messages are encrypted and carry a check value, so an attacker without the keys can neither read them nor \
                make up new ones. But they can record a genuine packet and send the same bytes again later. This is a \
                [[replay attack]]. The copy passes every check, because it is a perfect copy - think of the \"unlock\" message \
                of a door.\n\n\
                The defence is a counter. Every sender numbers its messages with a 24-bit sequence number, one higher each \
                time. The number travels in the packet and is part of the [[nonce]], so changing it makes the check value \
                fail. Each receiver remembers, per sender address, the highest number it has accepted, and discards \
                everything at or below it. A copy of an old message carries an old number and is thrown away.\n\n\
                So a sender must never go backwards. A switch that forgets its counter in a restart starts again at 0, and \
                every receiver ignores it, exactly like an attacker, until its counter climbs past the old value. Stacks \
                therefore save the counter in [[flash]]. Saving after every message would wear the flash out, so Zephyr \
                saves it every 128 messages and after a restart continues 128 above the last saved value: some numbers are \
                skipped, none is used twice.\n\n\
                24 bits are 16 777 216 numbers: 2.66 years at one message every five seconds, 194 days at one per second. \
                They must never wrap round, because a nonce may never repeat under the same key. This is what the IV Index \
                is for: a 32-bit number shared by the whole network and placed in front of the sequence number. Receivers \
                really compare both together, 7 octets called IVISeq. When a node's numbers run low, it starts an IV \
                Update: the IV Index goes up by one, and afterwards every node starts its sequence numbers again from 0. \
                With the higher IV Index in front, 0 counts as more than any old number.\n\n\
                The whole network cannot switch at the same moment, so the update has two steps of at least 96 hours each. \
                While it is \"in progress\", nodes know the new IV Index but still send with the old one; afterwards they \
                send with the new one. Receivers always accept the current and the previous IV Index, and the IVI bit in \
                every packet, the lowest bit of the IV Index, says which of the two the sender used. A recording made two \
                IV Indexes ago therefore no longer even decrypts. A node may start an update at most every 192 hours, so \
                its sequence numbers must last at least that long.",
            formulas: &[
                ("IVISeq = IV Index (4 octets) followed by SEQ (3 octets); accepted only if higher than the last one from that sender", "IV Index 0x12345678 and SEQ 4002 give 0x12345678000FA2."),
                ("lifetime of the sequence numbers = 2^24 x time between messages", "16 777 216 x 5 s = 83 886 080 s = 2.66 years."),
            ],
            try_this: &[
                Experiment {
                    action: "Restart the switch without saving its sequence number, then press it.",
                    watch: "the lamp ignoring the switch. Its numbers start at 0 again, below the 4002 the lamp remembers: its first 4003 messages after the restart will all be discarded.",
                    preset: Some("forget"),
                },
                Experiment {
                    action: "Tick \"saves its sequence number in flash\", restart the switch and press it.",
                    watch: "the switch continuing at 4096 (the last saved value, 3968, plus 128), and the lamp accepting it. Numbers 4003 to 4095 are never used.",
                    preset: Some("flash"),
                },
                Experiment {
                    action: "Start an IV Update, press, finish it, press again, then play back message #2.",
                    watch: "the press during the update still using the old IV Index. After it, the switch counts from SEQ 0 and the lamp accepts it, because the higher IV Index comes first. The old copy is still refused.",
                    preset: Some("iv"),
                },
                Experiment {
                    action: "Make the switch send a message every 0.02 s.",
                    watch: "the status turning red: the numbers last only 93 hours, less than the 192 hours an IV Update needs. The specification calls one message every five seconds a fairly high rate.",
                    preset: Some("busy"),
                },
            ],
            leaves_out: "All nodes here change their IV Index at the same moment, when you click. In a real network one node starts \
                the update, Secure Network beacons spread it, and the 96-hour minimum for each step is enforced (the \
                specification has a test mode that removes it, which is what the button does). A node that was switched off \
                for a long time accepts a jump of at most 42 IV Indexes (48 weeks); beyond that it must be provisioned again. \
                Only unsegmented messages are shown; for segmented ones the sequence number of the last segment is stored. \
                The lamp must keep its list across its own restarts too: a node that cannot tell whether a message is a \
                replay must ignore it. The restart of the switch follows Zephyr's CONFIG_BT_MESH_SEQ_STORE_RATE; other stacks \
                save differently. The network message cache, which stops relays from repeating the same packet, is a \
                separate mechanism and not shown.",
            refs: &[
                "Bluetooth Mesh Protocol 1.1: 3.9.3 (24-bit sequence number, 16,777,216 messages, 2.6 years at one every five seconds, never wraps), 3.9.4 (IV Index), 3.9.8 and Tables 3.74-3.75 (replay protection list, IVISeq = IV Index and SEQ), 3.11.5 and Table 3.84 (IV Update: two states of at least 96 hours, at most one start every 192 hours, sequence numbers reset to 0, beacons more than 42 ahead ignored, 48 weeks).",
                "Packets are encrypted with the specification's sample keys (8.2) by the code that matches sample messages #1, #18 and #21 bit for bit.",
                "Zephyr subsys/bluetooth/mesh/Kconfig: CONFIG_BT_MESH_SEQ_STORE_RATE, default 128, added to the last stored value at start-up.",
            ],
        },
    }
}
