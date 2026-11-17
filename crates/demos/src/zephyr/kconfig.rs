use crate::prelude::*;
use sim_core::zephyr::{Symbol, Why, kconfig};

/// A small excerpt in the spirit of Zephyr's real symbols. Sizes are illustrative.
const SYMBOLS: [Symbol; 8] = [
    Symbol { name: "GPIO", prompt: Some("GPIO driver: digital pins"), depends_on: &[], selects: &[], default_on: false, flash_kb: 2.0 },
    Symbol { name: "SERIAL", prompt: Some("Serial port driver"), depends_on: &[], selects: &[], default_on: false, flash_kb: 3.0 },
    Symbol { name: "UART_CONSOLE", prompt: Some("Console output on the serial port"), depends_on: &["SERIAL"], selects: &[], default_on: false, flash_kb: 1.0 },
    Symbol { name: "LOG", prompt: Some("Logging"), depends_on: &[], selects: &[], default_on: false, flash_kb: 6.0 },
    Symbol { name: "LOG_BACKEND_UART", prompt: Some("Send log messages to the serial console"), depends_on: &["LOG", "UART_CONSOLE"], selects: &[], default_on: true, flash_kb: 1.0 },
    Symbol { name: "BT", prompt: Some("Bluetooth"), depends_on: &[], selects: &[], default_on: false, flash_kb: 60.0 },
    Symbol { name: "BT_PERIPHERAL", prompt: Some("Bluetooth peripheral role"), depends_on: &["BT"], selects: &["BT_CONN"], default_on: false, flash_kb: 12.0 },
    Symbol { name: "BT_CONN", prompt: None, depends_on: &[], selects: &[], default_on: false, flash_kb: 20.0 },
];
const KERNEL_KB: f64 = 12.0;

pub struct Kconfig {
    /// What prj.conf says per symbol: not mentioned, `=y`, or `=n`.
    wishes: [Option<bool>; 8],
}

impl Kconfig {
    fn with(on: &[&str], off: &[&str]) -> Self {
        let mut wishes = [None; 8];
        for (i, s) in SYMBOLS.iter().enumerate() {
            if on.contains(&s.name) {
                wishes[i] = Some(true);
            } else if off.contains(&s.name) {
                wishes[i] = Some(false);
            }
        }
        Self { wishes }
    }
}

impl Default for Kconfig {
    fn default() -> Self {
        Self::with(&["GPIO", "SERIAL", "UART_CONSOLE", "LOG"], &[])
    }
}

impl Demo for Kconfig {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        let prj: Vec<(&str, bool)> = SYMBOLS.iter().zip(&self.wishes).filter_map(|(s, w)| w.map(|v| (s.name, v))).collect();
        let r = kconfig(&SYMBOLS, &prj);

