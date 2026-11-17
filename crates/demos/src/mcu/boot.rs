use crate::prelude::*;
use sim_core::mcu::{Slot, UpdateScenario, firmware_update};

const BOOTLOADER_KB: u32 = 32;

pub struct Boot {
    s: UpdateScenario,
    firmware_kb: u32,
}

impl Default for Boot {
    fn default() -> Self {
        Self { s: UpdateScenario { two_slots: true, power_cut_while_writing: false, download_corrupted: false, new_version_faulty: false }, firmware_kb: 200 }
    }
}

impl Demo for Boot {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        let s = &mut self.s;
        widgets::group(ui, "The update, and what goes wrong", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut s.two_slots, true, "bootloader and two slots").on_hover_text("A small permanent program, the bootloader, starts first and decides which firmware to run. The new firmware is stored next to the old one.");
                ui.selectable_value(&mut s.two_slots, false, "no bootloader: overwrite in place").on_hover_text("The simple way: the new firmware is written straight over the old one.");
            });
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut s.power_cut_while_writing, "the power fails while writing").on_hover_text("Someone pulls the plug, or the battery is empty, in the middle of the update.");
                ui.checkbox(&mut s.download_corrupted, "the download arrives damaged").on_hover_text("A few bytes were changed on the way, for example by radio interference.");
                ui.checkbox(&mut s.new_version_faulty, "the new version has a bug and hangs").on_hover_text("The file is complete and intact, but the new firmware does not work on this device.");
            });
            ui.add(egui::Slider::new(&mut self.firmware_kb, 20..=900).logarithmic(true).text("size of the firmware").suffix(" KB")).on_hover_text("How much flash memory one copy of the firmware takes.");
        });
        let s = self.s;
        let steps = firmware_update(s);

        ui.label(RichText::new("The update, step by step: what is in flash memory, and what runs").strong());
        let cell = |ui: &mut Ui, text: String, fill: Color32, width: f32, strong: bool| {
            let (rect, _) = ui.allocate_exact_size(vec2(width, 30.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 3.0, fill);
            if strong {
                ui.painter().rect_stroke(rect, 3.0, (2.5, pal.text), egui::StrokeKind::Inside);
            }
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(13.0), ui.visuals().strong_text_color());
        };
        let slot = |slot: Slot| match slot {
            Slot::Empty => ("empty".to_owned(), pal.canvas),
            Slot::Valid(v) => (format!("version {v}"), pal.trace[if v == 1 { 0 } else { 2 }].gamma_multiply(0.55)),
            Slot::Broken => ("damaged".to_owned(), pal.bad.gamma_multiply(0.7)),
        };
        for (i, step) in steps.iter().enumerate() {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label(RichText::new(format!("{}. {}", i + 1, step.title)).strong());
                ui.horizontal_wrapped(|ui| {
                    if s.two_slots {
                        cell(ui, "bootloader".to_owned(), pal.dim.gamma_multiply(0.5), 100.0, false);
                    }
                    let (text, fill) = slot(step.slot_a);
                    cell(ui, text, fill, 150.0, step.running.is_some());
                    if s.two_slots {
                        let (text, fill) = slot(step.slot_b);
                        cell(ui, format!("spare: {text}"), fill, 150.0, false);
                    }
                    match step.running {
                        Some(v) => ui.label(RichText::new(format!("running: version {v}")).color(pal.good).strong()),
                        None => ui.label(RichText::new("nothing is running").color(pal.bad).strong()),
                    };
                });
                ui.label(step.note);
            });
        }

        let end = steps.last().and_then(|s| s.running);
        widgets::status(
            ui,
            end.is_some(),
            match end {
                Some(2) => "the device runs the new version".to_owned(),
                Some(v) => format!("the update did not happen, but the device still works: it runs version {v} and can try again"),
                None => "the device is dead (\"bricked\"): someone has to open it and connect a programmer".to_owned(),
            },
        );
        let need = if s.two_slots { BOOTLOADER_KB + 2 * self.firmware_kb } else { self.firmware_kb };
        widgets::formula(
            ui,
            if s.two_slots {
                format!("flash memory needed = bootloader + 2 x firmware = {BOOTLOADER_KB} KB + 2 x {} KB = {need} KB", self.firmware_kb)
            } else {
                format!("flash memory needed = firmware = {need} KB        (with a bootloader and two slots: {} KB)", BOOTLOADER_KB + 2 * self.firmware_kb)
            },
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        let s = &mut self.s;
        match preset {
            "in-place-power-cut" => (s.two_slots, s.power_cut_while_writing) = (false, true),
            "power-cut" => s.power_cut_while_writing = true,
            "corrupted" => s.download_corrupted = true,
            "faulty" => s.new_version_faulty = true,
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "mcu-boot",
        title: "Boot, bootloader and firmware update",
        blurb: "Replacing the program of a running device without ever leaving it dead - even if the power fails halfway.",
        path: &["Inside the microcontroller"],
        make: || Box::new(Boot::default()),
        explain: Explanation {
            what: "Each box is one moment of a firmware update from version 1 to version 2. The coloured cells are regions of the \
                chip's [[flash]] memory, where programs are stored: a small [[bootloader]], the slot the firmware runs from \
                (outlined when something is running), and a spare slot. Read from top to bottom.\n\n\
                The opening state is an update where everything goes right. The checkboxes at the top let things go wrong.",
            why: "When a microcontroller is switched on or reset, the processor starts executing at a fixed place in flash. In \
                the simplest device the firmware itself sits there. To update it, you would overwrite it with the new \
                version - and for as long as that takes, flash contains half of the old program and half of the new one. If \
                the power fails at that moment, neither can run, and no program is left that could repair the damage.\n\n\
                So devices that are updated in the field put a bootloader at that fixed place: a small program that is never \
                overwritten. It runs first after every reset, checks what is in flash, and then starts the firmware.\n\n\
                With a bootloader, the update can be made safe step by step. The new version is downloaded into a spare slot \
                while the old one keeps running, so an interrupted download costs nothing. Before anything is switched, a \
                [[checksum]] - and in serious products a cryptographic signature - confirms that the file is complete and \
                genuine. Then the bootloader swaps the two slots, keeping the old version.\n\n\
                The last danger is a new version that is intact but does not work. For that the swap is only on trial: the \
                new firmware has to confirm itself once it is up and running properly. If it hangs instead, the [[watchdog]] \
                resets the chip, the bootloader finds no confirmation, and swaps the old version back.\n\n\
                The price is flash memory: room for the bootloader and for two copies of the firmware.",
            formulas: &[("flash needed = bootloader + 2 x firmware", "32 KB + 2 x 200 KB = 432 KB, against 200 KB without this safety net.")],
            try_this: &[
                Experiment {
                    action: "Choose \"no bootloader\" and let the power fail while writing.",
                    watch: "the single slot ending up damaged with nothing running and nothing able to fix it. This is the case everything else on this page exists to prevent.",
                    preset: Some("in-place-power-cut"),
                },
                Experiment { action: "The same power cut, with a bootloader and two slots.", watch: "only the spare slot being damaged. Version 1 was never touched and simply carries on.", preset: Some("power-cut") },
                Experiment { action: "Let the download arrive damaged.", watch: "the check rejecting the image before anything is swapped.", preset: Some("corrupted") },
                Experiment {
                    action: "Let the new version have a bug that makes it hang.",
                    watch: "version 2 running on trial, never confirming, and the bootloader swapping version 1 back after the watchdog reset. Then try the same without a bootloader.",
                    preset: Some("faulty"),
                },
            ],
            leaves_out: "A power cut during the swap itself is not shown; real bootloaders record their progress so that a swap \
                can be resumed. The scheme here (swap, run on trial, confirm or revert) is the one used by the open-source \
                bootloader MCUboot; others copy instead of swapping, or simply alternate between two slots. Checking a \
                signature needs keys and is a topic of its own. Chips also have a factory bootloader in read-only memory, used \
                on the production line. And before any of this, a real start-up also sets up the clock, the stack and the \
                memory for variables.",
            refs: &[
                "ARMv7-M reset behaviour: the processor loads the initial stack pointer and the reset vector from the start of the vector table.",
                "MCUboot design: primary and secondary image slots, swap with test, image confirmation and revert (docs.mcuboot.com).",
            ],
        },
    }
}
