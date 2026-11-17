use crate::prelude::*;
use egui_plot::{Points, Polygon, VLine};
use sim_core::bus::{UsbAnswer, usb_polling};

const DURATION_MS: u32 = 64;

pub struct Usb {
    interval_ms: u32,
    key_ms: f64,
    second_key: bool,
}

impl Default for Usb {
    fn default() -> Self {
        Self { interval_ms: 8, key_ms: 18.5, second_key: false }
    }
}

impl Demo for Usb {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "A keyboard on a USB port", |ui| {
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "interval", |ui| ui.add(egui::Slider::new(&mut self.interval_ms, 1..=32).text("the computer asks every").suffix(" ms")).on_hover_text("The polling interval. The keyboard names the interval it would like when it is plugged in; the computer then asks at least that often."));
                widgets::wrap(ui, "key", |ui| ui.add(egui::Slider::new(&mut self.key_ms, 0.0..=60.0).text("a key is pressed at").suffix(" ms").fixed_decimals(1)).on_hover_text("The moment a finger presses a key."));
            });
            ui.checkbox(&mut self.second_key, "a second key is pressed 3 ms later").on_hover_text("Two events between two polls.");
        });
        let mut events = vec![self.key_ms];
        if self.second_key {
            events.push(self.key_ms + 3.0);
        }
        let tr = usb_polling(self.interval_ms, &events, DURATION_MS);

        ui.label(RichText::new("Who speaks on the cable, millisecond by millisecond").strong());
        let names = ["keyboard answers", "computer asks", "at the keyboard"].map(str::to_owned).to_vec();
        plots::timeline("usb", names, "time [ms]", DURATION_MS as f64).show(ui, |p| {
            for frame in 0..=DURATION_MS {
                p.vline(VLine::new("frame", frame as f64).color(pal.grid));
            }
            for poll in &tr.polls {
                let x = poll.frame as f64;
                p.polygon(Polygon::new("IN", plots::bar(x + 0.05, x + 0.3, 0.7, 1.3)).fill_color(pal.trace[0]).stroke((0.0, pal.trace[0])));
                let (name, fill) = match poll.answer {
                    UsbAnswer::Nak => ("nothing to report", pal.dim.gamma_multiply(0.8)),
                    UsbAnswer::Data => ("data", pal.trace[2]),
                };
                p.polygon(Polygon::new(name, plots::bar(x + 0.35, x + if poll.answer == UsbAnswer::Data { 0.95 } else { 0.55 }, -0.3, 0.3)).fill_color(fill).stroke((0.0, fill)));
            }
            p.points(Points::new("key pressed", events.iter().map(|e| [*e, 2.0]).collect::<Vec<_>>()).shape(egui_plot::MarkerShape::Down).radius(7.0).color(pal.warn));
            for (pressed, reported) in &tr.reported {
                if let Some(r) = reported {
                    let fill = pal.warn.gamma_multiply(0.35);
                    p.polygon(Polygon::new("waiting", plots::bar(*pressed, *r, 1.9, 2.1)).fill_color(fill).stroke((0.0, fill)));
                }
            }
        });
        widgets::key(
            ui,
            &[
                (pal.warn, "triangle: a key is pressed; pale bar: the key press waits in the keyboard"),
                (pal.trace[0], "the computer asks: \"anything new?\""),
                (pal.dim, "keyboard: \"no\""),
                (pal.trace[2], "keyboard: \"yes\" - and the data"),
                (pal.grid, "thin lines: 1 ms frames"),
            ],
        );

        let unreported = tr.reported.iter().any(|r| r.1.is_none());
        widgets::status(
            ui,
            !unreported,
            if unreported {
                "a key press has not been reported yet: the next poll comes after the end of this picture".to_owned()
            } else {
                format!("the computer learned of the key press {:.1} ms after it happened", tr.worst_delay_ms)
            },
        );
        let asks = tr.polls.len();
        let empty = tr.polls.iter().filter(|p| p.answer == UsbAnswer::Nak).count();
        widgets::formula(
            ui,
            format!(
                "waiting time = time from the key press to the next poll: between 0 and {} ms, on average {:.1} ms\n\
                 questions asked in {DURATION_MS} ms = {DURATION_MS} / {} = {asks}, of which {empty} got the answer \"nothing to report\"",
                self.interval_ms,
                self.interval_ms as f64 / 2.0,
                self.interval_ms,
            ),
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "unlucky" => self.key_ms = 16.1,
            "fast" => (self.interval_ms, self.key_ms) = (1, 16.1),
            "slow" => (self.interval_ms, self.second_key) = (32, true),
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "bus-usb",
        title: "USB basics: the host asks",
        blurb: "A USB device never speaks first. See a key press wait in the keyboard until the computer asks for it.",
        path: &["Wired buses"],
        make: || Box::new(Usb::default()),
        explain: Explanation {
            what: "The top row is the keyboard itself: a triangle marks the instant a key is pressed. The middle row shows the \
                computer asking the keyboard \"anything new?\", at regular intervals. The bottom row is the keyboard's \
                answer: a short grey bar for \"no\", a longer green one when it has a key press to report.\n\n\
                The pale bar at the top is the time the key press has to wait inside the keyboard.",
            why: "On the buses so far, a device with something to say can say it: a UART just sends, a CAN node starts a \
                message. [[USB]] is built the other way round, and understanding this explains most of how it behaves. There \
                is exactly one master of the bus, the host - the computer. Devices are not allowed to put anything on the \
                cable unless the host has just asked them to.\n\n\
                So a keyboard cannot announce a key press. The press is noted inside the keyboard, and there it waits. The \
                host divides time into frames of one millisecond, and according to a schedule it sends each device a short \
                packet that means \"your turn: do you have data for me?\". The device answers at once, either with the \
                data or with a tiny packet that means \"nothing right now\". Then the host moves on to the next device.\n\n\
                This looks wasteful - most questions get \"no\" for an answer - but it buys a great deal. Two devices can \
                never talk at the same time, so nothing collides. The host decides how the bus's time is shared out, so a \
                webcam can be promised its share while a flash drive gets whatever is left. And devices can be dumb: they \
                need no notion of who else is on the bus.\n\n\
                How often a device is asked is negotiated when it is plugged in. The device describes itself to the host - \
                what it is, what kinds of data it offers, and how often it would like to be polled. For a keyboard that is \
                typically every 8 or 10 ms; a gaming mouse asks for every millisecond.",
            formulas: &[
                ("waiting time = time to the next poll: 0 ... polling interval", "With an 8 ms interval a key press waits 4 ms on average and at most 8 ms."),
                ("polls per second = 1000 ms / polling interval", "Every 8 ms: 125 questions per second, nearly all answered with \"no\"."),
            ],
            try_this: &[
                Experiment {
                    action: "Press the key at 16.1 ms, just after the keyboard was asked.",
                    watch: "the worst case: the press has only just missed the poll at 16 ms and waits almost a full interval for the next one.",
                    preset: Some("unlucky"),
                },
                Experiment {
                    action: "Keep that moment, and poll every millisecond.",
                    watch: "the wait shrinking to under a millisecond - and the row of grey \"no\" answers becoming a solid band. This is what a 1000 Hz gaming mouse does.",
                    preset: Some("fast"),
                },
                Experiment {
                    action: "Poll only every 32 ms, and press two keys 3 ms apart.",
                    watch: "both key presses going out in the same answer. The computer cannot tell which came first, or that they were 3 ms apart.",
                    preset: Some("slow"),
                },
            ],
            leaves_out: "One device and one kind of traffic. USB has four kinds: the regular polling shown here (\"interrupt\" \
                transfers, despite the name), bulk transfers for large data that take whatever time is left, isochronous \
                transfers with reserved time for audio and video, and control transfers for setting a device up. Each \
                question and answer is really a short sequence of packets with a checksum, and the electrical side - a \
                differential pair like RS-485, the faster speeds, power delivery - is not shown. Frames are 1 ms long at \
                full speed; high-speed USB divides each into eight. The plugging-in procedure (enumeration, descriptors) is \
                a topic of its own.",
            refs: &["USB 2.0 Specification, chapters 5 (data flow model: host-scheduled transfers, 1 ms frames, interrupt endpoints with a polling interval) and 8 (IN token, DATA and NAK/ACK handshake packets)."],
        },
    }
}
