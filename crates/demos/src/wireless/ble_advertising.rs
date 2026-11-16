use crate::prelude::*;
use egui_plot::{Points, Polygon};
use sim_core::ble::{AdvScanParams, simulate_adv_scan};

const DURATION_MS: f64 = 1500.0;

pub struct BleAdvertising {
    adv_interval_ms: f64,
    scan_interval_ms: f64,
    scan_window_ms: f64,
    scan_offset_ms: f64,
    data_bytes: usize,
    seed: u64,
}

impl Default for BleAdvertising {
    fn default() -> Self {
        Self { adv_interval_ms: 100.0, scan_interval_ms: 300.0, scan_window_ms: 100.0, scan_offset_ms: 40.0, data_bytes: 20, seed: 3 }
    }
}

fn lane(channel: u8) -> f64 {
    (40 - channel) as f64 // 37 -> 3, 38 -> 2, 39 -> 1
}

fn lane_label(mark: egui_plot::GridMark, _range: &std::ops::RangeInclusive<f64>) -> String {
    match mark.value {
        v if (v - 3.0).abs() < 0.01 => "ch 37".into(),
        v if (v - 2.0).abs() < 0.01 => "ch 38".into(),
        v if (v - 1.0).abs() < 0.01 => "ch 39".into(),
        _ => String::new(),
    }
}

impl Demo for BleAdvertising {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        self.scan_window_ms = self.scan_window_ms.min(self.scan_interval_ms);
        let packet_ms = (1 + 4 + 2 + 6 + self.data_bytes + 3) as f64 * 8.0 / 1000.0;

        widgets::group(ui, "Advertiser (for example a sensor on a coin cell)", |ui| {
            ui.add(widgets::si_slider(&mut self.adv_interval_ms, 20.0..=1000.0, "advertising interval", "ms").logarithmic(true).custom_formatter(|v, _| format!("{v:.0} ms")))
                .on_hover_text("Time between two advertising events. Allowed: 20 ms to about 10 s. Shorter = found faster, but the battery drains faster.");
            ui.add(egui::Slider::new(&mut self.data_bytes, 0..=31).text("bytes of advertising data"))
                .on_hover_text("More data makes each packet longer on air.");
        });
        widgets::group(ui, "Scanner (for example a phone or a gateway)", |ui| {
            ui.add(egui::Slider::new(&mut self.scan_interval_ms, 20.0..=1000.0).logarithmic(true).text("scan interval").suffix(" ms").fixed_decimals(0))
                .on_hover_text("How often the scanner starts listening. It moves to the next advertising channel each time.");
            ui.add(egui::Slider::new(&mut self.scan_window_ms, 0.0..=self.scan_interval_ms).text("scan window").suffix(" ms").fixed_decimals(0))
                .on_hover_text("How long it listens each time. Window = interval means listening all the time, which only mains-powered devices can afford.");
            ui.horizontal(|ui| {
                ui.add(egui::Slider::new(&mut self.scan_offset_ms, 0.0..=300.0).text("scanner starts later by").suffix(" ms").fixed_decimals(0))
                    .on_hover_text("The two devices do not share a clock. This slides the scanner's schedule against the advertiser's.");
                if ui.button("Roll again").on_hover_text("New random delays (the 0 to 10 ms the advertiser adds to every interval)").clicked() {
                    self.seed += 1;
                }
            });
        });

        let params = AdvScanParams {
            adv_interval_ms: self.adv_interval_ms,
            packet_ms,
            channel_gap_ms: 0.4,
            scan_interval_ms: self.scan_interval_ms,
            scan_window_ms: self.scan_window_ms,
            scan_offset_ms: self.scan_offset_ms,
            duration_ms: DURATION_MS,
            seed: self.seed,
        };
        let trace = simulate_adv_scan(&params);

        ui.label(RichText::new("1.5 seconds on the three advertising channels").strong());
        plots::plot("timeline", 230.0)
            .x_axis_label("time [ms]")
            .y_axis_formatter(lane_label)
            .include_y(0.4)
            .include_y(3.6)
            .include_x(0.0)
            .include_x(DURATION_MS)
            .y_grid_spacer(egui_plot::uniform_grid_spacer(|_| [1.0, 1.0, 1.0]))
            .show(ui, |p| {
                for w in &trace.windows {
                    let y = lane(w.channel);
                    p.polygon(Polygon::new("scanner listening", plots::bar(w.start_ms.max(0.0), w.end_ms, y - 0.38, y + 0.38)).fill_color(pal.trace[1].gamma_multiply(0.35)).stroke((1.0, pal.trace[1])));
                }
                let pts = |heard: bool| -> Vec<[f64; 2]> { trace.packets.iter().filter(|x| x.heard == heard).map(|x| [x.start_ms, lane(x.channel)]).collect() };
                p.points(Points::new("packet sent, nobody listening", pts(false)).radius(3.0).color(pal.dim));
                p.points(Points::new("packet received", pts(true)).radius(6.0).color(pal.good).shape(egui_plot::MarkerShape::Diamond));
            });

        ui.label(RichText::new("One advertising event, zoomed in: the same packet three times").strong());
        plots::plot("event", 150.0).x_axis_label("time [ms]").y_axis_formatter(lane_label).include_y(0.4).include_y(3.6).include_x(-0.2).include_x(3.5).y_grid_spacer(egui_plot::uniform_grid_spacer(|_| [1.0, 1.0, 1.0])).show(ui, |p| {
            for x in trace.packets.iter().filter(|x| x.event == 0) {
                let y = lane(x.channel);
                p.polygon(Polygon::new("packet on air", plots::bar(x.start_ms, x.end_ms, y - 0.3, y + 0.3)).fill_color(pal.trace[0].gamma_multiply(0.6)).stroke((1.0, pal.trace[0])));
            }
        });

