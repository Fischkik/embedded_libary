use crate::prelude::*;
use sim_core::ble::{AdStructure, build_ad};
use sim_core::mesh::{self, Key, NetworkPdu};
use sim_core::units::{hex, unhex};

const SPEC_NET_KEY: &str = "7dd7364cd842ad18c17c2b820c84c3d6";
const SPEC_APP_KEY: &str = "63964771734fbd76e3b40519d1d94a48";

pub struct MeshPdu {
    net_key: Key,
    net_key_text: String,
    app_key: Key,
    app_key_text: String,
    iv_index: u32,
    src: u16,
    dst: u16,
    seq: u32,
    ttl: u8,
    on: bool,
    tid: u8,
    /// Bytes as they arrive at the receiver. Follows the sent packet until edited.
    rx: Vec<u8>,
    rx_text: String,
    rx_edited: bool,
    receiver_other_network: bool,
    receiver_has_app_key: bool,
}

impl Default for MeshPdu {
    fn default() -> Self {
        let key = |h: &str| -> Key { unhex(h).and_then(|b| b.try_into().ok()).unwrap_or([0; 16]) };
        Self {
            net_key: key(SPEC_NET_KEY),
            net_key_text: SPEC_NET_KEY.to_owned(),
            app_key: key(SPEC_APP_KEY),
            app_key_text: SPEC_APP_KEY.to_owned(),
            iv_index: 0x1234_5678,
            src: 0x0001,
            dst: 0x000A,
            seq: 7,
            ttl: 5,
            on: true,
            tid: 1,
            rx: Vec::new(),
            rx_text: String::new(),
            rx_edited: false,
            receiver_other_network: false,
            receiver_has_app_key: true,
        }
    }
}

fn key_edit(ui: &mut Ui, label: &str, hover: &str, text: &mut String, key: &mut Key) {
    let pal = theme::palette(ui);
    ui.horizontal(|ui| {
        let l = ui.label(label);
        let r = ui.add(egui::TextEdit::singleline(text).font(egui::TextStyle::Monospace).desired_width(300.0)).labelled_by(l.id).on_hover_text(hover);
        let parsed: Option<Key> = unhex(text).and_then(|b| b.try_into().ok());
        match parsed {
            Some(k) if r.changed() => *key = k,
            None => {
                ui.label(RichText::new("needs exactly 32 hex digits").color(pal.bad).small());
            }
            _ => {}
        }
    });
}

fn step(ui: &mut Ui, label: &str, bytes: &[u8], note: &str) {
    let pal = theme::palette(ui);
    ui.horizontal_wrapped(|ui| {
        ui.label(label);
        ui.label(RichText::new(hex(bytes)).monospace().strong());
        if !note.is_empty() {
            ui.label(RichText::new(note).small().color(pal.dim));
        }
    });
}

