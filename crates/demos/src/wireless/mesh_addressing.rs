use crate::prelude::*;
use sim_core::mesh::{accepts, address_kind};

/// The lamps in the building: (name, unicast address).
const LAMPS: [(&str, u16); 6] = [("Office 1, lamp A", 0x0005), ("Office 1, lamp B", 0x0006), ("Office 2, lamp A", 0x0007), ("Office 2, lamp B", 0x0008), ("Corridor lamp", 0x0009), ("Stairwell lamp", 0x000A)];
const OFFICE_1: u16 = 0xC001;
const OFFICE_2: u16 = 0xC002;
const CORRIDOR: u16 = 0xC003;

pub struct MeshAddressing {
    /// Per lamp: subscribed to office 1, office 2, corridor.
    subs: [[bool; 3]; 6],
    publish: u16,
    on: [bool; 6],
    last_press: Option<u16>,
}

impl MeshAddressing {
    fn subscriptions(&self, lamp: usize) -> Vec<u16> {
        [OFFICE_1, OFFICE_2, CORRIDOR].iter().zip(self.subs[lamp]).filter(|(_, s)| *s).map(|(g, _)| *g).collect()
    }

    fn press(&mut self) {
        let dst = self.publish;
        for i in 0..LAMPS.len() {
            if accepts(LAMPS[i].1, &self.subscriptions(i), dst) {
                self.on[i] = !self.on[i];
            }
        }
        self.last_press = Some(dst);
    }
}

impl Default for MeshAddressing {
    fn default() -> Self {
        let mut d = Self { subs: [[true, false, false], [true, false, false], [false, true, false], [false, true, false], [false, false, true], [false, false, true]], publish: OFFICE_1, on: [false; 6], last_press: None };
        d.press();
        d
    }
}

fn group_name(addr: u16) -> String {
    match addr {
        OFFICE_1 => "group 0xC001 \"Office 1\"".to_owned(),
        OFFICE_2 => "group 0xC002 \"Office 2\"".to_owned(),
        CORRIDOR => "group 0xC003 \"Corridor\"".to_owned(),
        0xFFFF => "0xFFFF all-nodes".to_owned(),
        a => format!("0x{a:04X}"),
    }
}

