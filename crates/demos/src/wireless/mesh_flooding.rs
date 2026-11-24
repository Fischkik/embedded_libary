use crate::prelude::*;
use sim_core::mesh::{FloodNode, FloodParams, FloodResult, Reaction, flood};

const WORLD_W: f32 = 40.0;
const WORLD_H: f32 = 24.0;
/// Simulated milliseconds per real second at speed 1.
const MS_PER_SECOND: f64 = 25.0;
/// How long the ring of one transmission takes to spread, purely for looks.
const RING_MS: f64 = 12.0;

struct Node {
    key: u32,
    pos: Pos2,
    relay: bool,
}

pub struct MeshFlooding {
    nodes: Vec<Node>,
    src: u32,
    dst: Option<u32>,
    range: f64,
    ttl: u8,
    use_cache: bool,
    seed: u64,
    result: FloodResult,
    clock: Clock,
    next_key: u32,
    dirty: bool,
}

impl Default for MeshFlooding {
    fn default() -> Self {
        let layout: [(f32, f32); 12] =
            [(4.0, 12.0), (10.0, 7.0), (10.0, 17.0), (16.0, 12.0), (21.0, 7.0), (22.0, 18.0), (27.0, 11.5), (32.0, 6.0), (32.0, 17.5), (37.0, 12.0), (16.0, 21.0), (26.0, 21.5)];
        let nodes: Vec<Node> = layout.iter().enumerate().map(|(i, (x, y))| Node { key: i as u32, pos: pos2(*x, *y), relay: true }).collect();
        let mut d = Self {
            nodes,
            src: 0,
            dst: Some(9),
            range: 8.0,
            ttl: 7,
            use_cache: true,
            seed: 1,
            result: FloodResult::default(),
            clock: Clock { running: true, speed: 1.0, t: 0.0 },
            next_key: 12,
            dirty: false,
        };
        d.send();
        d
    }
}

impl MeshFlooding {
    fn index(&self, key: u32) -> Option<usize> {
        self.nodes.iter().position(|n| n.key == key)
    }

    fn params(&self) -> FloodParams {
        FloodParams { range: self.range, ttl: self.ttl, src: self.index(self.src).unwrap_or(0), dst: self.dst.and_then(|k| self.index(k)), use_cache: self.use_cache, seed: self.seed }
    }

    fn simulate(&mut self) {
        let nodes: Vec<FloodNode> = self.nodes.iter().map(|n| FloodNode { x: n.pos.x as f64, y: n.pos.y as f64, relay: n.relay }).collect();
        self.result = flood(&nodes, &self.params());
    }

    fn end_ms(&self) -> f64 {
        self.result.transmissions.last().map_or(0.0, |t| t.t_ms) + RING_MS + 5.0
    }

    /// Send the message again and replay it from the start.
    fn send(&mut self) {
        self.simulate();
        self.clock.t = 0.0;
        self.clock.running = true;
    }

    /// Mesh unicast address of a node: 0x0001 upwards.
    fn address(&self, index: usize) -> u16 {
        self.nodes[index].key as u16 + 1
    }
}