impl Demo for MeshPdu {
    fn ui(&mut self, ui: &mut Ui, cx: &mut DemoCx) {
        let pal = theme::palette(ui);

        widgets::group(ui, "The message and who it is for", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Tell the lamp to switch");
                ui.selectable_value(&mut self.on, true, "ON");
                ui.selectable_value(&mut self.on, false, "OFF");
                ui.add(egui::Slider::new(&mut self.ttl, 0..=127).text("TTL")).on_hover_text(mesh::ttl_meaning(self.ttl));
            });
            ui.horizontal_wrapped(|ui| {
                ui.add(egui::Slider::new(&mut self.src, 1..=0x7FFF).hexadecimal(4, false, false).text("sender address")).on_hover_text("Unicast address of the sending node (0001 to 7fff).");
                ui.add(egui::Slider::new(&mut self.dst, 1..=0xFFFF).hexadecimal(4, false, false).text("destination address")).on_hover_text(mesh::address_kind(self.dst));
            });
            ui.label(RichText::new(format!("destination {:04x} is: {}", self.dst, mesh::address_kind(self.dst))).small().color(pal.dim));
            ui.add(egui::Slider::new(&mut self.seq, 0..=0xFF_FFFF).logarithmic(true).text("sequence number"))
                .on_hover_text("Goes up by one for every message this node sends. It makes every encryption unique, and receivers reject numbers they have seen.");
        });
        widgets::group(ui, "The network's secrets", |ui| {
            key_edit(ui, "NetKey:", "Shared by every node in the network. Needed to read addresses and to relay.", &mut self.net_key_text, &mut self.net_key);
            key_edit(ui, "AppKey:", "Shared only by the devices of one application (say, lighting). Needed to read what the message says.", &mut self.app_key_text, &mut self.app_key);
            ui.add(egui::Slider::new(&mut self.iv_index, 0..=u32::MAX).hexadecimal(8, false, false).logarithmic(true).text("IV index"))
                .on_hover_text("A counter shared by the whole network. It is part of every encryption.");
        });

        // ---- Build the packet, layer by layer -----------------------------------
        let keys = mesh::k2(&self.net_key, &[0]);
        let access = mesh::generic_onoff_set_unack(self.on, self.tid);
        let upper = mesh::encode_access(&access, &self.app_key, self.seq, self.src, self.dst, self.iv_index);
        let net = NetworkPdu { ctl: false, ttl: self.ttl, seq: self.seq, src: self.src, dst: self.dst, transport_pdu: upper.lower_transport_pdu.clone() };
        let steps = mesh::encode_network(&net, &keys, self.iv_index);
        if !self.rx_edited {
            self.rx = steps.pdu.clone();
            self.rx_text = hex(&self.rx);
        }

        widgets::group(ui, "Layer 1 - what the message means (access layer)", |ui| {
            step(ui, "Access message:", &access, &mesh::describe_access(&access));
            ui.label(RichText::new("82 03 = \"Generic OnOff Set, no reply wanted\", then on/off, then a transaction number.").small().color(pal.dim));
        });
        widgets::group(ui, "Layer 2 - lock the meaning with the AppKey (transport layer)", |ui| {
            step(ui, "Nonce:", &upper.nonce, "type, sequence number, sender, destination, IV index");
            step(ui, "Encrypted message:", &upper.encrypted, "");
            step(ui, "TransMIC:", &upper.trans_mic, "proof it was made with the AppKey");
            step(ui, "Header:", &[upper.header], &format!("says \"application key number {:02x}\" (a 6-bit fingerprint of the AppKey)", upper.aid));
            step(ui, "Transport PDU:", &upper.lower_transport_pdu, "header + encrypted message + TransMIC");
        });
        widgets::group(ui, "Layer 3 - lock the addresses with the NetKey (network layer)", |ui| {
            step(ui, "From the NetKey:", &[keys.nid], "NID, a 7-bit fingerprint sent in the clear");
            step(ui, "  encryption key:", &keys.encryption_key, "");
            step(ui, "  privacy key:", &keys.privacy_key, "");
            step(ui, "Nonce:", &steps.nonce, "type, TTL, sequence number, sender, IV index");
            step(ui, "Encrypted destination + transport PDU:", &steps.encrypted, "");
            step(ui, "NetMIC:", &steps.net_mic, "proof it was made with the NetKey");
        });
        widgets::group(ui, "Layer 4 - hide who is talking (obfuscation)", |ui| {
            step(ui, "Readable header (TTL, sequence number, sender):", &steps.header_plain, "");
            step(ui, "Scrambling pattern (PECB):", &steps.pecb, "made from the privacy key, the IV index and the first encrypted bytes");
            step(ui, "Header as sent:", &steps.header_obfuscated, "= readable header XOR pattern");
        });
        widgets::group(ui, "The finished network packet", |ui| {
            let fields = [
                bits::FieldSpan::new("IVI + NID", 8, "Lowest bit of the IV index, and the NetKey fingerprint. The only readable byte."),
                bits::FieldSpan::new("Obfuscated header", 48, "TTL, sequence number and sender, scrambled."),
                bits::FieldSpan::new("Encrypted destination", 16, "The destination address, encrypted with the NetKey."),
                bits::FieldSpan::new("Encrypted transport PDU", steps.encrypted.len() * 8 - 16, "The application message, encrypted twice: with the AppKey and then the NetKey."),
                bits::FieldSpan::new("NetMIC", 32, "Integrity check. Changing any bit of the packet makes it fail."),
            ];
            let mut none = None;
            bits::field_legend(ui, &fields, &mut none);
            let bits_msb: Vec<bool> = steps.pdu.iter().flat_map(|b| (0..8).rev().map(move |i| (b >> i) & 1 == 1)).collect();
            bits::bit_strip(ui, &bits_msb, &fields, None);
            bits::hex_row(ui, &format!("{} bytes:", steps.pdu.len()), &steps.pdu, None);
            if ui.button("See this inside a Bluetooth LE advertising packet").on_hover_text("Mesh packets travel as the data of ordinary advertising packets").clicked() {
                cx.open = Some(("ble-adv-packet", build_ad(&[AdStructure::mesh_message(&steps.pdu)])));
            }
        });

        // ---- Receiver ------------------------------------------------------------
        widgets::group(ui, "A node that receives it", |ui| {
            if bits::hex_edit(ui, "Bytes received (edit them to tamper with the packet):", &mut self.rx_text, &mut self.rx, 40) {
                self.rx_edited = self.rx != steps.pdu;
            }
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(self.rx_edited, egui::Button::new("Undo tampering")).clicked() {
                    self.rx_edited = false;
                }
                ui.checkbox(&mut self.receiver_other_network, "belongs to a different network (other NetKey)");
                ui.checkbox(&mut self.receiver_has_app_key, "has the AppKey").on_hover_text("A pure relay does not need the AppKey: it can pass the message on without being able to read it.");
            });
            let rx_keys = if self.receiver_other_network { mesh::k2(&[0x55; 16], &[0]) } else { keys };
            match mesh::decode_network(&self.rx, &rx_keys, self.iv_index) {
                Err(e) => widgets::status(ui, false, format!("Dropped. {e}")),
                Ok(p) => {
                    widgets::status(ui, true, "NetMIC correct: this packet belongs to our network and is unchanged.");
                    ui.label(format!(
                        "Network layer reads: from {:04x} to {:04x} ({}), sequence number {}, TTL {} - {}.",
                        p.src,
                        p.dst,
                        mesh::address_kind(p.dst),
                        p.seq,
                        p.ttl,
                        mesh::ttl_meaning(p.ttl)
                    ));
                    if !self.receiver_has_app_key {
                        ui.label("Without the AppKey this node cannot read the message itself - but it knows enough to relay it.");
                    } else {
                        match mesh::decode_access(&p.transport_pdu, &self.app_key, p.seq, p.src, p.dst, self.iv_index) {
                            Some(a) => widgets::status(ui, true, format!("TransMIC correct. The message says: {}", mesh::describe_access(&a))),
                            None => widgets::status(ui, false, "TransMIC wrong: the AppKey does not fit, so the content stays unreadable."),
                        }
                    }
                }
            }
        });
    }

    fn apply_preset(&mut self, preset: &str) {
        match preset {
            "other-network" => self.receiver_other_network = true,
            "relay-only" => self.receiver_has_app_key = false,
            "group" => self.dst = 0xC001,
            _ => {}
        }
    }

    /// `[0x00, ttl, src_hi, src_lo, dst_hi, dst_lo]` from the flooding demo, or
    /// `[0xFF, network PDU...]` from the BLE packet demo.
    fn receive(&mut self, data: &[u8]) {
        match data {
            [0x00, ttl, s1, s0, d1, d0, ..] => {
                self.ttl = *ttl;
                self.src = u16::from_be_bytes([*s1, *s0]).clamp(1, 0x7FFF);
                self.dst = u16::from_be_bytes([*d1, *d0]).max(1);
            }
            [0xFF, pdu @ ..] => {
                self.rx = pdu.to_vec();
                self.rx_text = hex(&self.rx);
                self.rx_edited = true;
            }
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "mesh-pdu",
        title: "Inside a mesh packet: keys and encryption",
        blurb: "Follow \"lamp on\" through every layer until it is 22 unreadable bytes - then tamper with it and watch it be rejected.",
        path: &["Wireless", "Bluetooth", "Bluetooth Mesh"],
        make: || Box::new(MeshPdu::default()),
        explain: Explanation {
            what: "A four-byte instruction - switch the lamp on - is wrapped layer by layer, top to bottom of the page, until it is \
                the finished network [[packet]] that the flooding demo passes around. Every intermediate value is shown, and every \
                one of them is real: these are the bytes an actual Bluetooth Mesh node would produce with these keys.\n\n\
                At the bottom a receiving node takes the packet apart again. You can change its bytes or its keys.",
            why: "In a mesh, your message is carried by other people's devices. Any [[relay]] has to be able to pass it on, but \
                should a light switch in the corridor be able to read or forge commands for the door lock? Bluetooth Mesh answers \
                with two separate keys.\n\n\
                The [[AppKey]] protects what the message means. Only devices belonging to the same application have it. The \
                message is encrypted with it using [[AES]], and a 4-byte [[MIC]] is added that proves the sender had the key.\n\n\
                The [[NetKey]] protects the delivery. Every node in the network has it. With it, the destination address and the \
                already-encrypted message are encrypted again, and a second MIC is added. So a relay can open the outer layer, see \
                where the packet is going and what its [[TTL]] is, and pass it on - without being able to read the inner layer.\n\n\
                Encrypting the same message twice with the same key would give the same bytes twice, and an eavesdropper could \
                recognise \"that is the lamp-on command again\". So every encryption also mixes in a [[nonce]] built from the \
                sender's address, its [[sequence number]] and the network's [[IV index]]. The sequence number goes up with every \
                message, which also lets receivers refuse old packets that someone recorded and replays.\n\n\
                The last step, [[obfuscation]], scrambles the sender address and sequence number. They are not secret from members \
                of the network, but hiding them stops an outsider from tracking a device by watching its packets.\n\n\
                Only the first byte stays readable. It holds the [[NID]], a short fingerprint of the NetKey, so that a node can \
                tell at a glance whether a packet could be for its network before spending effort on decrypting it.",
            formulas: &[
                ("NID, encryption key, privacy key = k2(NetKey)", "One secret NetKey is stretched into the three values used on air. The NetKey itself is never transmitted."),
                ("network packet = NID, scrambled header, AES-CCM(destination + message), NetMIC", "9 bytes of overhead plus 4 bytes of NetMIC around the transport data."),
            ],
            try_this: &[
                Experiment {
                    action: "In the \"Bytes received\" box, change any single hex digit.",
                    watch: "the receiver dropping the packet because the NetMIC no longer fits. It cannot even tell which part you changed.",
                    preset: None,
                },
                Experiment {
                    action: "Tick \"belongs to a different network\".",
                    watch: "the packet being ignored after looking at just the first byte: the NID does not match.",
                    preset: Some("other-network"),
                },
                Experiment {
                    action: "Untick \"has the AppKey\".",
                    watch: "the node still reading sender, destination and TTL - everything a relay needs - but not the command.",
                    preset: Some("relay-only"),
                },
                Experiment {
                    action: "Raise the sequence number by one and compare the finished packet before and after.",
                    watch: "almost every byte changing, although the command is the same. Nobody listening can tell that it is a repeat.",
                    preset: None,
                },
            ],
            leaves_out: "Only short messages that fit one packet are shown; longer ones are split into segments. Messages to virtual \
                addresses, the device key used for configuration, and friendship keys are not covered. A real node also checks the \
                sequence number against the last one it saw from that sender and refuses anything old.",
            refs: &[
                "Mesh Protocol 1.1: 3.4.4 (network PDU), 3.9.2 (k1 to k4), 3.9.5 (nonces), 3.9.7 (encryption and obfuscation).",
                "Checked byte for byte against the specification's sample messages 1, 18 and 21 (section 8.3).",
                "Mesh Model 1.1, 3.2.1 (Generic OnOff messages).",
            ],
        },
    }
}
