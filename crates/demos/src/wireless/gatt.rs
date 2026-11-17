use crate::prelude::*;
use sim_core::gatt::{self, GattServer};

pub struct Gatt {
    server: GattServer,
    /// Exchanged PDUs, oldest first: (sent by the client?, bytes).
    log: Vec<(bool, Vec<u8>)>,
    handle: u16,
    write_text: String,
    write_bytes: Vec<u8>,
}

impl Gatt {
    fn request(&mut self, pdu: Vec<u8>) {
        let rsp = self.server.handle_request(&pdu);
        self.log.push((true, pdu));
        self.log.push((false, rsp));
        self.trim();
    }

    fn battery_drops(&mut self) {
        let level = self.server.get(0x0006).map_or(90, |a| a.value[0]).saturating_sub(1);
        if let Some(n) = self.server.update(0x0006, vec![level]) {
            self.log.push((false, n));
            self.trim();
        }
    }

    fn trim(&mut self) {
        let extra = self.log.len().saturating_sub(8);
        self.log.drain(..extra);
    }
}

impl Default for Gatt {
    fn default() -> Self {
        let mut g = Self { server: GattServer::sensor("Sensor", 90), log: Vec::new(), handle: 0x0006, write_text: "01 00".to_owned(), write_bytes: vec![0x01, 0x00] };
        g.request(gatt::read_request(0x0006));
        g
    }
}

/// A short name for an attribute type, for the narrow table.
fn short(uuid: u16) -> &'static str {
    match uuid {
        gatt::UUID_PRIMARY_SERVICE => "Service",
        gatt::UUID_CHARACTERISTIC => "Characteristic",
        gatt::UUID_CCCD => "Configuration",
        gatt::UUID_DEVICE_NAME => "Device Name",
        gatt::UUID_BATTERY_LEVEL => "Battery Level",
        _ => "?",
    }
}

/// What a value means, by attribute type.
fn meaning(a: &gatt::Attribute) -> String {
    let v = &a.value;
    match a.type_uuid {
        gatt::UUID_PRIMARY_SERVICE if v.len() == 2 => {
            let u = u16::from_le_bytes([v[0], v[1]]);
            format!("{} (0x{u:04X})", gatt::uuid_name(u))
        }
        gatt::UUID_CHARACTERISTIC if v.len() == 5 => {
            let h = u16::from_le_bytes([v[1], v[2]]);
            let mut props = Vec::new();
            for (bit, name) in [(gatt::PROP_READ, "read"), (gatt::PROP_WRITE, "write"), (gatt::PROP_NOTIFY, "notify")] {
                if v[0] & bit != 0 {
                    props.push(name);
                }
            }
            format!("{}, value at 0x{h:04X}", props.join(" + "))
        }
        gatt::UUID_DEVICE_NAME => format!("\"{}\"", String::from_utf8_lossy(v)),
        gatt::UUID_BATTERY_LEVEL if v.len() == 1 => format!("{} %", v[0]),
        gatt::UUID_CCCD if v.len() == 2 => {
            if u16::from_le_bytes([v[0], v[1]]) & gatt::CCCD_NOTIFY != 0 { "notifications ON".to_owned() } else { "notifications off".to_owned() }
        }
        _ => String::new(),
    }
}

impl Demo for Gatt {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        ui.label(RichText::new("The sensor's attribute table (the server). Click a row to choose it").strong());
        egui::Frame::group(ui.style()).show(ui, |ui| {
            egui::Grid::new("table").striped(true).num_columns(5).show(ui, |ui| {
                for h in ["handle", "type", "value (bytes)", "what it means", "client may"] {
                    ui.label(RichText::new(h).strong());
                }
                ui.end_row();
                for a in &self.server.attrs {
                    let is_service = a.type_uuid == gatt::UUID_PRIMARY_SERVICE;
                    let text = RichText::new(format!("0x{:04X}", a.handle)).monospace();
                    if ui.selectable_label(self.handle == a.handle, if is_service { text.strong() } else { text }).on_hover_text("Choose this attribute for the next read or write.").clicked() {
                        self.handle = a.handle;
                    }
                    ui.add(egui::Label::new(format!("0x{:04X} {}", a.type_uuid, short(a.type_uuid))).extend()).on_hover_text(gatt::uuid_name(a.type_uuid));
                    ui.add(egui::Label::new(RichText::new(sim_core::units::hex(&a.value)).monospace()).extend());
                    ui.add(egui::Label::new(RichText::new(meaning(a)).color(pal.trace[0])).extend());
                    ui.label(match (a.readable, a.writable) {
                        (true, true) => "read, write",
                        (true, false) => "read",
                        (false, true) => "write",
                        (false, false) => "-",
                    });
                    ui.end_row();
                }
            });
        });