impl Demo for MeshFlooding {
    fn ui(&mut self, ui: &mut Ui, cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        if self.src_missing() {
            self.src = self.nodes.first().map_or(0, |n| n.key);
        }
        if self.dirty {
            // The network changed: recompute and show the finished result while editing.
            self.simulate();
            self.clock.t = self.end_ms() / MS_PER_SECOND;
            self.dirty = false;
        }
        self.clock.tick(ui, cx.dt);
        let now = self.clock.t * MS_PER_SECOND;
        if now > self.end_ms() {
            self.clock.running = false;
        }

        widgets::group(ui, "The message", |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Send the message").on_hover_text("Send one message from the sender and replay how it spreads").clicked() {
                    self.send();
                }
                let before = (self.ttl, self.range, self.use_cache);
                widgets::wrap(ui, "TTL", |ui| {
                    ui.add(egui::Slider::new(&mut self.ttl, 0..=10).text("TTL"))
                        .on_hover_text("Time to live: how many more times the message may be passed on. Every relay lowers it by one.")
                });
                widgets::wrap(ui, "radio range", |ui| {
                    ui.add(egui::Slider::new(&mut self.range, 3.0..=16.0).text("radio range").suffix(" m").fixed_decimals(1))
                        .on_hover_text("How far a transmission can be heard. The faint lines connect nodes that can hear each other.")
                });
                ui.checkbox(&mut self.use_cache, "nodes remember what they have seen").on_hover_text("The network message cache. Switch it off to see why it is essential.");
                if before != (self.ttl, self.range, self.use_cache) {
                    self.dirty = true;
                }
            });
            ui.label(RichText::new("Drag nodes. Click empty space to add a node. Right-click a node to make it the sender or destination, switch its relay on or off, or remove it.").small().color(pal.dim));
        });
        ui.horizontal(|ui| {
            self.clock.controls(ui);
            ui.label(format!("t = {:.0} ms", now.min(self.end_ms())));
        });

        // Which nodes have the message by now, and who is transmitting.
        let mut has = vec![false; self.nodes.len()];
        for tx in self.result.transmissions.iter().filter(|t| t.t_ms <= now) {
            if let Some(h) = has.get_mut(tx.from) {
                *h = true;
            }
            if now - tx.t_ms >= RING_MS * 0.6 {
                for (i, r) in &tx.heard {
                    if *r != Reaction::Duplicate || !self.use_cache {
                        has[*i] = true;
                    }
                }
            }
        }

        let world = egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(WORLD_W, WORLD_H));
        enum Edit {
            Sender(u32),
            Destination(u32),
            ToggleRelay(usize),
            Remove(usize),
            Add(Pos2),
        }
        let mut edit = None;
        let mut moved = false;
        let (src_i, dst_i) = (self.index(self.src), self.dst.and_then(|k| self.index(k)));
        scene::scene(ui, "mesh", world, 520.0, |s| {
            // Who can hear whom.
            for a in 0..self.nodes.len() {
                for b in a + 1..self.nodes.len() {
                    if self.nodes[a].pos.distance(self.nodes[b].pos) as f64 <= self.range {
                        s.painter.line_segment([s.pos(self.nodes[a].pos), s.pos(self.nodes[b].pos)], (1.0, pal.grid));
                    }
                }
            }
            // Transmissions in progress: an expanding ring, then lines to whoever heard it.
            for tx in &self.result.transmissions {
                let age = now - tx.t_ms;
                if !(0.0..RING_MS * 2.5).contains(&age) || tx.from >= self.nodes.len() {
                    continue;
                }
                let from = s.pos(self.nodes[tx.from].pos);
                let grow = (age / RING_MS).min(1.0) as f32;
                let fade = (1.0 - (age / (RING_MS * 2.5)) as f32).clamp(0.0, 1.0);
                s.painter.circle_stroke(from, s.len(self.range as f32 * grow), (2.0, pal.trace[1].gamma_multiply(fade)));
                if grow >= 0.6 {
                    for (i, reaction) in &tx.heard {
                        let color = match reaction {
                            Reaction::Duplicate => pal.dim,
                            Reaction::Delivered => pal.good,
                            Reaction::WillRelay => pal.trace[1],
                            Reaction::TtlExhausted | Reaction::NotARelay => pal.bad,
                        };
                        s.painter.line_segment([from, s.pos(self.nodes[*i].pos)], (2.0, color.gamma_multiply(fade)));
                    }
                }
            }
            for (i, n) in self.nodes.iter_mut().enumerate() {
                let r = s.handle(("node", n.key), &mut n.pos, 11.0);
                moved |= r.dragged();
                let c = s.pos(n.pos);
                let fill = if Some(i) == src_i {
                    pal.trace[1]
                } else if Some(i) == dst_i {
                    if has[i] { pal.good } else { pal.trace[2].gamma_multiply(0.45) }
                } else if has[i] {
                    pal.trace[0]
                } else {
                    pal.canvas
                };
                s.painter.circle_filled(c, 10.0, fill);
                s.painter.circle_stroke(c, 10.0, (if n.relay { 2.0 } else { 1.0 }, if n.relay { pal.text } else { pal.dim }));
                let tag = if Some(i) == src_i {
                    "sender".to_owned()
                } else if Some(i) == dst_i {
                    "destination".to_owned()
                } else if n.relay {
                    format!("{}", n.key + 1)
                } else {
                    format!("{} (no relay)", n.key + 1)
                };
                s.painter.text(c + vec2(0.0, -14.0), egui::Align2::CENTER_BOTTOM, tag, egui::FontId::proportional(13.0), pal.text);
                let (key, relay) = (n.key, n.relay);
                r.context_menu(|ui| {
                    if ui.button("Make this the sender").clicked() {
                        edit = Some(Edit::Sender(key));
                        ui.close();
                    }
                    if ui.button("Make this the destination").clicked() {
                        edit = Some(Edit::Destination(key));
                        ui.close();
                    }
                    if ui.button(if relay { "Switch relay off" } else { "Switch relay on" }).clicked() {
                        edit = Some(Edit::ToggleRelay(i));
                        ui.close();
                    }
                    if ui.button("Remove this node").clicked() {
                        edit = Some(Edit::Remove(i));
                        ui.close();
                    }
                });
            }
            if let Some(p) = s.background_click() {
                edit = Some(Edit::Add(p));
            }
        });
        let changed = edit.is_some() || moved;
        match edit {
            Some(Edit::Sender(k)) => self.src = k,
            Some(Edit::Destination(k)) => self.dst = Some(k),
            Some(Edit::ToggleRelay(i)) => self.nodes[i].relay = !self.nodes[i].relay,
            Some(Edit::Remove(i)) if self.nodes.len() > 2 => {
                let key = self.nodes.remove(i).key;
                if self.dst == Some(key) {
                    self.dst = None;
                }
            }
            Some(Edit::Add(p)) if self.nodes.len() < 40 => {
                self.nodes.push(Node { key: self.next_key, pos: p, relay: true });
                self.next_key += 1;
            }
            _ => {}
        }
        if changed {
            self.dirty = true;
        }

        // Outcome.
        let params = self.params();
        let reached = self.result.reached.iter().filter(|r| **r).count();
        let finished = now >= self.end_ms() - RING_MS;
        if finished {
            match (params.dst, self.result.delivered_ms) {
                (Some(d), Some(t)) => {
                    let hops = self.result.transmissions.iter().find(|tx| tx.heard.contains(&(d, Reaction::Delivered))).map_or(0, |tx| params.ttl - tx.ttl + 1);
                    widgets::status(ui, true, format!("delivered after {t:.0} ms and {hops} hop(s), using {} transmissions in total", self.result.transmissions.len()));
                }
                (Some(_), None) => widgets::status(ui, false, format!("not delivered: the message reached {reached} node(s) and died out ({} transmissions)", self.result.transmissions.len())),
                (None, _) => widgets::status(ui, reached + 1 >= self.nodes.len(), format!("no destination set: the message reached {reached} of {} other nodes", self.nodes.len() - 1)),
            }
            if self.result.truncated {
                widgets::status(ui, false, "stopped after 400 transmissions: without the cache the same message is relayed back and forth until the TTL runs out");
            }
        }

        widgets::group(ui, "Every transmission, in order", |ui| {
            egui::ScrollArea::vertical().id_salt("tx_list").max_height(170.0).show(ui, |ui| {
                egui::Grid::new("tx").striped(true).num_columns(5).show(ui, |ui| {
                    for h in ["time", "sent by", "TTL", "heard by", ""] {
                        ui.label(RichText::new(h).strong());
                    }
                    ui.end_row();
                    for (n, tx) in self.result.transmissions.iter().enumerate().take(60) {
                        if tx.from >= self.nodes.len() {
                            continue;
                        }
                        let fresh = tx.heard.iter().filter(|(_, r)| *r != Reaction::Duplicate).count();
                        ui.monospace(format!("{:.0} ms", tx.t_ms));
                        ui.monospace(if Some(tx.from) == src_i { "sender".to_owned() } else { format!("node {}", self.nodes[tx.from].key + 1) });
                        ui.monospace(format!("{}", tx.ttl));
                        ui.label(format!("{} node(s), {} of them for the first time", tx.heard.len(), fresh));
                        if ui.small_button("Inspect packet").on_hover_text("Open this transmission in the Mesh packet demo").clicked() {
                            let src = self.address(params.src);
                            let dst = params.dst.map_or(0xFFFF, |d| self.address(d));
                            cx.open = Some(("mesh-pdu", vec![0x00, tx.ttl, (src >> 8) as u8, src as u8, (dst >> 8) as u8, dst as u8]));
                        }
                        ui.end_row();
                        let _ = n;
                    }
                });
            });
        });
    }

    fn apply_preset(&mut self, preset: &str) {
        match preset {
            "low-ttl" => self.ttl = 3,
            "no-cache" => {
                self.use_cache = false;
                self.ttl = 6;
            }
            "broken-relay" => {
                for k in [3usize, 4, 5] {
                    self.nodes[k].relay = false;
                }
            }
            _ => {}
        }
        self.send();
    }
}

