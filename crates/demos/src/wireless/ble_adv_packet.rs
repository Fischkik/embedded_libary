use crate::prelude::*;
use sim_core::ble::{self, ADV_ACCESS_ADDRESS, ADV_CRC_INIT, AdStructure, AdvPdu, PduType};
use sim_core::modem::{self, Scheme};
use sim_core::units::{bits_lsb_first, hex, unhex};
use std::collections::BTreeSet;

pub struct BleAdvPacket {
    pdu_type: PduType,
    random_address: bool,
    address: [u8; 6],
    address_text: String,
    adv_data: Vec<u8>,
    adv_text: String,
    name: String,
    tx_channel: u8,
    rx_channel: u8,
    /// On-air bit positions damaged in transit.
    flipped: BTreeSet<usize>,
    show_whitened: bool,
    selected_field: Option<usize>,
}

impl Default for BleAdvPacket {
    fn default() -> Self {
        let mut d = Self {
            pdu_type: PduType::AdvInd,
            random_address: true,
            address: [0xC1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6],
            address_text: String::new(),
            adv_data: Vec::new(),
            adv_text: String::new(),
            name: "Lab".to_owned(),
            tx_channel: 37,
            rx_channel: 37,
            flipped: BTreeSet::new(),
            show_whitened: true,
            selected_field: None,
        };
        d.address_text = hex(&d.address).replace(' ', ":");
        d.rebuild_from_name();
        d
    }
}

impl BleAdvPacket {
    fn rebuild_from_name(&mut self) {
        self.adv_data = ble::build_ad(&[AdStructure::flags(0x06), AdStructure::complete_name(&self.name)]);
        self.adv_text = hex(&self.adv_data);
    }

    fn pdu(&self) -> AdvPdu {
        AdvPdu { pdu_type: self.pdu_type, ch_sel: false, tx_add: self.random_address, rx_add: false, adv_a: self.address, adv_data: self.adv_data.clone() }
    }

    fn fields(&self) -> Vec<bits::FieldSpan> {
        use bits::FieldSpan as F;
        vec![
            F::new("Preamble", 8, "Alternating bits. Gives the receiver a moment to lock on to the signal and find where bits start."),
            F::new("Access address", 32, "Says which conversation this packet belongs to. All advertising packets use 0x8E89BED6."),
            F::new("Header", 16, "First byte: the packet type and two address flags. Second byte: how many bytes follow."),
            F::new("Advertiser address", 48, "Who is sending. Six bytes, sent last byte first."),
            F::new("Advertising data", self.adv_data.len() * 8, "What the device wants to tell the world, as a list of small labelled items."),
            F::new("CRC", 24, "A checksum of header, address and data. The receiver recomputes it to detect damage."),
        ]
    }
}