        widgets::group(ui, "The phone (the client) sends a request", |ui| {
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "handle", |ui| ui.add(egui::Slider::new(&mut self.handle, 0x0001..=0x0010).text("handle").hexadecimal(4, false, true)).on_hover_text("Which attribute to ask for. Handles above 0x0007 do not exist on this sensor."));
                if ui.button("Read it").on_hover_text("Send a Read Request for this handle.").clicked() {
                    self.request(gatt::read_request(self.handle));
                }
                bits::hex_edit(ui, "value to write", &mut self.write_text, &mut self.write_bytes, 8);
                if ui.button("Write it").on_hover_text("Send a Write Request with these bytes to this handle.").clicked() {
                    self.request(gatt::write_request(self.handle, &self.write_bytes.clone()));
                }
            });
            ui.horizontal_wrapped(|ui| {
                let on = self.server.notifications_on(0x0006);
                if ui.button(if on { "Switch notifications off" } else { "Switch notifications on" }).on_hover_text("Write 01 00 (or 00 00) to the configuration descriptor at handle 0x0007.").clicked() {
                    self.request(gatt::write_request(0x0007, if on { &[0x00, 0x00] } else { &[0x01, 0x00] }));
                }
                if ui.button("The battery drops by 1 %").on_hover_text("Something changes on the sensor side. If notifications are on, the sensor tells the phone by itself.").clicked() {
                    self.battery_drops();
                }
                if ui.button("Clear the log").on_hover_text("Forget the exchanged packets shown below.").clicked() {
                    self.log.clear();
                }
            });
        });

        ui.label(RichText::new("What went over the air, newest at the bottom").strong());
        for (from_client, pdu) in &self.log {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let (who, color) = if *from_client { ("phone -> sensor", pal.trace[1]) } else { ("sensor -> phone", pal.trace[2]) };
                    ui.label(RichText::new(who).strong().color(color));
                    ui.monospace(sim_core::units::hex(pdu));
                });
                for (bytes, what) in gatt::describe_pdu(pdu) {
                    ui.horizontal_wrapped(|ui| {
                        ui.add_space(16.0);
                        ui.label(RichText::new(sim_core::units::hex(&bytes)).monospace().color(pal.trace[0]));
                        ui.label(what);
                    });
                }
            });
        }

        let last = self.log.last().map(|l| l.1.clone()).unwrap_or_default();
        widgets::status(
            ui,
            last.first() != Some(&gatt::OP_ERROR_RSP),
            match last.first() {
                Some(&gatt::OP_ERROR_RSP) => "the server refused the last request with an Error Response".to_owned(),
                Some(&gatt::OP_HANDLE_NFY) => "the sensor sent the new value by itself, without being asked".to_owned(),
                Some(_) => "the last request was answered".to_owned(),
                None => "nothing exchanged yet".to_owned(),
            },
        );
        widgets::formula(
            ui,
            "Read Request   = 0A, handle             Read Response  = 0B, value\n\
             Write Request  = 12, handle, value      Write Response = 13\n\
             Notification   = 1B, handle, value      Error Response = 01, request, handle, error\n\
             a handle is 2 bytes, lowest byte first: 0x0006 is sent as 06 00",
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "read-name" => {
                self.handle = 0x0003;
                self.request(gatt::read_request(0x0003));
            }
            "denied" => self.request(gatt::write_request(0x0006, &[50])),
            "notify" => {
                self.request(gatt::write_request(0x0007, &[0x01, 0x00]));
                self.battery_drops();
                self.battery_drops();
            }
            "missing" => {
                self.handle = 0x0009;
                self.request(gatt::read_request(0x0009));
            }
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "ble-gatt",
        title: "GATT: services and characteristics",
        blurb: "A connected Bluetooth device is a small table of numbered values. See how a phone reads, writes and subscribes to them.",
        path: &["Wireless", "Bluetooth", "Bluetooth LE"],
        make: || Box::new(Gatt::default()),
        explain: Explanation {
            what: "The table at the top is everything a small Bluetooth sensor offers to a connected phone: its [[attribute]]s. \
                Each row has a number (the handle), a type, and a value. Below it are the requests a phone can send, and \
                underneath, byte by byte, what was sent each way.\n\n\
                In the opening state the phone has just read the battery level: request 0A 06 00, answer 0B 5A - 5A being \
                hexadecimal for 90.",
            why: "Once two Bluetooth LE devices are connected, they need a common way to say \"what do you have, and give it to \
                me\". That is [[GATT]], the Generic Attribute Profile, and it is surprisingly simple: the device acting as \
                server keeps a table, and the other side, the client, reads and writes rows of it by number.\n\n\
                The rows are not all data. A service declaration starts a group of rows belonging together: here \"Generic \
                Access\", which every device has and which holds its name, and \"Battery\". Inside a service, a \
                characteristic declaration announces one value: what it may be used for (read, write, notify), at which \
                handle the value itself sits, and what kind of value it is. The types are 16-bit numbers that the \
                Bluetooth SIG assigns, so that any phone knows that 0x2A19 is a battery level in percent, whoever made the \
                sensor.\n\n\
                The client speaks the Attribute Protocol, [[ATT]]: tiny packets that start with an opcode byte saying what \
                they are, followed by a handle and perhaps a value. Every request gets exactly one answer: the value, an \
                acknowledgement, or an error saying why not.\n\n\
                Asking again and again whether the battery has changed would waste energy on both sides. Instead the client \
                writes 01 00 into a small extra row behind the value, the Client Characteristic Configuration. From then on, \
                the server sends a notification by itself whenever the value changes. This is how a heart-rate strap or a \
                thermometer streams its readings.",
            formulas: &[
                ("Read Request = opcode 0x0A + handle (2 bytes, lowest byte first)", "Reading handle 0x0006: 0A 06 00. The answer 0B 5A means Read Response, value 0x5A = 90 %."),
                ("Error Response = 0x01, failed request's opcode, handle, error code", "Writing to the read-only battery level: 01 12 06 00 03 - error 0x03, Write Not Permitted."),
            ],
            try_this: &[
                Experiment {
                    action: "Read handle 0x0003.",
                    watch: "the answer 0B followed by 53 65 6E 73 6F 72: the letters of \"Sensor\" in ASCII. A device's name is just one more attribute.",
                    preset: Some("read-name"),
                },
                Experiment {
                    action: "Try to write 50 % into the battery level.",
                    watch: "the server refusing with an Error Response, code 03, Write Not Permitted. The characteristic declaration at 0x0005 promised only read and notify.",
                    preset: Some("denied"),
                },
                Experiment {
                    action: "Switch notifications on, then let the battery drop twice.",
                    watch: "two packets starting 1B arriving from the sensor without any request: 1B 06 00 59, then 1B 06 00 58. Then switch them off and drop the battery again: silence.",
                    preset: Some("notify"),
                },
                Experiment { action: "Read handle 0x0009, which does not exist.", watch: "error code 01, Invalid Handle.", preset: Some("missing") },
            ],
            leaves_out: "A real client does not know the handles in advance: it first asks the server for its services and \
                characteristics (service discovery), using requests not shown here. Attributes can require an encrypted or \
                authenticated connection, which adds more error codes. Values longer than 22 bytes need special long-read \
                requests, unless the two sides agree on a larger packet size first. Notifications have a sibling, \
                indications, which the client must confirm. Custom services use 128-bit types instead of the 16-bit ones \
                reserved for standard services.",
            refs: &[
                "Bluetooth Core Specification, Vol 3 Part F (Attribute Protocol) and Part G (GATT). Opcodes, error codes, property bits and the default LE MTU of 23 as defined in BlueZ src/shared/att-types.h; attribute types 0x2800, 0x2803, 0x2902, 0x2A00, 0x2A19 and the Battery service 0x180F from BlueZ lib/bluetooth/uuid.h.",
                "Client Characteristic Configuration: a bit field, 0x0002 = indications (Core Vol 3 Part G 3.3.3.3); bit 0 (0x0001) = notifications.",
            ],
        },
    }
}