        widgets::group(ui, "The symbols, what prj.conf says, and what comes out", |ui| {
            egui::Grid::new("symbols").striped(true).num_columns(4).show(ui, |ui| {
                for h in ["in prj.conf", "symbol", "result", "why"] {
                    ui.label(RichText::new(h).strong());
                }
                ui.end_row();
                for (i, s) in SYMBOLS.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if s.prompt.is_some() {
                            ui.selectable_value(&mut self.wishes[i], None, "-").on_hover_text("Not mentioned in prj.conf: the symbol's default applies.");
                            ui.selectable_value(&mut self.wishes[i], Some(true), "=y").on_hover_text(format!("Write CONFIG_{}=y into prj.conf: ask for this feature.", s.name));
                            ui.selectable_value(&mut self.wishes[i], Some(false), "=n").on_hover_text(format!("Write CONFIG_{}=n into prj.conf: ask for this feature to be left out.", s.name));
                        } else {
                            ui.label(RichText::new("cannot be set").color(pal.dim)).on_hover_text("An invisible symbol: it has no prompt, so only other symbols can switch it on.");
                        }
                    });
                    ui.vertical(|ui| {
                        ui.add(egui::Label::new(RichText::new(format!("CONFIG_{}", s.name)).monospace()).extend());
                        ui.add(egui::Label::new(RichText::new(s.prompt.unwrap_or("Bluetooth connections (invisible symbol)")).small().color(pal.dim)).extend());
                    });
                    let (on, why) = r.values[i];
                    ui.label(RichText::new(if on { "y" } else { "n" }).monospace().strong().color(if on { pal.good } else { pal.dim }));
                    ui.add(egui::Label::new(match why {
                        Why::Assigned => "set in prj.conf".to_owned(),
                        Why::Default => format!("its default ({})", if s.default_on { "y" } else { "n" }),
                        Why::Selected(by) => format!("selected by CONFIG_{by}"),
                        Why::DependencyOff(dep) => format!("depends on CONFIG_{dep}, which is off"),
                    }).extend());
                    ui.end_row();
                }
            });
        });

        widgets::status(ui, r.warnings.is_empty(), if r.warnings.is_empty() { "every line of prj.conf took effect".to_owned() } else { format!("warning: {}", r.warnings.join("; ")) });

        let prj_text: String = if prj.is_empty() { "# (empty)\n".to_owned() } else { prj.iter().map(|(n, v)| format!("CONFIG_{n}={}\n", if *v { "y" } else { "n" })).collect() };
        let config_text: String = SYMBOLS.iter().zip(&r.values).map(|(s, (on, _))| if *on { format!("CONFIG_{}=y\n", s.name) } else { format!("# CONFIG_{} is not set\n", s.name) }).collect();
        ui.horizontal_wrapped(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("prj.conf - what you write").strong());
                widgets::formula(ui, prj_text.trim_end());
            });
            ui.vertical(|ui| {
                ui.label(RichText::new("build/zephyr/.config - what the build uses").strong());
                widgets::formula(ui, config_text.trim_end());
            });
        });
        let parts: Vec<String> = SYMBOLS.iter().zip(&r.values).filter(|(_, v)| v.0).map(|(s, _)| format!("{}", s.flash_kb)).collect();
        widgets::formula(
            ui,
            format!(
                "flash used = kernel + every feature that is on = {KERNEL_KB}{}{} = {} KB   (illustrative sizes)\n\
                 in the code:   if (IS_ENABLED(CONFIG_LOG)) {{ ... }}   is {} in this build",
                if parts.is_empty() { "" } else { " + " },
                parts.join(" + "),
                KERNEL_KB + r.flash_kb,
                if r.values[3].0 { "compiled in" } else { "left out completely" },
            ),
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = match preset {
            "missing-dep" => Self::with(&["GPIO", "SERIAL", "UART_CONSOLE", "LOG", "BT_PERIPHERAL"], &[]),
            "select" => Self::with(&["GPIO", "SERIAL", "UART_CONSOLE", "LOG", "BT", "BT_PERIPHERAL"], &[]),
            "ripple" => Self::with(&["GPIO", "UART_CONSOLE", "LOG"], &["SERIAL"]),
            _ => Self::default(),
        };
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "zephyr-kconfig",
        title: "Kconfig: switching features on and off",
        blurb: "A few lines of text decide which parts of Zephyr end up in your firmware - and why some lines are ignored.",
        path: &["Zephyr"],
        make: || Box::new(Kconfig::default()),
        explain: Explanation {
            what: "The table lists a handful of configuration symbols. In the first column you choose what the application's \
                configuration file, prj.conf, says about each one: nothing, =y (I want this) or =n (leave it out). The \
                \"result\" column is what the build really uses, and the last column says why.\n\n\
                Below are the two files side by side: the prj.conf you wrote and the complete configuration the build made \
                from it.",
            why: "[[Zephyr]] contains far more than any one device needs: drivers for hundreds of chips, a Bluetooth stack, \
                networking, file systems. On a microcontroller with a few hundred kilobytes of flash, everything that is not \
                needed must stay out. [[Kconfig]] is the system that decides. Every optional feature has a symbol whose name \
                starts with CONFIG_, and the build only compiles the code whose symbol is on.\n\n\
                You state your wishes in prj.conf, one line per symbol. But the result is not simply a copy of that file, \
                because symbols are connected by three rules, written down in Zephyr's Kconfig files.\n\n\
                Depends on: a symbol can only be on if the symbols it depends on are on. Console output on the serial port \
                is pointless without the serial driver. If you ask for a symbol whose dependency is off, your line is \
                ignored - with a warning in the build output that is easy to overlook.\n\n\
                Default: a symbol you do not mention gets its default value, and that default can be \"on\". This is how \
                enabling one thing quietly brings sensible companions with it.\n\n\
                Select: a symbol can force another one on. Some symbols are invisible - they have no prompt and cannot be \
                set in prj.conf at all; they exist only to be selected by others.\n\n\
                The outcome is written to a file called .config in the build folder, and as a header file of #define lines \
                for the C code. When a feature mysteriously does not work, that .config file is the first place to look: it \
                shows what the build really used, not what you asked for.",
            formulas: &[("flash used = kernel + sum of the features that are on", "Illustrative: 12 KB kernel + GPIO 2 + serial 3 + console 1 + logging 6 + log output 1 = 25 KB.")],
            try_this: &[
                Experiment {
                    action: "Ask for the Bluetooth peripheral role without switching Bluetooth itself on.",
                    watch: "the result staying \"n\" and a warning appearing: the line in prj.conf is there, but the build ignores it.",
                    preset: Some("missing-dep"),
                },
                Experiment {
                    action: "Now also set CONFIG_BT=y.",
                    watch: "three symbols turning on from two lines: CONFIG_BT_CONN, which cannot be set by hand, is selected by the peripheral role. And the flash use jumping.",
                    preset: Some("select"),
                },
                Experiment {
                    action: "Go back to the start and set CONFIG_SERIAL=n.",
                    watch: "the damage rippling down: the console loses its dependency, and with it the log output that was only on by default.",
                    preset: Some("ripple"),
                },
            ],
            leaves_out: "Only on/off symbols are shown; real ones can also be numbers and text (a stack size, a device name). The \
                eight symbols exist in Zephyr, but their real definitions have more conditions than shown, and the sizes are \
                made up to show the idea. A real configuration is merged from several files - the board's defaults, prj.conf, \
                optional extra files - and there is an interactive browser (menuconfig) that shows every symbol with its \
                dependencies. Kconfig decides about software; which hardware exists is the devicetree's job, and the two \
                work together.",
            refs: &[
                "Zephyr documentation, Build and Configuration Systems > Kconfig > Setting Kconfig configuration values: assignment syntax, visible and invisible symbols, \"Assignments in configuration files are only respected if the dependencies for the symbol are satisfied. A warning is printed otherwise\", merged configuration saved to zephyr/.config.",
                "Kconfig language: depends on, default, select.",
            ],
        },
    }
}
