use crate::prelude::*;
use sim_core::modem::{self, Scheme};
use sim_core::{dsp, rng::Rng};

const BIT_RATE: f64 = 1e6;
const SPS: usize = 16;
const FS: f64 = BIT_RATE * SPS as f64;
/// Carrier cycles per bit in the illustrative waveform (a real 2.4 GHz carrier has 2400).
const SHOWN_CYCLES_PER_BIT: f64 = 3.0;

pub struct Modulation {
    bits: Vec<bool>,
    scheme: Scheme,
    deviation_hz: f64,
    bt: f64,
    snr_db: f64,
    seed: u64,
}

impl Default for Modulation {
    fn default() -> Self {
        Self {
            bits: [1, 0, 1, 1, 0, 0, 1, 0, 1, 1, 1, 0, 0, 0, 1, 0].iter().map(|b| *b == 1).collect(),
            scheme: Scheme::Gfsk,
            deviation_hz: 250e3,
            bt: 0.5,
            snr_db: 30.0,
            seed: 1,
        }
    }
}

impl Demo for Modulation {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);

        widgets::group(ui, "The data: click a bit to flip it", |ui| {
            let fields = [bits::FieldSpan::new("data", self.bits.len(), "The bits to send, first bit on the left.")];
            if let Some(i) = bits::bit_strip(ui, &self.bits, &fields, None).clicked {
                self.bits[i] = !self.bits[i];
            }
        });
        widgets::group(ui, "How to put the bits on the carrier", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut self.scheme, Scheme::Ook, "OOK: on / off").on_hover_text("On-off keying. 1 = carrier on, 0 = carrier off. Cheap remote controls work like this.");
                ui.selectable_value(&mut self.scheme, Scheme::Fsk, "FSK: two frequencies").on_hover_text("Frequency-shift keying. 1 = slightly higher frequency, 0 = slightly lower, switching instantly.");
                ui.selectable_value(&mut self.scheme, Scheme::Gfsk, "GFSK: smoothed FSK").on_hover_text("Like FSK, but the jump between the two frequencies is smoothed. Bluetooth LE uses this.");
            });
            let fm = self.scheme != Scheme::Ook;
            ui.add_enabled(fm, widgets::si_slider(&mut self.deviation_hz, 50e3..=1e6, "frequency deviation", "Hz").logarithmic(true))
                .on_hover_text("How far the frequency moves away from the centre for a 1 or a 0. Bluetooth LE: about 250 kHz.");
            ui.add_enabled(self.scheme == Scheme::Gfsk, egui::Slider::new(&mut self.bt, 0.2..=1.0).text("smoothing (BT)").fixed_decimals(2))
                .on_hover_text("Smaller = smoother transitions = narrower signal, but neighbouring bits blur into each other. Bluetooth LE: 0.5.");
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "snr", |ui| {
                    ui.add(egui::Slider::new(&mut self.snr_db, -5.0..=30.0).text("signal-to-noise ratio").suffix(" dB").fixed_decimals(0))
                        .on_hover_text("How much stronger the signal is than the noise at the receiver. Lower = longer distance or more interference.")
                });
                if ui.button("New noise").on_hover_text("Roll the dice again for the random noise").clicked() {
                    self.seed += 1;
                }
            });
        });

        // Transmit, add noise, receive.
        let shape = modem::frequency_shape(&self.bits, self.scheme, SPS, self.bt);
        let clean = modem::modulate(&self.bits, self.scheme, SPS, self.deviation_hz, FS, self.bt);
        let mut noisy = clean.clone();
        modem::add_noise(&mut noisy, self.snr_db, &mut Rng::new(self.seed));
        let (rx_bits, decision) = modem::demodulate(&noisy, self.scheme, SPS, FS);
        let errors = modem::bit_errors(&self.bits, &rx_bits);
        let us = 1e6 / FS; // microseconds per sample

        ui.label(RichText::new("1. What the transmitter does to the carrier").strong());
        let (ylabel, command): (&str, Vec<f64>) = match self.scheme {
            Scheme::Ook => ("carrier strength", self.bits.iter().flat_map(|b| std::iter::repeat_n(*b as u8 as f64, SPS)).collect()),
            _ => ("frequency offset [kHz]", shape.iter().map(|s| s * self.deviation_hz / 1e3).collect()),
        };
        plots::plot("command", 185.0).x_axis_label("time [µs]  (1 bit = 1 µs)").y_axis_label(ylabel).show(ui, |p| {
            p.line(plots::line("transmitter", plots::series(&command, 0.0, us), pal.trace[0]));
        });

        ui.label(RichText::new("2. The wave that goes out (drawn with far fewer cycles per bit than in reality)").strong());
        let wave: Vec<[f64; 2]> = {
            // Draw on a low illustrative carrier with 8 points per input sample for a smooth curve.
            let over = 8;
            let mut phase = 0.0f64;
            let mut pts = Vec::with_capacity(command.len() * over);
            for (i, s) in shape.iter().enumerate() {
                for k in 0..over {
                    let t_bits = (i * over + k) as f64 / (SPS * over) as f64;
                    let (amp, dev) = match self.scheme {
                        Scheme::Ook => (if self.bits[(i / SPS).min(self.bits.len() - 1)] { 1.0 } else { 0.0 }, 0.0),
                        _ => (1.0, s * self.deviation_hz / BIT_RATE),
                    };
                    phase += std::f64::consts::TAU * (SHOWN_CYCLES_PER_BIT + dev) / (SPS * over) as f64;
                    pts.push([t_bits, amp * phase.sin()]);
                }
            }
            pts
        };
        plots::plot("wave", 170.0).x_axis_label("time [µs]").y_axis_label("field").include_y(-1.2).include_y(1.2).show(ui, |p| {
            p.line(plots::line("radio wave", wave, pal.trace[1]));
        });

        ui.label(RichText::new("3. What the receiver measures, and the bits it decides on").strong());
        let scale = if self.scheme == Scheme::Ook { 1.0 } else { 1e-3 };
        let measured: Vec<f64> = decision.iter().map(|d| d * scale).collect();
        plots::plot("rx", 190.0).x_axis_label("time [µs]").y_axis_label(ylabel).show(ui, |p| {
            p.line(plots::line("receiver sees", plots::series(&measured, 0.0, us), pal.trace[2]));
            let threshold = if self.scheme == Scheme::Ook { 0.5 } else { 0.0 };
            p.hline(egui_plot::HLine::new("decision threshold", threshold).color(pal.dim).style(egui_plot::LineStyle::dashed_dense()));
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Received:");
            for (i, b) in rx_bits.iter().enumerate() {
                let wrong = self.bits.get(i) != Some(b);
                let text = RichText::new(if *b { "1" } else { "0" }).monospace().strong();
                ui.label(if wrong { text.color(pal.bad).underline() } else { text.color(pal.good) });
            }
        });
        widgets::status(ui, errors == 0, if errors == 0 { "all bits received correctly".to_owned() } else { format!("{errors} of {} bits wrong (shown red)", self.bits.len()) });

        ui.label(RichText::new("4. How much room the signal takes up on air").strong());
        // Use a long random bit pattern for a representative spectrum.
        let mut rng = Rng::new(9);
        let long: Vec<bool> = (0..256).map(|_| rng.bool()).collect();
        let spec = dsp::spectrum_iq(&modem::modulate(&long, self.scheme, SPS, self.deviation_hz, FS, self.bt), FS);
        let obw = dsp::occupied_bandwidth(&spec, 0.99);
        let pts: Vec<[f64; 2]> = spec.iter().filter(|p| p[0].abs() <= 4e6).map(|p| [p[0] / 1e6, p[1].max(-90.0)]).collect();
        plots::plot("spectrum", 190.0).x_axis_label("distance from the carrier frequency [MHz]").y_axis_label("level [dB]").include_y(-80.0).include_y(0.0).show(ui, |p| {
            p.line(plots::line("spectrum", pts, pal.trace[4]).fill(-90.0).fill_alpha(0.15));
        });
        widgets::formula(
            ui,
            format!(
                "99 % of the power sits within {}      modulation index h = 2 x deviation / bit rate = {:.2}  (Bluetooth LE: 0.45 to 0.55)",
                si(obw, "Hz"),
                2.0 * self.deviation_hz / BIT_RATE
            ),
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        match preset {
            "fsk" => self.scheme = Scheme::Fsk,
            "noisy" => self.snr_db = 3.0,
            "blurred" => self.bt = 0.2,
            "ook" => self.scheme = Scheme::Ook,
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "modulation",
        title: "Modulation: bits on a wave",
        blurb: "How ones and zeros become changes in a radio wave - and how noise turns them into errors.",
        path: &["Wireless", "Radio basics"],
        make: || Box::new(Modulation::default()),
        explain: Explanation {
            what: "Sixteen [[bits]] travel from top to bottom of the page. Graph 1 is the transmitter's instruction to the \
                [[carrier]]: for every bit, move the frequency up or down a little. Graph 2 is the resulting radio wave. Graph 3 is \
                what a receiver measures after [[noise]] has been added, with the bits it decides on underneath. Graph 4 is the \
                [[spectrum]] of the transmission: how much room it takes on air.",
            why: "A steady carrier says nothing. [[Modulation]] means changing it in step with the data. The simplest way is to \
                switch it on and off (OOK). A sturdier way is to keep the strength constant and nudge the [[frequency]] instead: a \
                little higher for 1, a little lower for 0. That is frequency-shift keying, FSK. The size of the nudge is the \
                [[deviation]].\n\n\
                Jumping abruptly between the two frequencies has a cost: sharp changes spread energy over a wide range of \
                frequencies, which disturbs the neighbouring [[channels]]. So Bluetooth LE smooths the jumps with a Gaussian-shaped \
                filter first. That is [[GFSK]], and it is why graph 1 has rounded corners.\n\n\
                The nudge is astonishingly small. Bluetooth LE shifts a 2 400 000 kHz carrier by 250 kHz - about 0.01 %. In graph 2 \
                you can barely see the wave stretch and squeeze, and that is with the carrier drawn 800 times slower than real.\n\n\
                The receiver undoes it: it measures the frequency, and in the middle of each bit asks one question - above or below \
                the centre? Noise makes the measurement wobble. If a wobble crosses the centre line at the wrong moment, a bit flips.",
            formulas: &[
                ("h = 2 x deviation / bit rate", "The modulation index. Bluetooth LE requires 0.45 to 0.55, so at 1 Mbit/s the deviation is about 250 kHz."),
                ("1 bit = 1 µs", "On Bluetooth LE's basic 1 Mbit/s mode. A 40-byte packet is on air for about a third of a millisecond."),
            ],
            try_this: &[
                Experiment {
                    action: "Switch between GFSK and FSK.",
                    watch: "graph 4. Plain FSK has much taller \"skirts\" to the left and right: energy that lands in other channels. The smoothing removes it.",
                    preset: Some("fsk"),
                },
                Experiment {
                    action: "Lower the signal-to-noise ratio to about 3 dB and press \"New noise\" a few times.",
                    watch: "graph 3 getting ragged until it crosses the dashed line at the wrong moments, and red bits appearing.",
                    preset: Some("noisy"),
                },
                Experiment {
                    action: "With GFSK, pull the smoothing (BT) down to 0.2.",
                    watch: "graph 1: single bits no longer reach the full deviation because their neighbours blur into them. Narrower signal, but easier to get wrong.",
                    preset: Some("blurred"),
                },
            ],
            leaves_out: "The receiver here knows exactly where each bit starts and exactly what the carrier frequency is. Real \
                receivers have to find both from the signal, which is what the [[preamble]] of a packet is for. The wave in graph 2 \
                is drawn with 3 carrier cycles per bit; the real number at 2.4 GHz is 2400. The receive filter is a simple average.",
            refs: &["Bluetooth Core 5.4, Vol 6 Part A 3.1: GFSK, BT = 0.5, modulation index 0.45 to 0.55, 1 Msym/s."],
        },
    }
}