impl MeshFlooding {
    fn src_missing(&self) -> bool {
        self.index(self.src).is_none()
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "mesh-flooding",
        title: "Bluetooth Mesh: how a message spreads",
        blurb: "Build a mesh network on a floor plan, send a message, and watch it flood from node to node.",
        path: &["Wireless", "Bluetooth", "Bluetooth Mesh"],
        make: || Box::new(MeshFlooding::default()),
        explain: Explanation {
            what: "Each circle is a mesh node - think of a lamp, a switch or a sensor in a building about 40 m wide. Faint lines \
                join nodes that are within radio range of each other. Press \"Send the message\" and watch: an orange ring is one \
                node transmitting, and nodes turn blue once they have the message.\n\n\
                The lines that flash from a transmitter show what each listener did: orange means \"new to me, I will pass it on\", \
                grey means \"I have seen this already\", green is the destination receiving it, red means the message stopped there.",
            why: "The sender is far out of range of the destination. Bluetooth Mesh solves that without any routing tables or \
                planned paths: it uses [[flooding]]. The sender broadcasts the message as an ordinary [[advertising]] packet. Every \
                node that hears it and has the [[relay]] feature switched on simply broadcasts it again. Their neighbours do the \
                same, and the message spreads outwards like a rumour until it reaches the destination - usually along several \
                routes at once, which is what makes a mesh robust when a node fails.\n\n\
                Left like that, a rumour would circulate forever. Two rules stop it, and together they make it managed flooding.\n\n\
                First, every packet carries a [[TTL]], a counter. Each relay lowers it by one before sending the packet on, and a \
                packet that arrives with a TTL of 1 or 0 is not relayed any further. The TTL is the furthest a message can travel, \
                counted in [[hops]].\n\n\
                Second, every node keeps a [[message cache]]: a short list of the packets it has seen recently. A packet it has \
                already seen is dropped on the spot. That is why each node transmits once here even though it hears the message \
                from several neighbours.",
            formulas: &[
                ("relay only if TTL >= 2, and send with TTL - 1", "A packet received with TTL 3 goes out again with TTL 2, then 1, then stops."),
                ("TTL 0 = one hop only", "A special value meaning \"for my direct neighbours, never to be relayed\"."),
            ],
            try_this: &[
                Experiment {
                    action: "Set the TTL to 3 and send.",
                    watch: "the flood stopping partway (red lines) and the destination never turning green. The message ran out of hops.",
                    preset: Some("low-ttl"),
                },
                Experiment {
                    action: "Untick \"nodes remember what they have seen\" and send.",
                    watch: "the transmission count exploding. Without the cache, neighbours keep relaying the same message to each other until the TTL is used up.",
                    preset: Some("no-cache"),
                },
                Experiment {
                    action: "Right-click the three nodes in the middle and switch their relay off.",
                    watch: "the message reaching them but going no further. Then drag one relay node into the gap to bridge it.",
                    preset: Some("broken-relay"),
                },
                Experiment {
                    action: "Press \"Inspect packet\" on any row of the list.",
                    watch: "the Mesh packet demo opening with exactly that transmission: same sender, destination and TTL, shown down to the encrypted bytes.",
                    preset: None,
                },
            ],
            leaves_out: "Radio range here is a hard circle and every transmission inside it is heard; real links fade gradually and \
                packets collide. Each node sends the message once, where a real node repeats it several times on all three \
                advertising channels to make up for losses. Relays wait a random 5 to 25 ms. Sequence numbers, encryption and \
                group addresses are left to the next demo. Bluetooth Mesh 1.1 also offers directed forwarding, which sets up \
                paths instead of flooding.",
            refs: &["Mesh Protocol 1.1: 3.4.4.4 (TTL), 3.4.6.3 (relaying), 3.4.6.5 (network message cache), 3.3.1 (advertising bearer)."],
        },
    }
}
