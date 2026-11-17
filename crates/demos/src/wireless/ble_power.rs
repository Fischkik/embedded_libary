use crate::prelude::*;
use egui_plot::{HLine, Points};
use sim_core::ble::AdvPower;

const INTERVAL_S: (f64, f64) = (0.02, 10.24);

pub struct BlePower {
    p: AdvPower,
    capacity_mah: f64,
}

impl Default for BlePower {
    fn default() -> Self {
        Self { p: AdvPower { interval_s: 0.1, data_bytes: 31, connectable: true, tx_a: 7.1e-3, rx_a: 6.5e-3, sleep_a: 1.6e-6, ramp_s: 140e-6 }, capacity_mah: 225.0 }
    }
}

/// "45 s", "2.3 days", "7.8 years".
fn human(seconds: f64) -> String {
    const DAY: f64 = 86_400.0;
    let (v, unit) = match seconds {
        s if s < 7200.0 => (s / 60.0, "min"),
        s if s < 2.0 * DAY => (s / 3600.0, "hours"),
        s if s < 730.0 * DAY => (s / DAY, "days"),
        s => (s / (365.25 * DAY), "years"),
    };
    if v >= 100.0 { format!("{v:.0} {unit}") } else { format!("{v:.1} {unit}") }
}

impl Demo for BlePower {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        let p = &mut self.p;
        widgets::group(ui, "A sensor that advertises on a coin cell", |ui| {
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "interval", |ui| {
                    ui.add(widgets::si_slider(&mut p.interval_s, INTERVAL_S.0..=INTERVAL_S.1, "advertising interval", "s").logarithmic(true)).on_hover_text("How often the sensor sends an advertising event. The specification allows 20 ms to 10.24 s.")
                });
                widgets::wrap(ui, "data", |ui| ui.add(egui::Slider::new(&mut p.data_bytes, 0..=31).text("advertising data").suffix(" bytes")).on_hover_text("How much data each packet carries: a name, a temperature, ... Legacy advertising allows up to 31 bytes."));
                ui.checkbox(&mut p.connectable, "connectable").on_hover_text("A connectable device listens briefly after each packet, in case a phone wants to connect. A pure beacon does not.");
            });
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "tx", |ui| ui.add(widgets::si_slider(&mut p.tx_a, 1e-3..=20e-3, "current while sending", "A").logarithmic(true)).on_hover_text("What the chip draws while its radio transmits. 7.1 mA is the nRF52832 at 0 dBm."));
                widgets::wrap(ui, "rx", |ui| ui.add(widgets::si_slider(&mut p.rx_a, 1e-3..=20e-3, "current while listening", "A").logarithmic(true)).on_hover_text("What the chip draws while its radio receives. 6.5 mA for the nRF52832."));
            });
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "sleep", |ui| ui.add(widgets::si_slider(&mut p.sleep_a, 0.1e-6..=20e-6, "current while asleep", "A").logarithmic(true)).on_hover_text("What the chip draws between events, with a slow clock running to wake it. 1.6 µA for the nRF52832."));
                widgets::wrap(ui, "ramp", |ui| ui.add(widgets::si_slider(&mut p.ramp_s, 0.0..=500e-6, "radio start-up per packet", "s")).on_hover_text("The time the radio needs to start before each packet, at roughly the receive current. Example value; see the chip's datasheet."));
                widgets::wrap(ui, "bat", |ui| ui.add(egui::Slider::new(&mut self.capacity_mah, 20.0..=3000.0).logarithmic(true).text("battery").suffix(" mAh").fixed_decimals(0)).on_hover_text("225 mAh is a CR2032 coin cell; two AA cells hold around 2500 mAh."));
            });
        });
        let p = self.p;

        // One advertising event as a current profile, in microseconds.
        ui.label(RichText::new("One advertising event: the radio's current over time").strong());
        let mut prof = vec![[-100.0, p.sleep_a * 1e3], [0.0, p.sleep_a * 1e3]];
        let mut t = 0.0;
        for _ in 0..3 {
            for (dur, amps) in [(p.ramp_s, p.rx_a), (p.packet_s(), p.tx_a), (p.listen_s(), p.rx_a)] {
                if dur > 0.0 {
                    prof.extend([[t * 1e6, amps * 1e3], [(t + dur) * 1e6, amps * 1e3]]);
                    t += dur;
                }
            }
        }
        prof.extend([[t * 1e6, p.sleep_a * 1e3], [t * 1e6 + 200.0, p.sleep_a * 1e3]]);
        plots::plot_bare("event", 170.0).x_axis_label("time [µs]").y_axis_label("current [mA]").include_y(0.0).include_y(p.tx_a.max(p.rx_a) * 1.15e3).show(ui, |pl| {
            pl.line(plots::line("current", prof, pal.trace[0]).width(2.0));
            pl.hline(HLine::new("send", p.tx_a * 1e3).color(pal.trace[3]).style(egui_plot::LineStyle::dashed_dense()));
        });
        widgets::key(ui, &[(pal.trace[0], "current: start-up, send, listen - on channels 37, 38 and 39 in turn"), (pal.trace[3], "dashed: the sending current")]);

        ui.label(RichText::new("Battery life against the advertising interval").strong());
        let (a, b) = (INTERVAL_S.0.log10(), INTERVAL_S.1.log10());
        let life = |q: AdvPower| (q.battery_life_s(self.capacity_mah) / 86_400.0).log10();
        let curve = |q: AdvPower| -> Vec<[f64; 2]> { (0..=150).map(|i| a + (b - a) * i as f64 / 150.0).map(|x| [x, life(AdvPower { interval_s: 10f64.powf(x), ..q })]).collect() };
        let pow = |unit: &'static str, days: bool| {
            move |mark: egui_plot::GridMark, _: &std::ops::RangeInclusive<f64>| {
                let i = mark.value.round();
                if (mark.value - i).abs() > 0.01 {
                    String::new()
                } else if days {
                    human(10f64.powf(i) * 86_400.0)
                } else {
                    si(10f64.powf(i), unit)
                }
            }
        };
        plots::plot_bare("life", 220.0)
            .x_axis_label("advertising interval")
            .y_axis_label("battery life")
            .x_grid_spacer(egui_plot::uniform_grid_spacer(|_| [1.0, 1.0, 1.0]))
            .y_grid_spacer(egui_plot::uniform_grid_spacer(|_| [1.0, 1.0, 1.0]))
            .x_axis_formatter(pow("s", false))
            .y_axis_formatter(pow("", true))
            .show(ui, |pl| {
                pl.line(plots::line("only the sleep current", vec![[a, (self.capacity_mah * 3.6 / p.sleep_a / 86_400.0).log10()], [b, (self.capacity_mah * 3.6 / p.sleep_a / 86_400.0).log10()]], pal.dim).style(egui_plot::LineStyle::dashed_dense()));
                pl.line(plots::line("this sensor", curve(p), pal.trace[0]).width(2.5));
                pl.points(Points::new("now", vec![[p.interval_s.log10(), life(p)]]).radius(6.0).color(pal.text));
            });
        widgets::key(ui, &[(pal.trace[0], "battery life of this sensor"), (pal.dim, "dashed: the limit set by the sleep current alone"), (pal.text, "dot: the current setting")]);

        let avg = p.average_a();
        let share = p.sleep_a / avg;
        widgets::formula(
            ui,
            format!(
                "packet on air  = (16 + {} data bytes) x 8 µs = {}\n\
                 event charge   = 3 x (start-up x {} + packet x {} + listen x {}) = {}\n\
                 average        = event charge / (interval + 5 ms) + sleep = {} / {} + {} = {}\n\
                 battery life   = {} / {} = {}   (sleeping: {:.0} % of the drain)",
                p.data_bytes,
                si(p.packet_s(), "s"),
                si(p.rx_a, "A"),
                si(p.tx_a, "A"),
                si(p.rx_a, "A"),
                si(p.event_charge_c(), "C"),
                si(p.event_charge_c(), "C"),
                si(p.interval_s + 5e-3, "s"),
                si(p.sleep_a, "A"),
                si(avg, "A"),
                si(self.capacity_mah * 1e-3, "Ah"),
                si(avg, "A"),
                human(p.battery_life_s(self.capacity_mah)),
                share * 100.0,
            ),
        );
        let years = p.battery_life_s(self.capacity_mah) / (365.25 * 86_400.0);
        widgets::status(ui, years >= 1.0, if years >= 1.0 { format!("the battery lasts {}", human(p.battery_life_s(self.capacity_mah))) } else { format!("the battery is empty after {}", human(p.battery_life_s(self.capacity_mah))) });
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "second" => self.p.interval_s = 1.0,
            "beacon" => (self.p.interval_s, self.p.connectable, self.p.data_bytes) = (1.0, false, 20),
            "slowest" => self.p.interval_s = 10.24,
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "ble-power",
        title: "Power: how long does the battery last?",
        blurb: "A Bluetooth sensor spends almost all its energy in a few hundred microseconds per event. How often those happen decides between weeks and years.",
        path: &["Wireless", "Bluetooth", "Bluetooth LE"],
        make: || Box::new(BlePower::default()),
        explain: Explanation {
            what: "The upper graph is the current a Bluetooth chip draws during one [[advertising]] event, over about a \
                millisecond or two: three stretches back to back, one for each advertising channel. Each is the radio \
                starting up, sending the packet, and - if the device accepts connections - listening briefly for an answer.\n\n\
                Between events the chip sleeps. The lower graph turns all of that into battery life, for every advertising \
                interval from 20 ms to 10 s.",
            why: "A Bluetooth LE sensor that only advertises spends its life in two states. Asleep, it draws a microampere or \
                two - enough to keep its memory and a slow clock that wakes it. Awake with the radio on, it draws several \
                milliamperes, thousands of times more. The radio is on only for an extremely short time: a full 31-byte \
                advertising packet lasts 376 microseconds at one megabit per second.\n\n\
                So the battery's fate is decided by a simple ratio: how much charge one event costs, and how often events \
                happen. One event here costs about 14 microcoulombs. Every 100 ms, that averages to almost 140 µA, and a \
                coin cell is flat in about two months. Every second, it is 16 µA and about a year and a half. Every ten \
                seconds, sleeping already accounts for half of the drain.\n\n\
                That is the shape of the curve: falling steeply while the events dominate, then flattening as it approaches \
                the limit set by the sleep current alone. At that point, advertising even less often gains almost nothing; \
                only a lower sleep current helps. The same reasoning as in the microcontroller's sleep-mode demo, now with \
                real radio numbers.\n\n\
                The design choice is a trade with the user: a phone can only find or connect to the sensor while it is \
                advertising. A long interval saves the battery but makes the sensor slow to discover and slow to report.",
            formulas: &[
                ("average current = charge per event / (interval + average advDelay) + sleep current", "14.4 µC / 105 ms + 1.6 µA = 139 µA. The extra 5 ms is the average of the random 0-10 ms delay added to every interval."),
                ("battery life = capacity / average current", "225 mAh / 139 µA = 1617 h, about 67 days. At a 1 s interval: 225 mAh / 16 µA, about 1.6 years."),
            ],
            try_this: &[
                Experiment {
                    action: "Advertise once per second instead of ten times.",
                    watch: "the battery life growing from about 67 days to about 1.6 years - nearly ten times, not quite, because the sleep current stays.",
                    preset: Some("second"),
                },
                Experiment {
                    action: "Make it a beacon: not connectable, 20 bytes of data, once per second.",
                    watch: "the listening part of each burst disappearing and the packets getting shorter: another big step in battery life.",
                    preset: Some("beacon"),
                },
                Experiment {
                    action: "Go to the slowest interval the specification allows, 10.24 s.",
                    watch: "the dot reaching the flat part of the curve: half of the drain is now sleeping, so advertising even less often would gain little. Try lowering the sleep current instead.",
                    preset: Some("slowest"),
                },
            ],
            leaves_out: "The processor's own work around each event (waking up, preparing the packet, measuring the sensor) is \
                left out, and so is the time the chip's fast crystal needs to start; on real chips these add noticeably. \
                The radio start-up time is an example value. The sending current depends on the transmit power. A real \
                coin cell delivers less than its rated capacity under current pulses and loses some charge by itself over \
                the years, so long lifetimes are optimistic. A connected device (instead of an advertising one) has a similar \
                budget, with the connection interval in place of the advertising interval.",
            refs: &[
                "Bluetooth Core Specification 5.4, Vol 6 Part B 4.4.2: advertising interval 20 ms to 10.24 s, plus advDelay 0-10 ms; T_IFS = 150 µs; packet layout of a legacy advertising PDU at 1 Mbit/s.",
                "Nordic Semiconductor nRF52832 Product Specification and product brief: TX at 0 dBm 7.1 mA and RX at 1 Mbit/s 6.5 mA (electrical specification, total current), 1.6 µA idle with RTC running. The headline figures of 5.3 / 5.4 mA count the radio only.",
                "CR2032 nominal capacity 225 mAh: Panasonic CR2032 datasheet.",
            ],
        },
    }
}
