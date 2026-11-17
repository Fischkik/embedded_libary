use crate::prelude::*;
use bits::FieldSpan;
use sim_core::firmware::{from_q, q_mul, q_step, to_q};

pub struct FixedPoint {
    a: f64,
    b: f64,
    frac_bits: u32,
}

impl Default for FixedPoint {
    fn default() -> Self {
        Self { a: 3.3, b: 2.5, frac_bits: 8 }
    }
}

/// The 16 bits of `raw`, most significant first.
fn bits_of(raw: i16) -> Vec<bool> {
    (0..16).rev().map(|i| (raw as u16 >> i) & 1 == 1).collect()
}

impl Demo for FixedPoint {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        let n = self.frac_bits;
        let (lo, hi) = (from_q(i16::MIN, n), from_q(i16::MAX, n));
        widgets::group(ui, "Two numbers and a format", |ui| {
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "a", |ui| ui.add(egui::Slider::new(&mut self.a, -130.0..=130.0).text("a").fixed_decimals(4)).on_hover_text("A number with a fractional part, for example a voltage or a gain."));
                widgets::wrap(ui, "b", |ui| ui.add(egui::Slider::new(&mut self.b, -130.0..=130.0).text("b").fixed_decimals(4)).on_hover_text("A second number, to multiply the first one with."));
            });
            ui.add(egui::Slider::new(&mut self.frac_bits, 0..=15).text("bits after the point")).on_hover_text("How many of the 16 bits are used for the fractional part. More of them means finer steps but a smaller range.");
        });
        let n = self.frac_bits;
        let (ra, clip_a) = to_q(self.a, n);
        let (rb, clip_b) = to_q(self.b, n);
        let step = q_step(n);

        ui.label(RichText::new(format!("a as a 16-bit integer, format Q{}.{} - click a bit to flip it", 15 - n, n)).strong());
        let mut fields = vec![FieldSpan::new("sign", 1, "1 means the number is negative (two's complement).")];
        if n < 15 {
            fields.push(FieldSpan::new("whole part", (15 - n) as usize, "The bits before the binary point: 1, 2, 4, 8 ..."));
        }
        if n > 0 {
            fields.push(FieldSpan::new("fraction", n as usize, "The bits after the binary point: one half, one quarter, one eighth ..."));
        }
        let strip = bits::bit_strip(ui, &bits_of(ra), &fields, None);
        if let Some(i) = strip.clicked {
            self.a = from_q((ra as u16 ^ (1 << (15 - i))) as i16, n);
        }
        widgets::key(ui, &[(theme::field_color(0, pal.dark), "sign"), (theme::field_color(1, pal.dark), if n < 15 { "whole part" } else { "fraction" }), (theme::field_color(2, pal.dark), "fraction")]);

        widgets::formula(
            ui,
            format!(
                "one step       = 1 / 2^{n} = {step}\n\
                 range          = {lo} ... {hi:.6}\n\
                 a is stored as   round({} / {step}) = {ra}        which stands for {ra} x {step} = {}\n\
                 rounding error = {:+.6}",
                self.a,
                from_q(ra, n),
                from_q(ra, n) - self.a,
            ),
        );
        widgets::status(
            ui,
            !clip_a && !clip_b,
            if clip_a || clip_b { format!("out of range: this format only reaches from {lo} to {hi:.4}, the stored value was clipped") } else { format!("both numbers fit; nothing finer than {step} can be told apart") },
        );

        let (rp, overflow) = q_mul(ra, rb, n);
        let wide = ra as i32 * rb as i32;
        let exact = from_q(ra, n) * from_q(rb, n);
        widgets::group(ui, "Multiplying a x b with integers only", |ui| {
            widgets::formula(
                ui,
                format!(
                    "raw a x raw b      = {ra} x {rb} = {wide}        (needs 32 bits: its unit is step x step)\n\
                     shift right by {n:<2}  = {wide} >> {n} = {}        (back to the unit \"one step\")\n\
                     result             = {rp} x {step} = {}\n\
                     exact              = {} x {} = {exact}        error {:+.6}",
                    wide >> n,
                    from_q(rp, n),
                    from_q(ra, n),
                    from_q(rb, n),
                    from_q(rp, n) - exact,
                ),
            );
            widgets::status(ui, !overflow, if overflow { "overflow: the product does not fit into 16 bits and was clipped to the largest value".to_owned() } else { "the product fits".to_owned() });
        });
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "q15" => (self.a, self.b, self.frac_bits) = (0.5, 0.5, 15),
            "coarse" => self.frac_bits = 2,
            "overflow" => (self.a, self.b) = (100.0, 2.0),
            "tenth" => (self.a, self.b) = (0.1, 10.0),
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "firmware-fixed-point",
        title: "Fixed-point arithmetic",
        blurb: "Fractions on a processor that only knows whole numbers: agree where the point is, and count in small steps.",
        path: &["Firmware craft"],
        make: || Box::new(FixedPoint::default()),
        explain: Explanation {
            what: "The row of sixteen cells is one 16-bit integer, as it sits in the microcontroller's memory. Colours mark how \
                the [[bit]]s are divided up: one sign bit, some bits for the whole part of a number, and the rest for the \
                part after the point. The slider \"bits after the point\" moves that boundary.\n\n\
                Below is what the integer stands for, how far that is from the number you asked for, and a multiplication \
                done with integer operations alone.",
            why: "Many small microcontrollers can only calculate with whole numbers in hardware. Numbers with a fractional \
                part - 3.3 volts, a gain of 0.85 - need floating-point arithmetic, which such a chip has to imitate in \
                software, slowly and with a good deal of extra code. Inside an interrupt handler or a fast control loop \
                that is often unaffordable.\n\n\
                [[Fixed-point]] arithmetic is the classic way out. You decide once that an integer does not count ones but \
                small fixed steps - say 1/256. The number 3.3 is then stored as 3.3 x 256 = 845 (rounded), and 845 is an \
                ordinary integer that the processor adds, subtracts and compares at full speed. The point is not stored \
                anywhere; it exists only in the programmer's head and in the name of the format. Q7.8, for example, means \
                7 bits before the point and 8 after.\n\n\
                There is a fixed budget of bits, so there is a trade. Every bit moved behind the point halves the step size \
                and halves the largest number that fits. With 8 fraction bits the steps are 0.004 and the range is about \
                plus or minus 128; with 15 fraction bits the steps are 0.00003, but nothing beyond plus or minus 1 fits.\n\n\
                Adding two numbers of the same format just works. Multiplying needs care: the product of two step counts is \
                counted in steps times steps, which is far too fine and far too large. So the multiplication is done in a \
                wider integer, and the result is shifted right by the number of fraction bits to bring it back to the \
                original unit. If it then does not fit, it has overflowed - and a plain integer would silently wrap round \
                to nonsense, which is why careful code clips instead.",
            formulas: &[
                ("stored integer = round(value x 2^n)        value = stored integer / 2^n", "n = 8: 3.3 x 256 = 844.8, stored as 845, which stands for 3.30078."),
                ("product = (raw a x raw b) >> n", "845 x 640 = 540 800; shifted right by 8 that is 2112, which stands for 8.25."),
            ],
            try_this: &[
                Experiment {
                    action: "Reduce the bits after the point to 2.",
                    watch: "the steps growing to 0.25: 3.3 becomes 3.25. The whole part gains bits it does not need, and the fraction pays for it.",
                    preset: Some("coarse"),
                },
                Experiment {
                    action: "Set a = 0.1 and b = 10.",
                    watch: "0.1 turning into 0.1016, because one tenth is not a sum of halves, quarters and eighths. Multiplied by ten, the error is ten times as big: 1.0156 instead of 1.",
                    preset: Some("tenth"),
                },
                Experiment {
                    action: "Set a = 100 and b = 2 with 8 bits after the point.",
                    watch: "both numbers fitting, but not their product: 200 is outside the range of about 128. The result is clipped and the status turns red.",
                    preset: Some("overflow"),
                },
                Experiment {
                    action: "Choose 15 bits after the point with a = b = 0.5.",
                    watch: "the format signal processing uses, Q15: everything lies between -1 and 1, and the product of two such numbers always fits again.",
                    preset: Some("q15"),
                },
            ],
            leaves_out: "Sixteen bits only; real code picks 16, 32 or 64 bits per quantity. The multiplication here drops the bits \
                that are shifted out; careful code adds half a step first, to round instead. Division, and mixing numbers of \
                different formats, need the same kind of bookkeeping. Larger microcontrollers have a floating-point unit, \
                and there fixed-point is used mainly for signal processing and where results must be bit-for-bit \
                repeatable.",
            refs: &["Q number format (Qm.n) and fixed-point multiplication with a double-width intermediate, as described in e.g. ARM's CMSIS-DSP documentation (Q7, Q15, Q31) and R. Yates, \"Fixed-Point Arithmetic: An Introduction\"."],
        },
    }
}
