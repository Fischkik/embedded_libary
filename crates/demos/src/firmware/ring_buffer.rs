use crate::prelude::*;
use sim_core::firmware::Ring;

pub struct RingBuffer {
    ring: Ring,
    next: u8,
    last_read: Option<u8>,
    last_put_ok: bool,
}

impl RingBuffer {
    fn with_capacity(capacity: usize) -> Self {
        Self { ring: Ring::new(capacity), next: 0x41, last_read: None, last_put_ok: true }
    }

    fn put(&mut self) {
        self.last_put_ok = self.ring.put(self.next);
        if self.last_put_ok {
            self.next = if self.next >= 0x5a { 0x41 } else { self.next + 1 };
        }
    }
}

impl Default for RingBuffer {
    fn default() -> Self {
        // Six in, four out, three in: the write position has wrapped around.
        let mut d = Self::with_capacity(8);
        (0..6).for_each(|_| d.put());
        (0..4).for_each(|_| d.last_read = d.ring.get());
        (0..3).for_each(|_| d.put());
        d
    }
}

impl Demo for RingBuffer {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        let (head_before, tail_before) = (self.ring.head, self.ring.tail);
        widgets::group(ui, "A buffer between a fast writer and a slow reader", |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Write a byte").on_hover_text("The writer (for example an interrupt handler receiving serial data) stores the next letter at the head.").clicked() {
                    self.put();
                }
                if ui.button("Write 4").on_hover_text("Four bytes arrive in a burst.").clicked() {
                    (0..4).for_each(|_| self.put());
                }
                if ui.button("Read a byte").on_hover_text("The reader (for example the main loop) takes the oldest byte from the tail.").clicked() {
                    self.last_read = self.ring.get();
                }
                if ui.button("Read all").on_hover_text("The reader catches up completely.").clicked() {
                    while let Some(b) = self.ring.get() {
                        self.last_read = Some(b);
                    }
                }
                ui.add_space(12.0);
                ui.label("Size:");
                for cap in [4, 8, 16] {
                    if ui.selectable_label(self.ring.capacity() == cap, format!("{cap} bytes")).on_hover_text("The buffer's fixed size. Changing it empties the buffer.").clicked() && self.ring.capacity() != cap {
                        *self = Self::with_capacity(cap);
                    }
                }
            });
        });
        let r = &self.ring;
        let cap = r.capacity();

        ui.label(RichText::new("The buffer's memory: a fixed row of slots").strong());
        let cw = ((ui.available_width().min(760.0) - 8.0) / cap as f32).min(64.0);
        let (rect, _) = ui.allocate_exact_size(vec2(cw * cap as f32, 104.0), egui::Sense::hover());
        let painter = ui.painter();
        for i in 0..cap {
            let cell = egui::Rect::from_min_size(pos2(rect.left() + i as f32 * cw, rect.top() + 18.0), vec2(cw - 3.0, 40.0));
            let used = r.is_used(i);
            painter.rect_filled(cell, 3.0, if used { pal.trace[0].gamma_multiply(0.55) } else { pal.canvas });
            painter.rect_stroke(cell, 3.0, (1.0, pal.dim), egui::StrokeKind::Inside);
            painter.text(pos2(cell.center().x, rect.top() + 8.0), egui::Align2::CENTER_CENTER, format!("{i}"), egui::FontId::monospace(11.0), pal.dim);
            let ch = r.slots[i];
            if ch != 0 {
                painter.text(cell.center(), egui::Align2::CENTER_CENTER, format!("{}", ch as char), egui::FontId::monospace(17.0), if used { ui.visuals().strong_text_color() } else { pal.dim.gamma_multiply(0.6) });
            }
            let mark = |text: &str, row: f32, color: Color32| {
                painter.text(pos2(cell.center().x, cell.bottom() + 10.0 + row * 15.0), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(12.0), color);
            };
            if i == r.head {
                mark("^ head", 0.0, pal.trace[1]);
            }
            if i == r.tail {
                mark("^ tail", 1.0, pal.trace[2]);
            }
        }
        widgets::key(ui, &[(pal.trace[0].gamma_multiply(0.55), "slot holds a byte not read yet"), (pal.canvas, "free slot (old letters in grey are leftovers)"), (pal.trace[1], "head: the next write goes here"), (pal.trace[2], "tail: the next read comes from here")]);

        widgets::status(
            ui,
            self.last_put_ok,
            if !self.last_put_ok {
                format!("the buffer is full: the last write was refused. {} byte(s) lost so far", r.dropped)
            } else if r.count == 0 {
                "the buffer is empty: a read would get nothing".to_owned()
            } else {
                format!("{} of {cap} slots in use{}", r.count, self.last_read.map_or(String::new(), |b| format!("; the last byte read was '{}'", b as char)))
            },
        );
        widgets::formula(
            ui,
            format!(
                "after a write: head = (head + 1) mod size        head was {head_before}, is {} now\n\
                 after a read : tail = (tail + 1) mod size        tail was {tail_before}, is {} now\n\
                 in use       = (head - tail) mod size = ({} - {}) mod {cap} = {}{}\n\n\
                 bool put(uint8_t byte)\n{{\n    if (count == SIZE) return false;      /* full: refuse, never overwrite */\n    buf[head] = byte;\n    head = (head + 1) % SIZE;             /* step on, back to 0 after the last slot */\n    count++;\n    return true;\n}}",
                r.head,
                r.tail,
                r.head,
                r.tail,
                (r.head + cap - r.tail) % cap,
                if r.is_full() { "   <- 0 can mean empty or full: that is why this code also keeps a count" } else { "" },
            ),
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "full" => (0..5).for_each(|_| self.put()),
            "empty" => {
                while let Some(b) = self.ring.get() {
                    self.last_read = Some(b);
                }
            }
            "small" => {
                *self = Self::with_capacity(4);
                (0..3).for_each(|_| self.put());
            }
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "firmware-ring-buffer",
        title: "Ring buffers",
        blurb: "A row of memory slots used in a circle: the standard way to hand bytes from an interrupt to the main program.",
        path: &["Firmware craft"],
        make: || Box::new(RingBuffer::default()),
        explain: Explanation {
            what: "The row of boxes is a small block of memory, eight bytes long. A writer puts letters in, one after the other, \
                at the position marked head. A reader takes them out at the position marked tail. Blue slots hold bytes that \
                have been written but not yet read.\n\n\
                In the opening state the writer has already reached the end of the row and carried on at slot 0: the buffer \
                has wrapped around.",
            why: "Data rarely arrives at the moment the program is ready for it. Bytes come in over a serial line whenever the \
                other side sends them, in an [[interrupt]]; the main program processes them when it gets round to it. In \
                between, the bytes have to wait somewhere, in the order they came.\n\n\
                The obvious container is an array that is filled from the front. But what happens when the reader has taken \
                out the first few bytes? Moving everything forward costs time on every read. The [[ring buffer]] avoids this \
                with a simple trick: nothing is ever moved. Two position numbers do all the work. Writing stores a byte at \
                head and moves head one step on. Reading takes the byte at tail and moves tail one step on. When either \
                reaches the end of the array, it continues at the beginning. The memory is used in a circle, hence the \
                name.\n\n\
                The bytes between tail and head are the contents; everything else is free. The buffer is empty when the \
                reader has caught up with the writer, and full when the writer has caught up with the reader from behind. \
                A full buffer is the one decision the programmer has to make: refuse the new byte, as here, or overwrite \
                the oldest. Either way data is lost, so the size must cover the longest burst the reader lets pile up.\n\n\
                Its memory is fixed, it never needs the [[heap]], each operation takes the same few instructions, and writer \
                and reader touch different variables - which is what makes it safe between an interrupt and the main \
                program. That is why it appears in practically every firmware.",
            formulas: &[
                ("next position = (position + 1) mod size", "With 8 slots, the step after slot 7 is slot 0."),
                ("bytes in use = (head - tail) mod size", "head 1, tail 4, size 8: (1 - 4) mod 8 = 5 bytes waiting."),
            ],
            try_this: &[
                Experiment {
                    action: "Write five more bytes without reading.",
                    watch: "the buffer filling up and the last writes being refused: head has caught up with tail from behind, and the status line counts the lost bytes.",
                    preset: Some("full"),
                },
                Experiment {
                    action: "Read everything.",
                    watch: "tail walking round to where head is. The letters are still in memory, in grey, but the buffer is empty: only the two positions decide what counts.",
                    preset: Some("empty"),
                },
                Experiment {
                    action: "Switch to 4 bytes, then alternate: write one, read one, many times.",
                    watch: "head and tail chasing each other round and round the four slots. The buffer handles any amount of data, as long as it never holds more than four at once.",
                    preset: Some("small"),
                },
            ],
            leaves_out: "This version keeps a count besides head and tail. A common variant has no count and leaves one slot \
                always empty, so that \"head equals tail\" can only mean empty; that variant is the one that is safe without \
                any locking between one interrupt and the main program. With a size that is a power of two, the \"mod\" becomes \
                a cheap bit mask. Real buffers hold more than letters - whole messages, samples, log entries. Zephyr's \
                message queue from the Zephyr path is a ring buffer with waiting built in.",
            refs: &["Circular buffer (FIFO) with head and tail indices: standard data structure, e.g. D. Knuth, \"The Art of Computer Programming\", vol. 1, section 2.2.2 (sequential allocation of queues)."],
        },
    }
}
