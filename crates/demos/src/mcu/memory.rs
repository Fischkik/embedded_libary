use crate::prelude::*;
use sim_core::mcu::{RAM_BASE, RamLayout};

pub struct Memory {
    ram: RamLayout,
}

impl Default for Memory {
    fn default() -> Self {
        Self { ram: RamLayout { ram_bytes: 8192, globals_bytes: 1024, heap_bytes: 2048, call_depth: 8, frame_bytes: 64 } }
    }
}

fn bytes(n: i64) -> String {
    if n.abs() >= 1024 && n % 1024 == 0 { format!("{} KB", n / 1024) } else { format!("{n} bytes") }
}

impl Demo for Memory {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        let r = &mut self.ram;
        widgets::group(ui, "The program", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("RAM size:");
                for kb in [2, 8, 32] {
                    ui.selectable_value(&mut r.ram_bytes, kb * 1024, format!("{kb} KB")).on_hover_text("How much working memory the chip has. Small microcontrollers have a few kilobytes.");
                }
            });
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "globals", |ui| {
                    ui.add(egui::Slider::new(&mut r.globals_bytes, 0..=8192).text("global variables").suffix(" bytes")).on_hover_text("Variables that exist for the whole life of the program. Their total size is fixed when the program is built.")
                });
                widgets::wrap(ui, "heap", |ui| {
                    ui.add(egui::Slider::new(&mut r.heap_bytes, 0..=16384).text("heap in use").suffix(" bytes")).on_hover_text("Memory the program has asked for while running (malloc). It grows upward from the end of the global variables.")
                });
            });
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "depth", |ui| {
                    ui.add(egui::Slider::new(&mut r.call_depth, 0..=200).logarithmic(true).text("function calls nested")).on_hover_text("How many functions are in the middle of running: main called A, A called B, B called C ... Each one occupies stack until it returns.")
                });
                widgets::wrap(ui, "frame", |ui| {
                    ui.add(egui::Slider::new(&mut r.frame_bytes, 8..=2048).logarithmic(true).text("stack per call").suffix(" bytes")).on_hover_text("What one call puts on the stack: the place to return to, saved registers, and the function's local variables. A big local array makes this big.")
                });
            });
        });
        let r = self.ram;
        let free = r.free_bytes();

        ui.label(RichText::new("The chip's RAM, lowest address at the bottom").strong());
        let (h, w) = (360.0f32, 190.0f32);
        let (rect, _) = ui.allocate_exact_size(vec2((w + 420.0).min(ui.available_width()), h + 24.0), egui::Sense::hover());
        let bar = egui::Rect::from_min_size(rect.min + vec2(0.0, 12.0), vec2(w, h));
        let painter = ui.painter();
        let y_of = |b: f64| bar.bottom() - (b / r.ram_bytes as f64).clamp(0.0, 1.0) as f32 * h;
        let font = egui::FontId::monospace(13.0);
        let small = egui::FontId::proportional(13.0);
        painter.rect_filled(bar, 0.0, pal.canvas);
        let globals_end = r.globals_bytes as f64;
        let heap_end = globals_end + r.heap_bytes as f64;
        let stack_low = r.ram_bytes as f64 - r.stack_bytes() as f64;
        let segment = |from: f64, to: f64, color: Color32, name: &str| {
            let seg = egui::Rect::from_min_max(pos2(bar.left(), y_of(to)), pos2(bar.right(), y_of(from)));
            painter.rect_filled(seg, 0.0, color);
            if seg.height() > 18.0 {
                painter.text(seg.center(), egui::Align2::CENTER_CENTER, name, small.clone(), if pal.dark { Color32::BLACK } else { Color32::WHITE });
            }
        };
        segment(0.0, globals_end, pal.trace[0], "global variables");
        segment(globals_end, heap_end, pal.trace[1], "heap (grows up)");
        segment(stack_low, r.ram_bytes as f64, pal.trace[2], "stack (grows down)");
        if free < 0 {
            segment(stack_low.max(0.0), heap_end.min(r.ram_bytes as f64), pal.bad, "OVERWRITTEN");
        } else if y_of(heap_end) - y_of(stack_low) > 18.0 {
            painter.text(pos2(bar.center().x, (y_of(heap_end) + y_of(stack_low)) / 2.0), egui::Align2::CENTER_CENTER, "free", small.clone(), pal.dim);
        }
        painter.rect_stroke(bar, 0.0, (1.0, pal.dim), egui::StrokeKind::Outside);
        // Address labels, pushed apart so they never overlap.
        let mut labels = vec![
            (y_of(r.ram_bytes as f64), format!("0x{:08X}  top of RAM: the stack starts here", r.stack_top())),
            (y_of(stack_low), format!("0x{:08X}  stack pointer", r.stack_pointer())),
            (y_of(heap_end), format!("0x{:08X}  end of the heap", r.heap_end())),
            (y_of(globals_end), format!("0x{:08X}  end of the global variables", r.heap_start())),
            (y_of(0.0), format!("0x{RAM_BASE:08X}  start of RAM")),
        ];
        labels.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut y = f32::NEG_INFINITY;
        for (at, text) in labels {
            y = at.max(y + 17.0);
            painter.line_segment([pos2(bar.right(), at), pos2(bar.right() + 10.0, y)], (1.0, pal.dim));
            painter.text(pos2(bar.right() + 14.0, y), egui::Align2::LEFT_CENTER, text, font.clone(), pal.text);
        }

        widgets::status(
            ui,
            free >= 0,
            if free >= 0 {
                format!("{} free between heap and stack", bytes(free))
            } else {
                format!("stack overflow: the stack has grown {} into the heap{}. Nothing stops it - the data is silently overwritten", bytes(-free), if stack_low < globals_end { " and the global variables" } else { "" })
            },
        );
        widgets::formula(
            ui,
            format!(
                "stack in use = calls nested x stack per call = {} x {} = {}\n\
                 free         = RAM - globals - heap - stack = {} - {} - {} - {} = {} bytes",
                r.call_depth,
                r.frame_bytes,
                bytes(r.stack_bytes() as i64),
                r.ram_bytes,
                r.globals_bytes,
                r.heap_bytes,
                r.stack_bytes(),
                free
            ),
        );
        widgets::group(ui, "Where everything lives on an ARM Cortex-M: one 32-bit address space", |ui| {
            egui::Grid::new("map").striped(true).num_columns(2).show(ui, |ui| {
                for (range, what) in [
                    ("0xE0000000 - 0xFFFFFFFF", "the processor's own controls (interrupt controller, system timer, debug)"),
                    ("0x40000000 - 0x5FFFFFFF", "peripherals: writing to an address here switches a pin or starts a timer"),
                    ("0x20000000 - 0x3FFFFFFF", "RAM: the picture above. Lost when the power goes"),
                    ("0x00000000 - 0x1FFFFFFF", "code: the program and its constants, in flash memory. Kept without power"),
                ] {
                    ui.monospace(range);
                    ui.label(what);
                    ui.end_row();
                }
            });
        });
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "recursion" => self.ram.call_depth = 90,
            "big-local" => self.ram.frame_bytes = 1024,
            "heap" => self.ram.heap_bytes = 6800,
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "mcu-memory",
        title: "Memory map, stack and heap",
        blurb: "A few kilobytes of RAM, shared by three things that grow towards each other. See what happens when they meet.",
        path: &["Inside the microcontroller"],
        make: || Box::new(Memory::default()),
        explain: Explanation {
            what: "The tall bar is the microcontroller's RAM, its working memory, with the lowest address at the bottom. Three \
                things live in it. At the bottom the global variables. Above them the [[heap]], which grows upward when the \
                program asks for more memory. At the very top the [[stack]], which grows downward with every function call. \
                The numbers on the right are the addresses of the borders.\n\n\
                The table underneath shows the bigger picture: RAM is only one region of the chip's address space.",
            why: "A microcontroller has no operating system that hands out memory. There is one block of RAM, often only a few \
                thousand bytes, and the firmware divides it up itself.\n\n\
                Global variables are simple: their total size is known when the program is built, so they get a fixed block \
                at the start of RAM.\n\n\
                The stack is the processor's notepad. Every time a function is called, the processor notes where to return \
                to, saves the registers it is about to reuse, and makes room for the function's local variables. When the \
                function returns, all of that is released again. So the stack grows and shrinks all the time, and how deep it \
                gets depends on how many calls are nested and how big their local variables are. On ARM Cortex-M the stack \
                starts at the top of RAM and grows downward; the [[stack pointer]] marks its current end.\n\n\
                The heap is for memory whose size is only known while the program runs. It starts right after the global \
                variables and grows upward.\n\n\
                Stack and heap therefore grow towards each other, and nothing stands between them. If they meet, the stack \
                simply writes over the heap or the global variables: a stack overflow. No error appears. Some variable just \
                changes its value by itself, and the firmware misbehaves minutes later in a completely different place. This \
                is why embedded programmers avoid recursion and large local arrays, and why many avoid the heap altogether.",
            formulas: &[
                ("stack in use = calls nested x stack per call", "8 nested calls of 64 bytes each: 512 bytes."),
                ("free = RAM - globals - heap - stack", "8192 - 1024 - 2048 - 512 = 4608 bytes of margin."),
            ],
            try_this: &[
                Experiment {
                    action: "Let functions call each other 90 levels deep, as a recursive function easily does.",
                    watch: "the stack growing down through the free space and into the heap. The status line turns red, but on a real chip nothing would tell you.",
                    preset: Some("recursion"),
                },
                Experiment {
                    action: "Back to 8 nested calls, but give each a 1 KB local array.",
                    watch: "the same collision with just eight calls: one careless \"char buffer[1024]\" inside a function is enough.",
                    preset: Some("big-local"),
                },
                Experiment {
                    action: "Let the heap grow to 6800 bytes.",
                    watch: "the collision coming from the other side: now the next few function calls overwrite memory the program has only just asked for.",
                    preset: Some("heap"),
                },
                Experiment { action: "Switch the RAM size to 2 KB.", watch: "how little room there is on a small chip: the opening program no longer fits at all.", preset: None },
            ],
            leaves_out: "Real stack frames differ in size from function to function, and interrupts use stack too, at moments you \
                do not choose. A real heap also gets fragmented: enough bytes are free in total, but not in one piece. With an \
                RTOS every task has its own, much smaller stack. Chips with a memory protection unit can place a guard below \
                the stack so that an overflow is caught instead of going unnoticed. The address ranges in the table are the \
                regions the ARM architecture reserves; a real chip fills only a small part of each, and where exactly its \
                flash and RAM sit is in its datasheet.",
            refs: &[
                "ARMv7-M Architecture Reference Manual, system address map: Code 0x00000000, SRAM 0x20000000, Peripheral 0x40000000, System 0xE0000000 (each region 0.5 GB).",
                "ARM Cortex-M procedure call standard: full-descending stack, initial stack pointer loaded from the first entry of the vector table.",
            ],
        },
    }
}
