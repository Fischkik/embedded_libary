use crate::prelude::*;
use egui_plot::{HLine, LineStyle, Points, Polygon};
use sim_core::ble::{Csa1, channel_mhz};

const EVENTS: usize = 80;

pub struct BleConnection {
    hop: u8,
    interval_ms: f64,
    /// Centre frequency of a Wi-Fi network nearby, if any.
    wifi_mhz: Option<u32>,
    /// Whether the connection has learned to avoid the blocked channels.
    adaptive: bool,
}

impl Default for BleConnection {
    fn default() -> Self {
        Self { hop: 7, interval_ms: 30.0, wifi_mhz: None, adaptive: false }
    }
}

impl BleConnection {
    fn blocked(&self, channel: u8) -> bool {
        self.wifi_mhz.is_some_and(|w| (channel_mhz(channel) as i32 - w as i32).abs() <= 10)
    }
}

impl Demo for BleConnection {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "The connection", |ui| {
            ui.add(egui::Slider::new(&mut self.hop, 5..=16).text("hop increment"))
                .on_hover_text("How many channels to move on for each connection event. Chosen at random (5 to 16) when the connection is set up.");
            ui.add(egui::Slider::new(&mut self.interval_ms, 7.5..=500.0).logarithmic(true).text("connection interval").suffix(" ms").fixed_decimals(1))
                .on_hover_text("Time between two meetings. Allowed: 7.5 ms to 4 s. Short = responsive, long = saves battery.");
        });
        widgets::group(ui, "The neighbourhood", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Wi-Fi network nearby:");
                ui.selectable_value(&mut self.wifi_mhz, None, "none");
                for (name, mhz) in [("channel 1", 2412), ("channel 6", 2437), ("channel 11", 2462)] {
                    ui.selectable_value(&mut self.wifi_mhz, Some(mhz), name).on_hover_text(format!("A Wi-Fi network about 20 MHz wide, centred on {mhz} MHz"));
                }
            });
            ui.checkbox(&mut self.adaptive, "The connection avoids channels it has found to be bad (channel map)")
                .on_hover_text("Both devices agree on a list of usable channels. Hops that would land on a bad channel are redirected to a good one.");
        });

        let mut used = [true; 37];
        if self.adaptive {
            (0..37u8).for_each(|c| used[c as usize] = !self.blocked(c));
        }
        let mut csa = Csa1::new(self.hop, used);
        let hops: Vec<_> = (0..EVENTS).map(|_| csa.next_hop()).collect();
        let lost = hops.iter().filter(|h| self.blocked(h.channel)).count();
        let span_ms = EVENTS as f64 * self.interval_ms;

        ui.label(RichText::new(format!("{EVENTS} connection events: one dot per meeting")).strong());
        plots::plot("hops", 330.0)
            .x_axis_label("time [ms]")
            .y_axis_label("frequency [MHz]")
            .include_y(2400.0)
            .include_y(2482.0)
            .include_x(0.0)
            .include_x(span_ms)
            .show(ui, |p| {
                if let Some(w) = self.wifi_mhz {
                    let (lo, hi) = (w as f64 - 10.0, w as f64 + 10.0);
                    p.polygon(Polygon::new("Wi-Fi network", plots::bar(0.0, span_ms, lo, hi)).fill_color(pal.bad.gamma_multiply(0.25)).stroke((1.0, pal.bad)));
                }
                for ch in [37u8, 38, 39] {
                    p.hline(HLine::new("advertising channels (never used by a connection)", channel_mhz(ch) as f64).color(pal.dim).style(LineStyle::dashed_loose()));
                }
                let pts = |bad: bool| -> Vec<[f64; 2]> {
                    hops.iter().enumerate().filter(|(_, h)| self.blocked(h.channel) == bad).map(|(k, h)| [k as f64 * self.interval_ms, channel_mhz(h.channel) as f64]).collect()
                };
                p.points(Points::new("packets got through", pts(false)).radius(4.0).color(pal.good));
                p.points(Points::new("packets lost to interference", pts(true)).radius(5.0).color(pal.bad).shape(egui_plot::MarkerShape::Cross));
                p.line(plots::line("hop sequence", hops.iter().enumerate().map(|(k, h)| [k as f64 * self.interval_ms, channel_mhz(h.channel) as f64]).collect(), pal.dim).width(0.6));
            });

        if lost == 0 {
            widgets::status(ui, true, format!("all {EVENTS} events got through"));
        } else {
            widgets::status(ui, false, format!("{lost} of {EVENTS} events lost ({:.0} %): that data has to wait for a later event and be sent again", 100.0 * lost as f64 / EVENTS as f64));
        }

        widgets::group(ui, "The first hops, step by step", |ui| {
            egui::Grid::new("hop_table").striped(true).num_columns(5).show(ui, |ui| {
                for h in ["event", "(last + hop) mod 37", "channel used", "frequency", ""] {
                    ui.label(RichText::new(h).strong());
                }
                ui.end_row();
                let mut last = 0u8;
                for (k, h) in hops.iter().take(8).enumerate() {
                    ui.monospace(format!("{k}"));
                    ui.monospace(format!("({last} + {}) mod 37 = {}", self.hop, h.unmapped));
                    ui.monospace(format!("{}", h.channel));
                    ui.monospace(format!("{} MHz", channel_mhz(h.channel)));
                    ui.label(if h.remapped { "redirected: that channel is marked bad" } else if self.blocked(h.channel) { "lost" } else { "" });
                    ui.end_row();
                    last = h.unmapped;
                }
            });
        });
        widgets::formula(ui, format!("usable channels: {} of 37      one full round of all channels takes {:.0} ms", used.iter().filter(|u| **u).count(), 37.0 * self.interval_ms));
    }

    fn apply_preset(&mut self, preset: &str) {
        match preset {
            "wifi" => self.wifi_mhz = Some(2437),
            "adaptive" => {
                self.wifi_mhz = Some(2437);
                self.adaptive = true;
            }
            "slow" => self.interval_ms = 400.0,
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "ble-connection",
        title: "A BLE connection: hopping between channels",
        blurb: "Connected devices meet briefly at agreed times, on a different channel each time. See how that dodges Wi-Fi.",
        path: &["Wireless", "Bluetooth", "Bluetooth LE"],
        make: || Box::new(BleConnection::default()),
        explain: Explanation {
            what: "Each dot is one [[connection event]]: a short moment in which two connected devices both switch their radios on, \
                exchange [[packets]], and go back to sleep. Left to right is time. Up and down is the radio [[frequency]] the meeting \
                takes place on. The dots jump around because the connection moves to a different [[channel]] for every meeting.\n\n\
                The dashed lines are the three advertising channels, which connections never use. A red band is a Wi-Fi network \
                transmitting nearby.",
            why: "Advertising is fine for shouting short messages into the room, but it wastes power when two devices want an \
                ongoing conversation. So they connect: one sends a packet that proposes a timetable, and from then on both know \
                exactly when the next meeting is and on which channel. Between meetings, both sleep. The time between meetings is \
                the [[connection interval]].\n\n\
                Why change channel every time? The 2.4 GHz band is shared with Wi-Fi, microwave ovens and every other Bluetooth \
                device. If a connection stayed on one channel and that channel was busy, it would be stuck. By [[hop]]ping, a bad \
                channel only spoils the occasional meeting, and whatever was lost is simply sent again at the next one.\n\n\
                The hopping rule is deliberately simple: add a fixed number, the hop increment, to the last channel, and wrap around \
                at 37. Both sides compute the same sequence, so no extra radio traffic is needed to stay together.\n\n\
                Better still, the devices can learn. When they notice that some channels keep failing, they agree on a channel map \
                that marks them as bad, and any hop that would land there is redirected to a good channel. This is called adaptive \
                frequency hopping.",
            formulas: &[
                ("channel = (last channel + hop increment) mod 37", "Channel Selection Algorithm #1. If the result is marked bad, it is replaced by an entry from the list of good channels."),
                ("connection interval: 7.5 ms to 4 s", "In steps of 1.25 ms. Chosen by the device that starts the connection, and it can be changed later."),
            ],
            try_this: &[
                Experiment {
                    action: "Switch on the Wi-Fi network on channel 6.",
                    watch: "red crosses wherever the hop sequence wanders into the red band - roughly a quarter of all meetings are lost.",
                    preset: Some("wifi"),
                },
                Experiment {
                    action: "Now tick the box that lets the connection avoid bad channels.",
                    watch: "the crosses disappearing: hops that would land in the band are redirected, and the table marks them as redirected.",
                    preset: Some("adaptive"),
                },
                Experiment {
                    action: "Change the hop increment.",
                    watch: "the zig-zag pattern changing its slope, while every usable channel is still visited equally often. 37 is a prime number, so any increment reaches all channels.",
                    preset: None,
                },
            ],
            leaves_out: "A meeting inside the Wi-Fi band always fails here; in reality it fails only when the Wi-Fi network happens \
                to transmit at that instant. Newer devices use a second, more random-looking hopping rule (Channel Selection \
                Algorithm #2). What is said during a meeting - which side speaks first, acknowledgements, how lost data is repeated - \
                is not shown.",
            refs: &["Bluetooth Core 5.4, Vol 6 Part B 4.5.8.2 (Channel Selection Algorithm #1), 4.5.1 (connection interval), Table 1.3 (channel frequencies)."],
        },
    }
}
