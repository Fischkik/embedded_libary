use crate::prelude::*;
use egui_plot::{MarkerShape, Points, Polygon};
use sim_core::mesh::{Friendship, RECEIVE_DELAY_MS, RECEIVE_WINDOW_MS};

const WINDOW_S: f64 = 60.0;

pub struct MeshFriend {
    f: Friendship,
    capacity_mah: f64,
}

impl Default for MeshFriend {
    fn default() -> Self {
        Self { f: Friendship { poll_interval_s: 10.0, receive_delay_ms: 100.0, receive_window_ms: 50.0, messages_per_hour: 120.0, tx_a: 7.1e-3, rx_a: 6.5e-3, sleep_a: 1.6e-6, poll_tx_s: 0.5e-3 }, capacity_mah: 225.0 }
    }
}

fn human(seconds: f64) -> String {
    const DAY: f64 = 86_400.0;
    match seconds {
        s if s < 2.0 * DAY => format!("{:.0} hours", s / 3600.0),
        s if s < 730.0 * DAY => format!("{:.0} days", s / DAY),
        s => format!("{:.1} years", s / (365.25 * DAY)),
    }
}

impl Demo for MeshFriend {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        let f = &mut self.f;
        widgets::group(ui, "A battery sensor and its friend", |ui| {
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "poll", |ui| ui.add(egui::Slider::new(&mut f.poll_interval_s, 1.0..=600.0).logarithmic(true).text("the sensor asks every").suffix(" s").fixed_decimals(0)).on_hover_text("How often the low power node wakes up and sends a Friend Poll: \"anything for me?\"."));
                widgets::wrap(ui, "msgs", |ui| ui.add(egui::Slider::new(&mut f.messages_per_hour, 0.0..=600.0).text("messages for it per hour").fixed_decimals(0)).on_hover_text("How often something in the network sends the sensor a message, for example a new setting."));
            });
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "delay", |ui| ui.add(egui::Slider::new(&mut f.receive_delay_ms, RECEIVE_DELAY_MS.0..=RECEIVE_DELAY_MS.1).text("receive delay").suffix(" ms").fixed_decimals(0)).on_hover_text("The friend answers this long after the poll; the sensor sleeps in between. The specification allows 10 to 255 ms."));
                widgets::wrap(ui, "window", |ui| ui.add(egui::Slider::new(&mut f.receive_window_ms, RECEIVE_WINDOW_MS.0..=RECEIVE_WINDOW_MS.1).text("receive window").suffix(" ms").fixed_decimals(0)).on_hover_text("How long the sensor keeps its receiver on for the answer. The specification allows 1 to 255 ms."));
            });
        });
        let f = self.f;

        ui.label(RichText::new("One minute: the friend stores, the sensor wakes up and collects").strong());
        let names = ["sensor's radio", "friend's queue", "messages arrive"].map(str::to_owned).to_vec();
        let msg_every = if f.messages_per_hour > 0.0 { 3600.0 / f.messages_per_hour } else { f64::INFINITY };
        let arrivals: Vec<f64> = (0..).map(|k| 3.0 + k as f64 * msg_every).take_while(|t| *t < WINDOW_S).collect();
        let polls: Vec<f64> = (0..).map(|k| 1.0 + k as f64 * f.poll_interval_s).take_while(|t| *t < WINDOW_S).collect();
        plots::timeline("friend", names, "time [s]", WINDOW_S).show(ui, |p| {
            p.points(Points::new("message", arrivals.iter().map(|t| [*t, 2.0]).collect::<Vec<_>>()).shape(MarkerShape::Down).radius(6.0).color(pal.trace[1]));
            // Queue length over time, drawn as a bar per message waiting.
            for &a in &arrivals {
                let pick = polls.iter().copied().find(|p| *p >= a).unwrap_or(WINDOW_S);
                let fill = pal.trace[1].gamma_multiply(0.35);
                p.polygon(Polygon::new("waiting", plots::bar(a, pick, 0.85, 1.15)).fill_color(fill).stroke((0.0, fill)));
            }
            for &t in &polls {
                let w = 0.25f64.max(f.receive_window_ms * 1e-3);
                p.polygon(Polygon::new("radio on", plots::bar(t, t + w, -0.3, 0.3)).fill_color(pal.trace[2]).stroke((0.0, pal.trace[2])));
            }
        });
        widgets::key(ui, &[(pal.trace[1], "a message for the sensor arrives; pale bar: it waits in the friend's queue"), (pal.trace[2], "the sensor wakes, polls and listens (drawn wider than it is)")]);

        let avg = f.average_a();
        widgets::formula(
            ui,
            format!(
                "per poll  = sending + window x {} = {} + {} x {} = {}\n\
                 average   = per poll / interval + sleep = {} / {} + {} = {}\n\
                 always listening (like a relay): {}, {:.0} times more\n\
                 a message waits on average: interval / 2 + receive delay = {}",
                si(f.rx_a, "A"),
                si(f.poll_tx_s * f.tx_a, "C"),
                si(f.receive_window_ms * 1e-3, "s"),
                si(f.rx_a, "A"),
                si(f.poll_charge_c(), "C"),
                si(f.poll_charge_c(), "C"),
                si(f.poll_interval_s, "s"),
                si(f.sleep_a, "A"),
                si(avg, "A"),
                si(f.always_listening_a(), "A"),
                f.always_listening_a() / avg,
                si(f.average_delay_s(), "s"),
            ),
        );
        let life = f.battery_life_s(self.capacity_mah);
        widgets::status(
            ui,
            avg * 10.0 < f.always_listening_a(),
            format!("on a 225 mAh coin cell: {} as a low power node, {} if it listened all the time", human(life), human(self.capacity_mah * 3.6 / f.always_listening_a())),
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "fast" => self.f.poll_interval_s = 1.0,
            "slow" => self.f.poll_interval_s = 300.0,
            "tight" => self.f.receive_window_ms = 5.0,
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "mesh-friend",
        title: "Friends and low power nodes",
        blurb: "A mesh only works if nodes listen all the time - which a coin cell cannot afford. A mains-powered friend listens on its behalf.",
        path: &["Wireless", "Bluetooth", "Bluetooth Mesh"],
        make: || Box::new(MeshFriend::default()),
        explain: Explanation {
            what: "A battery-powered sensor in a [[Bluetooth Mesh]] network, over one minute. The top row shows messages arriving \
                for it from the network. They are caught and stored by a [[friend node]] nearby, a lamp on mains power; the \
                pale bars in the middle row are the time each message waits there. The bottom row is the sensor's radio: \
                every few seconds it wakes up, asks the friend for its messages, and listens briefly for the answer.\n\n\
                The numbers below compare the current with that of a node that simply listens all the time.",
            why: "Mesh messages spread by flooding: every relay hears and repeats them. That only works because nodes keep their \
                receivers on, and a receiver that is always on draws several milliamperes without pause. On a coin cell \
                that lasts about a day and a half.\n\n\
                Battery devices therefore join the network differently. A [[low power node]] makes a deal with a neighbour \
                that has power to spare: the friend. From then on, the friend keeps every message addressed to the low \
                power node in a queue, and the low power node is free to sleep.\n\n\
                Every so often the low power node wakes up and sends a Friend Poll: \"anything for me?\". The friend waits a \
                fixed receive delay, so that the sleeper can turn its radio off meanwhile, and then sends the oldest stored \
                message, or an update saying the queue is empty. The low power node listens only during an agreed receive \
                window. Then it goes back to sleep.\n\n\
                The trade is time against energy. Each poll costs a few hundred microcoulombs, mostly listening; divided over \
                the poll interval, the average current drops by a factor of a hundred or more. But a message now waits in \
                the friend's queue until the next poll - for a light switch that is unacceptable, for a sensor's \
                configuration it does not matter. A sensor sending its own readings is not delayed at all: it can send \
                whenever it likes.",
            formulas: &[
                ("average current = (poll send + receive window x receive current) / poll interval + sleep current", "(0.5 ms x 7.1 mA + 50 ms x 6.5 mA) / 10 s + 1.6 µA = 34 µA, instead of 6.5 mA when always listening."),
                ("waiting time of a message = poll interval / 2 + receive delay, on average", "Polling every 10 s with a 100 ms delay: 5.1 s."),
            ],
            try_this: &[
                Experiment {
                    action: "Poll every second.",
                    watch: "messages hardly waiting at all, and the current rising tenfold to about 330 µA. Still twenty times better than listening, but the battery life drops to about a month.",
                    preset: Some("fast"),
                },
                Experiment {
                    action: "Poll only every 5 minutes.",
                    watch: "a battery life of many years - and messages piling up in the friend's queue, each waiting minutes to be collected.",
                    preset: Some("slow"),
                },
                Experiment {
                    action: "Shrink the receive window to 5 ms.",
                    watch: "the cost of each poll falling almost tenfold. The friend must then answer with very precise timing; the window and the delay are agreed when the friendship is set up.",
                    preset: Some("tight"),
                },
            ],
            leaves_out: "Only one message is fetched per poll here; a real low power node polls again at once while the friend says \
                more are queued. Setting up the friendship (offers from several possible friends, choosing one, agreeing on \
                keys and timing) is not shown, and neither is the poll timeout after which the friend gives up on a silent \
                node. The friend's queue has a limited size; when it is full, the oldest messages are dropped. The radio \
                currents are those of one particular chip (nRF52832); the 0.5 ms for sending a poll is an example value.",
            refs: &[
                "Bluetooth Mesh Protocol 1.1, 3.5.5 (Friend Queue: oldest entries sent first, oldest discarded when full), 3.6.5.1-3.6.5.2 (Friend Poll, Friend Update), Tables 3.37, 3.38, 3.41: ReceiveDelay 10-255 ms, ReceiveWindow 1-255 ms, PollTimeout 0x00000A-0x34BBFF x 100 ms.",
                "Radio currents: Nordic nRF52832 product specification (7.1 mA TX, 6.5 mA RX, 1.6 µA idle with RTC).",
            ],
        },
    }
}
