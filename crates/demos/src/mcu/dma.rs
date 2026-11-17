use crate::prelude::*;
use egui_plot::Polygon;
use sim_core::mcu::ByteStream;

/// How many byte times the timeline shows.
const WINDOW_BYTES: usize = 32;

pub struct Dma {
    s: ByteStream,
    use_dma: bool,
}

impl Default for Dma {
    fn default() -> Self {
        Self { s: ByteStream { baud: 1e6, cpu_hz: 48e6, cycles_per_byte_irq: 150.0, block_bytes: 16, cycles_per_block_irq: 300.0, blocked_us: 0.0 }, use_dma: false }
    }
}

impl Demo for Dma {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        let s = &mut self.s;
        widgets::group(ui, "A stream of bytes arriving on a serial line", |ui| {
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "baud", |ui| {
                    ui.add(widgets::si_slider(&mut s.baud, 9600.0..=3e6, "line speed", "bit/s").logarithmic(true)).on_hover_text("Bits per second on the wire. Each byte takes 10 bits.")
                });
                widgets::wrap(ui, "cpu", |ui| {
                    ui.add(widgets::si_slider(&mut s.cpu_hz, 1e6..=120e6, "processor clock", "Hz").logarithmic(true)).on_hover_text("A slower processor needs longer for the same handler.")
                });
                widgets::wrap(ui, "blocked", |ui| {
                    ui.add(egui::Slider::new(&mut s.blocked_us, 0.0..=200.0).text("processor busy elsewhere for up to").suffix(" µs").fixed_decimals(0))
                        .on_hover_text("The longest time another, more urgent interrupt handler keeps the processor from answering.")
                });
            });
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut self.use_dma, "let DMA store the bytes").on_hover_text("DMA hardware copies each byte into memory by itself and calls the processor only when a whole block is full.");
                widgets::wrap(ui, "block", |ui| {
                    ui.add_enabled(self.use_dma, egui::Slider::new(&mut s.block_bytes, 4..=1024).logarithmic(true).text("block size").suffix(" bytes")).on_hover_text("How many bytes DMA collects before it tells the processor.")
                });
            });
        });
        let s = self.s;
        let byte_us = s.byte_time_us();
        let span = WINDOW_BYTES as f64 * byte_us;
        let (scale, unit) = if span < 2000.0 { (1.0, "µs") } else { (1e-3, "ms") };
        let load = if self.use_dma { s.dma_load() } else { s.irq_load() };

        // Handler bars: one after every byte, or one after every full block.
        let every = if self.use_dma { s.block_bytes as usize } else { 1 };
        let handler_us = if self.use_dma { s.cycles_per_block_irq / s.cpu_hz * 1e6 } else { s.irq_time_us() };
        let handlers: Vec<(f64, f64)> = (1..=WINDOW_BYTES).filter(|k| k % every == 0).map(|k| k as f64 * byte_us).map(|t| (t, (t + handler_us).min(span))).collect();

        ui.label(RichText::new(format!("What happens during {WINDOW_BYTES} bytes")).strong());
        let names = ["main program", "interrupt handler", "DMA hardware", "bytes on the wire"].map(str::to_owned).to_vec();
        plots::timeline("lanes", names, &format!("time [{unit}]"), span * scale).show(ui, |p| {
            let mut bar = |name: &str, from: f64, to: f64, row: f64, half: f64, fill: Color32| {
                p.polygon(Polygon::new(name, plots::bar(from * scale, to * scale, row - half, row + half)).fill_color(fill).stroke((0.0, fill)));
            };
            for k in 0..WINDOW_BYTES {
                let t = k as f64 * byte_us;
                bar("byte", t + 0.04 * byte_us, t + 0.96 * byte_us, 3.0, 0.3, pal.trace[0].gamma_multiply(0.8));
                if self.use_dma {
                    bar("DMA copies one byte", t + 0.96 * byte_us, t + 1.04 * byte_us, 2.0, 0.3, pal.trace[2]);
                }
            }
            let mut main_from = 0.0;
            for &(from, to) in &handlers {
                bar("handler", from, to, 1.0, 0.3, pal.trace[3]);
                if from > main_from {
                    bar("main program", main_from, from, 0.0, 0.3, pal.dim.gamma_multiply(0.7));
                }
                main_from = main_from.max(to);
            }
            if main_from < span {
                bar("main program", main_from, span, 0.0, 0.3, pal.dim.gamma_multiply(0.7));
            }
        });
        widgets::key(ui, &[(pal.trace[0], "a byte arriving"), (pal.trace[2], "DMA hardware stores it"), (pal.trace[3], "the processor runs an interrupt handler"), (pal.dim, "the processor runs the main program")]);

        let loses = !self.use_dma && s.irq_loses_bytes();
        widgets::status(
            ui,
            !loses,
            if loses {
                format!("bytes are lost: a byte lasts {}, but the processor may need {} before it has fetched the previous one", si(byte_us * 1e-6, "s"), si((s.blocked_us + s.irq_time_us()) * 1e-6, "s"))
            } else {
                format!("no byte is lost; {:.2} % of the processor is spent on receiving", load * 100.0)
            },
        );
        widgets::formula(
            ui,
            format!(
                "bytes per second     = line speed / 10 bits = {:.0}        one byte lasts {}\n\
                 interrupt per byte   : {:.0} x {:.0} cycles / {} = {:.1} % of the processor\n\
                 DMA, {:>4}-byte blocks: {:.0} / {} x {:.0} cycles / {} = {:.2} % of the processor",
                s.bytes_per_s(),
                si(byte_us * 1e-6, "s"),
                s.bytes_per_s(),
                s.cycles_per_byte_irq,
                si(s.cpu_hz, "Hz"),
                s.irq_load() * 100.0,
                s.block_bytes,
                s.bytes_per_s(),
                s.block_bytes,
                s.cycles_per_block_irq,
                si(s.cpu_hz, "Hz"),
                s.dma_load() * 100.0,
            ),
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "dma" => self.use_dma = true,
            "slow-cpu" => self.s.cpu_hz = 8e6,
            "blocked" => self.s.blocked_us = 50.0,
            "blocked-dma" => {
                self.s.blocked_us = 50.0;
                self.use_dma = true;
            }
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "mcu-dma",
        title: "DMA: moving data without the CPU",
        blurb: "Bytes pour in a hundred thousand times a second. Let the processor fetch each one, or let hardware do it?",
        path: &["Inside the microcontroller"],
        make: || Box::new(Dma::default()),
        explain: Explanation {
            what: "The top row shows bytes arriving on a serial line, one after the other. The rows below show who deals with \
                them. In the opening state it is the processor: after every single byte an [[interrupt]] [[handler]] runs to \
                store it (second row from the bottom), and the main program only gets the gaps in between (bottom row).\n\n\
                Tick \"let DMA store the bytes\" and a third player appears: [[DMA]] hardware stores each byte, and the \
                processor is only called once per block.",
            why: "A receiver for a serial line has room for exactly one finished byte. Someone has to take that byte away and \
                put it into memory before the next one is complete, or it is overwritten and lost.\n\n\
                The obvious way is an interrupt per byte. That works well at low speeds. But every interrupt costs the same \
                fixed effort - save the registers, run the handler, restore the registers - whether it moves one byte or a \
                thousand. At a hundred thousand bytes per second that overhead alone can eat a third of the processor. Worse, \
                the handler has to start within one byte time. If anything more urgent holds the processor for longer, a \
                byte is gone.\n\n\
                DMA, direct memory access, is a small piece of hardware built for exactly this chore. You tell it once: take \
                each byte from this receiver and store it at this place in memory, one after the other, for so many bytes. \
                From then on it does the copying itself, in a few clock cycles per byte, without the processor noticing. \
                Only when the block is full does it raise one interrupt.\n\n\
                The processor's work shrinks by the block size, and the deadline relaxes from one byte time to a whole \
                block. The same trick is used wherever data flows steadily: samples from an analog converter, pixels to a \
                display, audio to a speaker.",
            formulas: &[
                ("load with interrupts = bytes per second x cycles per interrupt / processor clock", "100 000 x 150 / 48 000 000 = 31 %."),
                ("load with DMA = bytes per second / block size x cycles per interrupt / processor clock", "100 000 / 16 x 300 / 48 000 000 = 3.9 %; with 256-byte blocks 0.24 %."),
            ],
            try_this: &[
                Experiment { action: "Tick \"let DMA store the bytes\".", watch: "the handler row emptying to two short bars and the main program getting nearly all the time back.", preset: Some("dma") },
                Experiment {
                    action: "Without DMA, slow the processor to 8 MHz.",
                    watch: "the handler bars becoming longer than the bytes themselves: the processor cannot keep up, bytes are lost, and the main program never runs.",
                    preset: Some("slow-cpu"),
                },
                Experiment {
                    action: "Back at 48 MHz, let something else keep the processor busy for up to 50 µs.",
                    watch: "the status turning red although the load is only 31 %: five bytes go by in 50 µs and the receiver can hold one.",
                    preset: Some("blocked"),
                },
                Experiment { action: "Same situation, with DMA.", watch: "no byte lost: the hardware does not wait for the processor.", preset: Some("blocked-dma") },
            ],
            leaves_out: "The cycle counts (150 per byte interrupt, 300 per block interrupt) are plausible example values; real \
                ones depend on the handler. DMA is drawn as costing the processor nothing; in reality it borrows the memory \
                bus for a few cycles per byte. The block here always fills up: real data often arrives in messages of unknown \
                length, which needs an extra \"line went quiet\" interrupt or a circular buffer. Many receivers also have a \
                small built-in queue (FIFO) of a few bytes, which eases the per-byte deadline somewhat.",
            refs: &[
                "Generic DMA controller of ARM Cortex-M microcontrollers: peripheral-to-memory transfer with a transfer-complete interrupt.",
                "Asynchronous serial framing 8N1: 10 bit times per byte; receiver overrun when a new byte completes before the previous one was read.",
            ],
        },
    }
}