        let events = trace.packets.iter().map(|p| p.event).max().map_or(0, |e| e + 1);
        let heard_events = (0..events).filter(|e| trace.packets.iter().any(|p| p.event == *e && p.heard)).count();
        match trace.first_heard_ms {
            Some(t) => widgets::status(ui, true, format!("the scanner first hears the advertiser after {t:.0} ms  ({heard_events} of {events} events heard)")),
            None => widgets::status(ui, false, format!("not discovered within {DURATION_MS:.0} ms: no packet fell completely inside a listening window on its channel")),
        }
        // Battery estimate with round, typical numbers.
        let tx_ma = 6.0;
        let sleep_ua = 2.0;
        let on_share = 3.0 * (packet_ms + 0.4) / (self.adv_interval_ms + 5.0);
        let avg_ua = tx_ma * 1000.0 * on_share + sleep_ua;
        widgets::formula(
            ui,
            format!(
                "packet on air: {packet_ms:.3} ms      advertiser radio on: {:.2} % of the time\n\
                 rough battery estimate ({tx_ma} mA while sending, {sleep_ua} µA asleep): average {avg_ua:.0} µA  ->  a 220 mAh coin cell lasts about {:.0} days\n\
                 scanner listening: {:.0} % of the time",
                on_share * 100.0,
                220_000.0 / avg_ua / 24.0,
                100.0 * self.scan_window_ms / self.scan_interval_ms
            ),
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        match preset {
            "continuous" => {
                self.scan_interval_ms = 100.0;
                self.scan_window_ms = 100.0;
            }
            "slow" => self.adv_interval_ms = 1000.0,
            "narrow" => {
                self.scan_interval_ms = 1000.0;
                self.scan_window_ms = 30.0;
            }
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "ble-advertising",
        title: "Advertising and scanning in time",
        blurb: "Two devices with no shared clock have to meet on the same channel at the same moment. See why discovery takes a while.",
        path: &["Wireless", "Bluetooth", "Bluetooth LE"],
        make: || Box::new(BleAdvertising::default()),
        explain: Explanation {
            what: "The big graph is a second and a half of radio time on the three [[advertising]] [[channels]]. Each dot is one \
                packet from the advertiser. Each orange block is a stretch of time during which the scanner's receiver is switched \
                on, on that one channel. A green diamond is a packet that landed inside a block: only those are received.\n\n\
                The small graph zooms into a single advertising event, where the packets are wide enough to see.",
            why: "Before two Bluetooth LE devices know each other they have no way to agree on a time or a channel. The solution \
                is brute force with rules. The advertiser sends the same [[packet]] on three fixed channels, one after the other - \
                that is one advertising event - and repeats the event at a steady interval. The scanner listens on one of the three \
                channels for a while, then on the next.\n\n\
                [[Scanning]] costs power the whole time the receiver is on, and advertising costs power for every packet, so both \
                sides do as little as they can get away with. A battery-powered sensor might advertise once a second. A phone in \
                the background might listen 30 ms out of every second. Discovery then becomes a matter of luck: a packet has to \
                fall completely inside a listening window.\n\n\
                There is one more trick. If both sides used exact intervals and happened to be out of step, they could miss each \
                other forever. So the advertiser adds a random delay of 0 to 10 ms to every interval. That makes its packets drift \
                relative to the scanner's windows, and sooner or later one lands.\n\n\
                Three channels are used instead of one so that interference on one (Wi-Fi sits between them) does not make a device \
                invisible.",
            formulas: &[
                ("time between events = advInterval + advDelay", "advInterval: 20 ms to about 10 s in steps of 0.625 ms. advDelay: random, 0 to 10 ms, new for every event."),
                ("packet time = 8 µs x bytes on air", "At 1 Mbit/s. A full legacy advertising packet (47 bytes) takes 376 µs."),
            ],
            try_this: &[
                Experiment {
                    action: "Make the scan window as long as the scan interval.",
                    watch: "the orange blocks joining up and the very first event being received. This is what a mains-powered gateway does.",
                    preset: Some("continuous"),
                },
                Experiment {
                    action: "Set the advertising interval to 1000 ms.",
                    watch: "the battery estimate jumping from weeks to years, and discovery taking much longer - often it does not happen within the graph.",
                    preset: Some("slow"),
                },
                Experiment {
                    action: "Give the scanner a short 30 ms window every 1000 ms, then press \"Roll again\" several times.",
                    watch: "how discovery now depends on luck. Sometimes one event hits, often none does.",
                    preset: Some("narrow"),
                },
            ],
            leaves_out: "A packet counts as received whenever it falls inside a listening window: range, [[noise]] and collisions with \
                other devices are ignored. Real scanners can also ask the advertiser for more data (a scan request), and Bluetooth 5 \
                adds extended advertising on the other 37 channels. The battery figure is a rough estimate from round numbers, not a \
                measurement of any real chip.",
            refs: &["Bluetooth Core 5.4, Vol 6 Part B 4.4.2.2.1 (advertising interval and advDelay), 4.4.3 (scanning)."],
        },
    }
}