impl Demo for MeshAddressing {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "The wall switch", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("publishes to:");
                for addr in [OFFICE_1, OFFICE_2, CORRIDOR, 0xFFFF, LAMPS[4].1] {
                    let label = if addr == LAMPS[4].1 { "0x0009 (one lamp directly)".to_owned() } else { group_name(addr) };
                    ui.selectable_value(&mut self.publish, addr, label).on_hover_text(address_kind(addr));
                }
            });
            if ui.button("Press the switch").on_hover_text("Send one \"toggle\" message to the publish address. Every lamp that accepts it switches.").clicked() {
                self.press();
            }
        });

        ui.label(RichText::new("The lamps, with their own address and the groups they subscribe to").strong());
        egui::Frame::group(ui.style()).show(ui, |ui| {
            egui::Grid::new("lamps").striped(true).num_columns(6).show(ui, |ui| {
                for h in ["", "lamp", "own address", "Office 1", "Office 2", "Corridor"] {
                    ui.label(RichText::new(h).strong());
                }
                ui.end_row();
                for i in 0..LAMPS.len() {
                    let (rect, _) = ui.allocate_exact_size(vec2(22.0, 22.0), egui::Sense::hover());
                    let lit = self.on[i];
                    ui.painter().circle_filled(rect.center(), 9.0, if lit { Color32::from_rgb(255, 214, 90) } else { pal.canvas });
                    ui.painter().circle_stroke(rect.center(), 9.0, (1.2, pal.dim));
                    let hit = self.last_press.is_some_and(|d| accepts(LAMPS[i].1, &self.subscriptions(i), d));
                    ui.label(RichText::new(LAMPS[i].0).color(if hit { pal.trace[2] } else { pal.text }));
                    ui.monospace(format!("0x{:04X}", LAMPS[i].1));
                    for sub in self.subs[i].iter_mut() {
                        ui.checkbox(sub, "").on_hover_text("Subscribe this lamp to the group: it will react to messages sent to the group's address.");
                    }
                    ui.end_row();
                }
            });
        });
        widgets::key(ui, &[(Color32::from_rgb(255, 214, 90), "lamp on"), (pal.trace[2], "green name: this lamp accepted the last message")]);

        let reached: Vec<&str> = (0..LAMPS.len()).filter(|&i| accepts(LAMPS[i].1, &self.subscriptions(i), self.publish)).map(|i| LAMPS[i].0).collect();
        widgets::formula(
            ui,
            format!(
                "destination 0x{:04X}: {}\n\
                 accepted if: own address, or subscribed to it, or 0xFFFF (all-nodes)\n\
                 unicast 0x0001-0x7FFF   virtual 0x8000-0xBFFF   group 0xC000-0xFFFF",
                self.publish,
                address_kind(self.publish),
            ),
        );
        widgets::status(
            ui,
            !reached.is_empty(),
            if reached.is_empty() { format!("a message to {} reaches no lamp: nobody subscribes to it", group_name(self.publish)) } else { format!("a message to {} reaches {} lamp(s): {}", group_name(self.publish), reached.len(), reached.join(", ")) },
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "regroup" => {
                self.subs[4][0] = true;
                self.press();
            }
            "all" => {
                self.publish = 0xFFFF;
                self.press();
            }
            "empty" => {
                for s in &mut self.subs {
                    s[1] = false;
                }
                self.publish = OFFICE_2;
            }
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "mesh-addressing",
        title: "Addresses, publish and subscribe",
        blurb: "A switch does not know which lamps it controls. It sends to a group, and the lamps decide whether they belong to it.",
        path: &["Wireless", "Bluetooth", "Bluetooth Mesh"],
        make: || Box::new(MeshAddressing::default()),
        explain: Explanation {
            what: "A wall switch and six lamps in a [[Bluetooth Mesh]] network. Each lamp has its own address, and ticks in the \
                table say which groups it belongs to. The switch has one setting: the address it publishes to.\n\n\
                Press the switch, and every lamp that accepts the message toggles. The names of those lamps turn green.",
            why: "In a building, the question \"which switch controls which lamps\" changes over the years: offices are split, \
                rooms merge, someone wants the corridor lights on the office switch. If the switch had a list of lamp \
                addresses inside, each change would mean reprogramming switches.\n\n\
                Bluetooth Mesh turns this round. Every [[element]] of a node - roughly, every independently controllable \
                part, such as one lamp - has a unicast address of its own, from 0x0001 to 0x7FFF. In addition there are \
                group addresses, from 0xC000 upwards, which belong to nobody. A device publishes its messages to one \
                address, usually a group. A device subscribes to any number of addresses, and accepts every message sent to \
                them.\n\n\
                So the switch publishes \"toggle\" to the group \"Office 1\" and knows nothing else. Which lamps react is decided \
                entirely on the lamps' side, by their subscription lists. Moving the corridor lamp onto the office switch \
                means adding one subscription to that lamp; the switch is not touched.\n\n\
                Because every message is relayed through the whole network anyway (see the flooding demo), sending to a \
                group costs no more than sending to one lamp. One message switches ten lamps at once, and they switch \
                together rather than one after another.\n\n\
                A few group addresses are fixed: 0xFFFF reaches every node, and others reach all relays or all friend \
                nodes. Virtual addresses, 0x8000 to 0xBFFF, are short stand-ins for long 128-bit labels.",
            formulas: &[
                ("accepted if: destination = own address, or destination in the subscription list, or destination = 0xFFFF", "The switch publishes to 0xC001; lamps 0x0005 and 0x0006 subscribe to it and toggle."),
                ("unicast 0x0001-0x7FFF, virtual 0x8000-0xBFFF, group 0xC000-0xFFFF", "Bit 15 = 0: unicast. Bits 15 and 14 = 10: virtual. Bits 15 and 14 = 11: group."),
            ],
            try_this: &[
                Experiment {
                    action: "Subscribe the corridor lamp to \"Office 1\" as well, and press the switch.",
                    watch: "three lamps toggling now. The switch was not changed at all; one tick in the corridor lamp's list did it.",
                    preset: Some("regroup"),
                },
                Experiment {
                    action: "Publish to 0xFFFF, all-nodes.",
                    watch: "every lamp toggling, whatever its subscriptions.",
                    preset: Some("all"),
                },
                Experiment {
                    action: "Remove every subscription to \"Office 2\" and publish to it.",
                    watch: "the message reaching nobody. Nothing tells the switch; it does not know who was meant to listen.",
                    preset: Some("empty"),
                },
            ],
            leaves_out: "Publishing and subscribing really belong to models inside an element (for example the Generic OnOff Server \
                of a lamp), not to the whole node, and are set up by a configuration client during commissioning. A toggle \
                is modelled here as each lamp inverting its own state; the real Generic OnOff message carries the new state, \
                \"on\" or \"off\", so that all lamps end up the same. Messages also carry a key index: a lamp only accepts a \
                message encrypted with an application key it was given.",
            refs: &["Bluetooth Mesh Protocol 1.1, section 3.4.2 and Table 3.7: unassigned 0x0000, unicast 0x0001-0x7FFF, virtual 0x8000-0xBFFF, group 0xC000-0xFFFF, fixed group addresses 0xFFF9-0xFFFF (all-proxies 0xFFFC, all-friends 0xFFFD, all-relays 0xFFFE, all-nodes 0xFFFF)."],
        },
    }
}
