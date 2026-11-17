use crate::prelude::*;
use egui_plot::{HLine, Points};
use sim_core::bus::{NoisyLink, RS485_COMMON_MODE_V, RS485_THRESHOLD_V, noisy_link};

pub struct Rs485 {
    link: NoisyLink,
}

impl Default for Rs485 {
    fn default() -> Self {
        Self { link: NoisyLink { drive_v: 3.0, noise_v: 4.0, imbalance: 0.0, ground_shift_v: 0.0 } }
    }
}

fn bits() -> Vec<bool> {
    (0..24).map(|i| i % 3 == 0 || i % 7 == 2).collect()
}

impl Demo for Rs485 {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        let l = &mut self.link;
        widgets::group(ui, "A cable through an electrically noisy building", |ui| {
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "noise", |ui| ui.add(egui::Slider::new(&mut l.noise_v, 0.0..=10.0).text("interference picked up").suffix(" V").fixed_decimals(1)).on_hover_text("Voltage that motors, lamp ballasts and mains cables induce in the data cable on its way."));
                widgets::wrap(ui, "ground", |ui| ui.add(egui::Slider::new(&mut l.ground_shift_v, -12.0..=12.0).text("ground difference").suffix(" V").fixed_decimals(1)).on_hover_text("Sender and receiver are far apart and their grounds are not at the same voltage. This shifts everything the receiver sees."));
            });
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "drive", |ui| ui.add(egui::Slider::new(&mut l.drive_v, 1.5..=5.0).text("signal size").suffix(" V").fixed_decimals(1)).on_hover_text("How far the sender moves the wire(s). An RS-485 driver must manage at least 1.5 V between the two wires."));
                widgets::wrap(ui, "imbalance", |ui| {
                    ui.add(egui::Slider::new(&mut l.imbalance, 0.0..=1.0).text("share of the interference that hits only one wire").custom_formatter(|v, _| format!("{:.0} %", v * 100.0)))
                        .on_hover_text("In a twisted pair both wires pick up almost the same interference: near 0 %. Two wires run apart from each other do not.")
                });
            });
        });
        let link = self.link;
        let sent = bits();
        let tr = noisy_link(&link, &sent, 7);
        let n = sent.len() as f64;
        let xy = |ys: &[f64]| -> Vec<[f64; 2]> { tr.t.iter().zip(ys).map(|(t, y)| [*t, *y]).collect() };
        let marks = |read: &[bool], y_ok: f64| -> (Vec<[f64; 2]>, Vec<[f64; 2]>) {
            let pts: Vec<([f64; 2], bool)> = read.iter().zip(&sent).enumerate().map(|(k, (r, s))| ([k as f64 + 0.5, y_ok], r == s)).collect();
            (pts.iter().filter(|p| p.1).map(|p| p.0).collect(), pts.iter().filter(|p| !p.1).map(|p| p.0).collect())
        };

        ui.label(RichText::new("One wire measured against ground").strong());
        plots::plot_bare("single", 170.0).x_axis_label("time [bit periods]").y_axis_label("volts").include_x(0.0).include_x(n).show(ui, |p| {
            p.line(plots::line("wire", xy(&tr.single), pal.trace[3]));
            p.hline(HLine::new("threshold", link.drive_v / 2.0).color(pal.dim).style(egui_plot::LineStyle::dashed_dense()));
            let (ok, bad) = marks(&tr.single_read, link.drive_v / 2.0);
            p.points(Points::new("read right", ok).radius(3.5).color(pal.good));
            p.points(Points::new("read wrong", bad).radius(5.0).color(pal.bad));
        });
        ui.label(RichText::new("RS-485: the two wires of the pair ...").strong());
        plots::plot_bare("pair", 170.0).x_axis_label("time [bit periods]").y_axis_label("volts").include_x(0.0).include_x(n).show(ui, |p| {
            p.line(plots::line("A", xy(&tr.a), pal.trace[0]));
            p.line(plots::line("B", xy(&tr.b), pal.trace[1]));
        });
        ui.label(RichText::new("... and what the receiver looks at: A minus B").strong());
        let diff: Vec<f64> = tr.a.iter().zip(&tr.b).map(|(a, b)| a - b).collect();
        plots::plot_bare("diff", 170.0).x_axis_label("time [bit periods]").y_axis_label("volts").include_x(0.0).include_x(n).include_y(-link.drive_v - 0.5).include_y(link.drive_v + 0.5).show(ui, |p| {
            p.line(plots::line("A - B", xy(&diff), pal.trace[2]).width(2.0));
            for y in [RS485_THRESHOLD_V, -RS485_THRESHOLD_V] {
                p.hline(HLine::new("threshold", y).color(pal.dim).style(egui_plot::LineStyle::dashed_dense()));
            }
            let (ok, bad) = marks(&tr.pair_read, 0.0);
            p.points(Points::new("read right", ok).radius(3.5).color(pal.good));
            p.points(Points::new("read wrong", bad).radius(5.0).color(pal.bad));
        });
        widgets::key(ui, &[(pal.trace[3], "the single wire"), (pal.trace[0], "wire A"), (pal.trace[1], "wire B"), (pal.trace[2], "A - B"), (pal.dim, "dashed: receiver thresholds"), (pal.good, "bit read right"), (pal.bad, "bit read wrong")]);

        let out_of_range = tr.pair_range.0 < RS485_COMMON_MODE_V.0 || tr.pair_range.1 > RS485_COMMON_MODE_V.1;
        widgets::status(ui, tr.single_errors == 0, format!("single wire: {} of {} bits read wrong", tr.single_errors, sent.len()));
        widgets::status(
            ui,
            tr.pair_errors == 0 && !out_of_range,
            if out_of_range {
                format!("RS-485 pair: the wires reach {:.1} V to {:.1} V, outside the {:.0} V to +{:.0} V a receiver is built for", tr.pair_range.0, tr.pair_range.1, RS485_COMMON_MODE_V.0, RS485_COMMON_MODE_V.1)
            } else {
                format!("RS-485 pair: {} of {} bits read wrong", tr.pair_errors, sent.len())
            },
        );
        widgets::formula(
            ui,
            format!(
                "wire A = +signal / 2 + interference        wire B = -signal / 2 + interference\n\
                 A - B  = signal = +/- {:.1} V        the interference appears in both and drops out\n\
                 left over: only the part that hits one wire alone = {:.0} % of {:.1} V = {:.1} V",
                link.drive_v,
                link.imbalance * 100.0,
                link.noise_v,
                link.imbalance * link.noise_v,
            ),
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        let l = &mut self.link;
        match preset {
            "ground" => (l.noise_v, l.ground_shift_v) = (0.5, 4.0),
            "untwisted" => (l.noise_v, l.imbalance) = (6.0, 0.6),
            "too-far" => (l.noise_v, l.ground_shift_v) = (0.5, 11.5),
            "quiet" => l.noise_v = 0.0,
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "bus-rs485",
        title: "RS-485: two wires against the noise",
        blurb: "How a signal survives a hundred metres of cable next to motors and mains: send it twice, mirrored, and look only at the difference.",
        path: &["Wired buses"],
        make: || Box::new(Rs485::default()),
        explain: Explanation {
            what: "The same row of bits is sent over a long cable in two ways, and both pick up the same interference. The top \
                graph is a single wire, measured against ground at the far end, as a simple serial port does it. The middle \
                graph shows the two wires of an [[RS-485]] pair. The bottom graph is what an RS-485 receiver actually looks \
                at: the voltage of one wire minus the other.\n\n\
                A dot marks each bit as the receiver reads it, green if right and red if wrong.",
            why: "Inside a device, a signal is a voltage on a wire measured against ground, and that works because the wire \
                is a few centimetres long. Over tens or hundreds of metres through a building, two things go wrong. The \
                cable acts as an antenna and picks up interference from everything it passes. And the far end's ground is \
                not at the same voltage as the near end's, sometimes by several volts. The receiver compares the wire with \
                its own ground and a fixed threshold - and reads nonsense.\n\n\
                RS-485 solves both problems with one idea. The sender uses two wires and drives them in opposite directions: \
                when one goes up, the other goes down. The receiver ignores ground altogether and only asks which of the two \
                wires is higher.\n\n\
                Interference hits both wires. If the wires run tightly side by side - twisted around each other - it hits \
                them equally, raising or lowering both by the same amount. A ground difference does exactly the same. \
                Anything that is added to both wires vanishes when one is subtracted from the other. What remains is the \
                signal.\n\n\
                The receiver needs very little: a difference of a fifth of a volt is enough to tell the two states apart, \
                while the driver delivers at least one and a half volts. That margin is what allows cable lengths of up to \
                about a kilometre at low speeds, and it is why RS-485 is the wire underneath so many industrial and \
                building systems - DMX stage lighting and Modbus among them.\n\n\
                There are limits. The trick only cancels what both wires have in common, so the pair must be twisted. And \
                each wire by itself must stay within the range the receiver chip tolerates, about -7 V to +12 V.",
            formulas: &[
                ("A - B = (signal / 2 + interference) - (-signal / 2 + interference) = signal", "With 3 V of signal and 4 V of interference, the difference is still a clean +/- 3 V."),
                ("receiver: A - B above +0.2 V is one state, below -0.2 V the other", "The driver must produce at least 1.5 V, so more than 1.3 V may be lost on the way."),
            ],
            try_this: &[
                Experiment {
                    action: "Take away the interference and give the far end a ground that is 4 V higher.",
                    watch: "the single wire sitting entirely above its threshold: every bit reads as 1. The pair is lifted by the same 4 V on both wires, and its difference has not moved at all.",
                    preset: Some("ground"),
                },
                Experiment {
                    action: "Untwist the pair: let 60 % of a 6 V interference hit only one wire.",
                    watch: "interference appearing in the difference for the first time, and the first red dots in the bottom graph. Twisting is not decoration.",
                    preset: Some("untwisted"),
                },
                Experiment {
                    action: "Raise the ground difference to 11.5 V.",
                    watch: "the difference still perfect, but the wires themselves climbing past +12 V: beyond what the receiver chip can take. Real installations add a ground wire or isolation for this.",
                    preset: Some("too-far"),
                },
            ],
            leaves_out: "RS-485 defines only the electrics - which voltage means what. Which bytes are sent, and who may speak when, \
                is up to the protocol on top (a UART usually supplies the bits). Several devices share the pair, so each \
                must switch its driver off when it is not sending. A long cable also needs a resistor of about 120 ohm \
                across the pair at each end, or the signal echoes back from the ends. The interference here is a made-up hum \
                with spikes; real interference depends on the installation.",
            refs: &[
                "TIA/EIA-485-A: differential driver output of at least 1.5 V into 54 ohm, receiver thresholds of +/-200 mV, common-mode range -7 V to +12 V.",
                "Texas Instruments, \"The RS-485 Design Guide\" (application report SLLA272): termination, grounding and cable length versus data rate.",
            ],
        },
    }
}