impl Demo for BleAdvPacket {
    fn ui(&mut self, ui: &mut Ui, cx: &mut DemoCx) {
        let pal = theme::palette(ui);

        // ---- Sender -------------------------------------------------------------
        widgets::group(ui, "The sender: what goes into the packet", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Packet type:");
                for t in PduType::COMMON {
                    ui.selectable_value(&mut self.pdu_type, t, t.name()).on_hover_text(t.meaning());
                }
            });
            ui.label(RichText::new(self.pdu_type.meaning()).small().color(pal.dim));
            ui.horizontal(|ui| {
                let l = ui.label("Device name:");
                let r = ui.add(egui::TextEdit::singleline(&mut self.name).char_limit(20).desired_width(180.0)).labelled_by(l.id);
                if r.on_hover_text("Typing here rebuilds the advertising data: a Flags item and a Complete Local Name item.").changed() {
                    self.rebuild_from_name();
                }
            });
            if bits::hex_edit(ui, "Advertising data, as raw bytes (edit freely, up to 31 bytes):", &mut self.adv_text, &mut self.adv_data, 31) {
                self.flipped.clear();
            }
            ui.horizontal(|ui| {
                let l = ui.label("Address:");
                let r = ui.add(egui::TextEdit::singleline(&mut self.address_text).font(egui::TextStyle::Monospace).desired_width(170.0)).labelled_by(l.id);
                if r.changed() {
                    if let Some(Ok(a)) = unhex(&self.address_text).map(<[u8; 6]>::try_from) {
                        self.address = a;
                    }
                }
                ui.checkbox(&mut self.random_address, "random address").on_hover_text("Sets the TxAdd flag in the header: the address is a random one, not a registered manufacturer address.");
            });
            ui.horizontal(|ui| {
                ui.label("Send on advertising channel:");
                for ch in [37u8, 38, 39] {
                    if ui.selectable_value(&mut self.tx_channel, ch, format!("{ch}  ({} MHz)", ble::channel_mhz(ch))).clicked() {
                        self.rx_channel = ch;
                    }
                }
            });
        });

        let pdu_bytes = self.pdu().to_bytes();
        let packet = ble::assemble(&pdu_bytes, self.tx_channel, ADV_ACCESS_ADDRESS, ADV_CRC_INIT);
        let fields = self.fields();

        // Decode of the advertising data.
        widgets::group(ui, "Advertising data, item by item", |ui| {
            let (items, error) = ble::parse_ad(&self.adv_data);
            egui::Grid::new("ad_items").striped(true).num_columns(4).show(ui, |ui| {
                for h in ["length", "type", "meaning", "data"] {
                    ui.label(RichText::new(h).strong());
                }
                ui.end_row();
                for item in &items {
                    ui.monospace(format!("{:02x}", item.data.len() + 1));
                    ui.monospace(format!("{:02x}", item.ad_type));
                    ui.label(ble::ad_type_name(item.ad_type));
                    let text = match item.ad_type {
                        0x08 | 0x09 => format!("{}   \"{}\"", hex(&item.data), String::from_utf8_lossy(&item.data)),
                        _ => hex(&item.data),
                    };
                    ui.monospace(text);
                    ui.end_row();
                }
            });
            if let Some(e) = error {
                widgets::status(ui, false, format!("the rest cannot be read: {e}"));
            }
        });

        // ---- On air -------------------------------------------------------------
        let mut air = if self.show_whitened {
            packet.bytes()
        } else {
            let mut b = vec![packet.preamble];
            b.extend_from_slice(&packet.access_address);
            b.extend_from_slice(&packet.pdu);
            b.extend_from_slice(&packet.crc);
            b
        };
        self.flipped.retain(|b| *b < air.len() * 8);
        if self.show_whitened {
            for &b in &self.flipped {
                air[b / 8] ^= 1 << (b % 8);
            }
        }
        widgets::group(ui, "On air: every bit, in the order it is sent", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut self.show_whitened, true, "as transmitted (whitened)").on_hover_text("The real bits on air. Header, address, data and CRC are scrambled.");
                ui.selectable_value(&mut self.show_whitened, false, "before whitening").on_hover_text("The same packet before scrambling, so you can read the fields.");
                if ui.add_enabled(!self.flipped.is_empty(), egui::Button::new("Repair damaged bits")).clicked() {
                    self.flipped.clear();
                }
            });
            bits::field_legend(ui, &fields, &mut self.selected_field);
            let strip = bits::bit_strip(ui, &bits_lsb_first(&air), &fields, self.selected_field);
            if let Some(i) = strip.clicked {
                if self.show_whitened {
                    if !self.flipped.remove(&i) {
                        self.flipped.insert(i);
                    }
                } else {
                    self.show_whitened = true;
                }
            }
            if let Some(f) = strip.hovered.or(self.selected_field) {
                ui.label(RichText::new(format!("{}: {}", fields[f].name, fields[f].help)).color(pal.dim));
            }
            ui.label(
                RichText::new(format!(
                    "{} bytes = {} bits = {} on air at 1 Mbit/s. Click a bit in the transmitted view to damage it in transit{}.",
                    air.len(),
                    air.len() * 8,
                    si(packet.duration_us() * 1e-6, "s"),
                    if self.flipped.is_empty() { String::new() } else { format!(" ({} damaged)", self.flipped.len()) }
                ))
                .small()
                .color(pal.dim),
            );
            bits::hex_row(ui, "As bytes:", &air, None);
        });

        // The radio signal for the first bytes.
        widgets::group(ui, "The radio signal for the first 6 bytes (preamble, access address, start of the header)", |ui| {
            let shown = bits_lsb_first(&packet.bytes()[..6]);
            let shape = modem::frequency_shape(&shown, Scheme::Gfsk, 16, 0.5);
            let khz: Vec<f64> = shape.iter().map(|s| s * 250.0).collect();
            plots::plot("gfsk", 185.0).x_axis_label("time [µs]").y_axis_label("frequency offset [kHz]").show(ui, |p| {
                p.line(plots::line(&format!("offset from {} MHz", ble::channel_mhz(self.tx_channel)), plots::series(&khz, 0.0, 1.0 / 16.0), pal.trace[0]));
            });
        });

        // ---- Receiver -----------------------------------------------------------
        widgets::group(ui, "The receiver", |ui| {
            ui.horizontal(|ui| {
                ui.label("Listening on channel:");
                for ch in [37u8, 38, 39] {
                    ui.selectable_value(&mut self.rx_channel, ch, format!("{ch}")).on_hover_text("A real receiver on another channel would hear nothing at all. Here it still gets the bits, so you can see what the channel number does to the scrambling.");
                }
            });
            let whitened_rx = {
                let mut b = packet.bytes();
                for &i in &self.flipped {
                    b[i / 8] ^= 1 << (i % 8);
                }
                b
            };
            if whitened_rx[1..5] != packet.access_address {
                widgets::status(ui, false, "The access address does not match. The receiver never notices that a packet arrived.");
                return;
            }
            let Some(rx) = ble::receive(&whitened_rx[5..], self.rx_channel, ADV_CRC_INIT) else { return };
            bits::hex_row(ui, "After removing the whitening:", &rx.pdu, None);
            ui.horizontal_wrapped(|ui| {
                ui.label("CRC received");
                ui.monospace(hex(&rx.crc_received));
                ui.label("  CRC computed from the received bytes");
                ui.monospace(hex(&rx.crc_computed));
            });
            if !rx.crc_ok() {
                let why = if self.rx_channel != self.tx_channel {
                    "the receiver removed the whitening pattern of the wrong channel, so everything is garbage"
                } else {
                    "a bit was damaged on the way"
                };
                widgets::status(ui, false, format!("CRC does not match: the packet is thrown away ({why})."));
                return;
            }
            widgets::status(ui, true, "CRC matches: the packet is accepted.");
            match AdvPdu::from_bytes(&rx.pdu) {
                Ok(p) => {
                    let (items, _) = ble::parse_ad(&p.adv_data);
                    let name = items.iter().find(|i| i.ad_type == 0x09 || i.ad_type == 0x08).map(|i| String::from_utf8_lossy(&i.data).into_owned());
                    ui.label(format!(
                        "A scanner would show: {}  from {}  ({} address){}",
                        p.pdu_type.name(),
                        hex(&p.adv_a).replace(' ', ":"),
                        if p.tx_add { "random" } else { "public" },
                        name.map_or(String::new(), |n| format!(",  name \"{n}\""))
                    ));
                    if items.iter().any(|i| i.ad_type == 0x2A) && ui.button("This carries a mesh message - open it in the Mesh packet demo").clicked() {
                        if let Some(m) = items.iter().find(|i| i.ad_type == 0x2A) {
                            cx.open = Some(("mesh-pdu", [vec![0xFF], m.data.clone()].concat()));
                        }
                    }
                }
                Err(e) => widgets::status(ui, false, format!("The bytes passed the CRC but do not form a valid advertising packet: {e}")),
            }
        });
    }

    fn apply_preset(&mut self, preset: &str) {
        match preset {
            "damage" => {
                self.flipped.insert(100);
            }
            "wrong-channel" => self.rx_channel = 39,
            "plain" => self.show_whitened = false,
            _ => {}
        }
    }

    /// Advertising data handed over by another demo (for example a mesh message).
    fn receive(&mut self, data: &[u8]) {
        self.pdu_type = PduType::AdvNonconnInd;
        self.adv_data = data.iter().copied().take(31).collect();
        self.adv_text = hex(&self.adv_data);
        self.name.clear();
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "ble-adv-packet",
        title: "Anatomy of a BLE advertising packet",
        blurb: "Build a Bluetooth LE packet byte by byte, see every bit that goes on air, damage one, and watch the receiver notice.",
        path: &["Wireless", "Bluetooth", "Bluetooth LE"],
        make: || Box::new(BleAdvPacket::default()),
        explain: Explanation {
            what: "This is one complete Bluetooth LE [[advertising]] [[packet]], the kind a device broadcasts to say \"I am here\". \
                At the top you set what the sender wants to say. The coloured strip in the middle shows every [[bit]] in the exact \
                order it leaves the antenna. Below that is the radio signal for the first bits, and at the bottom what a receiver \
                makes of it all.\n\n\
                The bits are exact: a Bluetooth sniffer next to a real device sending this packet would show the same bytes.",
            why: "A packet is wrapped in layers, and each layer solves one problem.\n\n\
                The [[preamble]] is eight alternating bits. It carries no information; it gives the receiver a moment to settle on \
                the signal and find the bit rhythm. The [[access address]] comes next and answers \"is this for me?\". A receiver \
                compares incoming bits against the address it expects, and only when all 32 match does it start recording. Every \
                advertising packet uses the same fixed address.\n\n\
                Then comes the content, called the [[PDU]]: a two-byte header (the packet type, and the number of bytes that follow), \
                the sender's six-byte address, and up to 31 bytes of advertising data. That data is a list of small labelled items, \
                each one an [[AD structure]]: length, type, value.\n\n\
                The [[CRC]] is three bytes calculated from the PDU. The receiver does the same calculation on what it received. If \
                even one bit changed on the way, the results differ and the packet is thrown away. Bluetooth never tries to guess \
                what a damaged packet meant.\n\n\
                Last, [[whitening]]. Radios cope badly with long runs of identical bits, because the frequency sits still and the \
                receiver loses track of the bit timing. So the PDU and CRC are scrambled with a fixed bit pattern that depends on \
                the [[channel]] number, and the receiver, knowing the channel, removes it again. It is not encryption: anyone can \
                undo it.",
            formulas: &[
                ("bytes on air = 1 + 4 + 2 + 6 + data + 3", "Preamble, access address, header, address, advertising data, CRC. With 31 bytes of data: 47 bytes = 376 µs."),
                ("CRC polynomial: x^24 + x^10 + x^9 + x^6 + x^4 + x^3 + x + 1", "The exact rule for the 24-bit checksum, started from the value 0x555555 for advertising packets."),
                ("whitening: x^7 + x^4 + 1", "A 7-bit shift register started from the channel number produces the scrambling pattern."),
            ],
            try_this: &[
                Experiment {
                    action: "Click any one bit in the strip (in the \"as transmitted\" view) to damage it.",
                    watch: "the receiver at the bottom: the two CRC values no longer agree and the whole packet is thrown away. One wrong bit in 200 is enough.",
                    preset: Some("damage"),
                },
                Experiment {
                    action: "Switch the strip to \"before whitening\" and back.",
                    watch: "the preamble and access address staying the same while everything after them changes completely. Only the PDU and CRC are scrambled.",
                    preset: Some("plain"),
                },
                Experiment {
                    action: "Leave the sender on channel 37 and set the receiver to channel 39.",
                    watch: "the de-whitened bytes turning into garbage and the CRC failing. The scrambling pattern depends on the channel number.",
                    preset: Some("wrong-channel"),
                },
                Experiment {
                    action: "Change the device name, one letter at a time.",
                    watch: "the length byte of the name item, the length in the header, and the entire CRC all changing.",
                    preset: None,
                },
            ],
            leaves_out: "Only the classic (\"legacy\") advertising packets are shown, on the basic 1 Mbit/s radio mode. Bluetooth 5 \
                added extended advertising with longer packets, a 2 Mbit/s mode and a long-range coded mode. The receiver here gets \
                the bits handed to it; a real one first has to detect the signal and find the bit timing from the preamble.",
            refs: &[
                "Bluetooth Core 5.4, Vol 6 Part B 2.1 (packet format), 2.3 (advertising PDUs), 3.1 (CRC), 3.2 (whitening).",
                "Checked bit for bit against the sample packet in Vol 6 Part C 4.2.1.",
            ],
        },
    }
}
