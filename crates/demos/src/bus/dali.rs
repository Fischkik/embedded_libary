use crate::prelude::*;
use bits::FieldSpan;
use egui_plot::{PlotPoint, Points, Text};
use sim_core::bus::{DALI_BIT_S, DaliAddress, dali_address_byte, dali_frame_bits, dali_manchester, dali_output};

/// A few of the standard commands (opcode byte when the selector bit is 1).
const COMMANDS: [(u8, &str, &str); 4] = [
    (0x00, "OFF", "Switch the lamp off at once."),
    (0x05, "RECALL MAX LEVEL", "Go to the highest level the lamp is set up for."),
    (0x06, "RECALL MIN LEVEL", "Go to the lowest level the lamp is set up for."),
    (0x01, "UP", "Dim up for a moment, at the lamp's stored fade rate."),
];

#[derive(Clone, Copy, PartialEq)]
enum Target {
    Short,
    Group,
    Broadcast,
}

pub struct Dali {
    target: Target,
    number: u8,
    is_command: bool,
    level: u8,
    command: usize,
}

impl Default for Dali {
    fn default() -> Self {
        Self { target: Target::Short, number: 5, is_command: false, level: 200, command: 0 }
    }
}

impl Demo for Dali {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "A message from the controller to the lamps", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("To:");
                ui.selectable_value(&mut self.target, Target::Short, "one lamp").on_hover_text("Every device on a DALI line has its own short address, 0 to 63.");
                ui.selectable_value(&mut self.target, Target::Group, "a group").on_hover_text("A lamp can belong to any of 16 groups, for example \"all lamps by the windows\".");
                ui.selectable_value(&mut self.target, Target::Broadcast, "all lamps").on_hover_text("Broadcast: every device on the line obeys.");
                let max = if self.target == Target::Group { 15 } else { 63 };
                self.number = self.number.min(max);
                widgets::wrap(ui, "number", |ui| ui.add_enabled(self.target != Target::Broadcast, egui::Slider::new(&mut self.number, 0..=max).text("number")).on_hover_text("Which lamp, or which group."));
            });
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut self.is_command, false, "set a brightness").on_hover_text("The second byte is a light level from 0 to 254.");
                ui.selectable_value(&mut self.is_command, true, "send a command").on_hover_text("The second byte is the number of a command.");
                if self.is_command {
                    for (i, (_, name, help)) in COMMANDS.iter().enumerate() {
                        ui.selectable_value(&mut self.command, i, *name).on_hover_text(*help);
                    }
                } else {
                    widgets::wrap(ui, "level", |ui| ui.add(egui::Slider::new(&mut self.level, 0..=254).text("level")).on_hover_text("The light level the lamp should go to. 0 is off, 254 is full brightness. The steps are not equal amounts of light: see the curve below."));
                }
            });
        });
        let address = match self.target {
            Target::Short => DaliAddress::Short(self.number),
            Target::Group => DaliAddress::Group(self.number),
            Target::Broadcast => DaliAddress::Broadcast,
        };
        let address_byte = dali_address_byte(address, self.is_command);
        let data_byte = if self.is_command { COMMANDS[self.command].0 } else { self.level };
        let frame = dali_frame_bits(address_byte, data_byte);

        ui.label(RichText::new("The frame: 1 start bit and two bytes").strong());
        let fields = [
            FieldSpan::new("start bit", 1, "Always 1. It tells every device that a frame begins."),
            FieldSpan::new("address", 7, "Who the message is for: one lamp, a group, or all."),
            FieldSpan::new("selector", 1, "0: the next byte is a brightness. 1: the next byte is a command."),
            FieldSpan::new("data", 8, "The brightness level or the command number."),
        ];
        // The strip groups cells in eights; pad the front so that the two bytes line up.
        let mut padded = vec![false; 7];
        padded.extend(&frame);
        let mut strip_fields = vec![FieldSpan::new("(unused)", 7, "Padding in this picture only; not on the wire.")];
        strip_fields.extend(fields.iter().cloned());
        bits::bit_strip(ui, &padded, &strip_fields, None);
        widgets::key(ui, &[(theme::field_color(1, pal.dark), "start bit"), (theme::field_color(2, pal.dark), "address"), (theme::field_color(3, pal.dark), "selector bit"), (theme::field_color(4, pal.dark), "data")]);

        ui.label(RichText::new("The same frame as the voltage on the two bus wires").strong());
        let edges = dali_manchester(&frame, 1.0);
        let span = frame.len() as f64 + 4.0;
        let ms = DALI_BIT_S * 1e3;
        let ms_edges: Vec<(f64, bool)> = edges.iter().map(|&(t, l)| (t * ms, l)).collect();
        plots::plot_bare("wave", 170.0).x_axis_label("time [ms]").y_axis_label("bus voltage [V]").include_x(0.0).include_x(span * ms).include_y(-3.0).include_y(22.0).show(ui, |p| {
            p.line(plots::line("bus", plots::digital(&ms_edges, true, 0.0, span * ms, 0.0, 16.0), pal.trace[0]).width(2.0));
            for (k, &bit) in frame.iter().enumerate() {
                p.text(Text::new("bit", PlotPoint::new((1.5 + k as f64) * ms, 19.5), if bit { "1" } else { "0" }).color(pal.text));
            }
        });

        let out = dali_output(self.level);
        if !self.is_command {
            ui.label(RichText::new("What the level means: the dimming curve").strong());
            ui.horizontal(|ui| {
                let (lamp, _) = ui.allocate_exact_size(vec2(60.0, 60.0), egui::Sense::hover());
                let seen = (out as f32).powf(1.0 / 2.2);
                ui.painter().circle_filled(lamp.center(), 26.0, Color32::from_rgb((255.0 * seen) as u8, (220.0 * seen) as u8, (90.0 * seen) as u8));
                ui.painter().circle_stroke(lamp.center(), 26.0, (1.5, pal.dim));
                ui.vertical(|ui| {
                    ui.label(RichText::new(format!("Level {} = {:.1} % of full power", self.level, out * 100.0)).strong());
                    ui.label(RichText::new("The circle shows roughly how bright that looks: the eye is far more sensitive at the dim end.").small().color(pal.dim));
                });
            });
            let curve: Vec<[f64; 2]> = (0..=254u8).map(|l| [l as f64, dali_output(l) * 100.0]).collect();
            plots::plot_bare("curve", 180.0).x_axis_label("DALI level").y_axis_label("light output [%]").include_x(0.0).include_x(254.0).include_y(0.0).include_y(100.0).show(ui, |p| {
                p.line(plots::line("light output", curve, pal.trace[1]).width(2.0));
                p.points(Points::new("this level", vec![[self.level as f64, out * 100.0]]).radius(6.0).color(pal.text));
            });
        }

        let who = match address {
            DaliAddress::Short(a) => format!("0AAA AAAS with A = {a}"),
            DaliAddress::Group(g) => format!("100A AAAS with A = {g}"),
            DaliAddress::Broadcast => "1111 111S".to_owned(),
        };
        widgets::formula(
            ui,
            format!(
                "address byte = {who}, S = {} = 0x{address_byte:02X}        data byte = 0x{data_byte:02X}{}\n\
                 time on the wire = 17 bits / 1200 bit/s = {:.1} ms{}",
                self.is_command as u8,
                if self.is_command { format!(" (command {})", COMMANDS[self.command].1) } else { format!(" (level {})", self.level) },
                17.0 * ms,
                if self.is_command { String::new() } else { format!("\nlight output = 10^(3 x (level - 254) / 253) = 10^(3 x ({} - 254) / 253) = {:.2} %", self.level, out * 100.0) },
            ),
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "half" => self.level = 127,
            "group" => (self.target, self.number) = (Target::Group, 3),
            "all-off" => (self.target, self.is_command, self.command) = (Target::Broadcast, true, 0),
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "bus-dali",
        title: "DALI lighting bus",
        blurb: "Two wires that tell every lamp in a building how bright to be. Build a message and see it on the wire.",
        path: &["Wired buses"],
        make: || Box::new(Dali::default()),
        explain: Explanation {
            what: "At the top you put together one [[DALI]] message: who it is for, and what it says. Below it is the message as \
                a row of bits, coloured by meaning, and then the same bits as the voltage on the bus over time. When the \
                message sets a brightness, the bottom graph shows how the level number translates into light.",
            why: "DALI is the bus that lighting controls and lamps in commercial buildings use to talk to each other. It is \
                deliberately simple and slow, built for electricians rather than for speed. Two wires run from lamp to lamp, \
                alongside the mains cable, in any shape - line, star or tree. The wires may be swapped at any device without \
                harm. They carry about 16 volts, enough to power small devices such as sensors and push-buttons from the bus \
                itself.\n\n\
                A message is tiny: a start bit and two bytes. The first byte says who is meant. Each device has a short \
                address from 0 to 63; it can also be a member of up to 16 groups; or the message can go to everyone. The \
                last bit of that byte says how to read the second byte: as a brightness, or as the number of a command such \
                as \"off\" or \"go to your maximum\".\n\n\
                On the wire, the bits are sent at 1200 per second in a code named Manchester. Every bit has a change of \
                level in its middle: a 1 is low then high, a 0 is high then low. This doubles the number of level changes, \
                but it means every single bit brings its own timing with it, so cheap devices with imprecise clocks can \
                follow along, and a constant level can never be mistaken for data.\n\n\
                The brightness level is not a percentage. Our eyes judge brightness by ratios: from 1 % to 2 % looks like \
                the same step as from 50 % to 100 %. If the 254 levels were spread evenly over the power, nearly all of \
                them would be wasted at the bright end where no difference can be seen. So the levels follow a curve in \
                which every step increases the light by the same factor, about 2.8 %. Level 1 is a tenth of a percent, \
                level 254 is full power - and the middle of the range is only 3 % of the light, although it looks far \
                brighter than that.",
            formulas: &[
                ("light output = 10^(3 x (level - 254) / 253)", "Level 254: 100 %. Level 229: 50 %. Level 127: 3.1 %. Level 1: 0.1 %."),
                ("address byte: 0AAAAAAS one lamp, 100AAAAS a group, 1111111S all", "Lamp 5, brightness follows: 0000 1010 = 0x0A. All lamps, command follows: 0xFF."),
            ],
            try_this: &[
                Experiment {
                    action: "Set the level to 127, the middle of the range.",
                    watch: "only 3 % of the power, yet the lamp circle is far from dark. Then find the level that gives 50 % power: it is around 229, close to the top.",
                    preset: Some("half"),
                },
                Experiment {
                    action: "Send the same brightness to group 3 instead of lamp 5.",
                    watch: "the address bits changing to 100 followed by the group number. One message, and every lamp that was told \"you are in group 3\" reacts together.",
                    preset: Some("group"),
                },
                Experiment {
                    action: "Send the command OFF to all lamps.",
                    watch: "the address byte becoming all ones, 0xFF, and the selector bit - the last of the address byte - now being 1: the second byte is a command, not a level.",
                    preset: Some("all-off"),
                },
            ],
            leaves_out: "Only messages from a controller to lamps are shown. A lamp can answer a query with a single byte, sent the \
                same way, and newer devices such as sensors and push-buttons send 24-bit frames of their own. Before lamps \
                have addresses, a controller hands them out in a commissioning procedure. A lamp does not jump to a new \
                level but fades there at a stored rate, and it stores scenes it can recall. The bus voltage is drawn as an \
                ideal 16 V and 0 V; in reality anything from 9.5 V upward counts as high and below 6.5 V as low at a \
                receiver, and the bus supply is limited to 250 mA. Level 255 is reserved and means \"no change\".",
            refs: &[
                "IEC 62386 (DALI), parts 101 and 102: 1200 bit/s Manchester code, forward frame of 1 start bit and 16 data bits, address byte formats, direct arc power control, command numbers (0x00 OFF, 0x01 UP, 0x05 RECALL MAX LEVEL, 0x06 RECALL MIN LEVEL), logarithmic dimming curve.",
                "Summary used for the constants: Wikipedia, \"Digital Addressable Lighting Interface\".",
            ],
        },
    }
}
