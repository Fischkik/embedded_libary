use crate::prelude::*;
use egui_plot::{HLine, PlotPoint, Polygon, Text, VLine};
use sim_core::zephyr::{MsgqScenario, WhenFull, simulate_msgq};

const DURATION_MS: f64 = 100.0;

pub struct Msgq {
    s: MsgqScenario,
    inspect_ms: f64,
}

impl Default for Msgq {
    fn default() -> Self {
        Self { s: MsgqScenario { capacity: 3, when_full: WhenFull::DropNew, burst: 5, spacing_ms: 1.0, burst_period_ms: 40.0, work_ms: 4.5 }, inspect_ms: 8.5 }
    }
}

impl Demo for Msgq {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        let s = &mut self.s;
        widgets::group(ui, "Producer, queue and consumer", |ui| {
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "burst", |ui| ui.add(egui::Slider::new(&mut s.burst, 1..=8).text("items per burst")).on_hover_text("The producer makes data items in groups, for example a run of sensor readings."));
                widgets::wrap(ui, "spacing", |ui| ui.add(egui::Slider::new(&mut s.spacing_ms, 0.5..=10.0).logarithmic(true).text("apart").suffix(" ms").fixed_decimals(1)).on_hover_text("Time between two items inside a burst."));
                widgets::wrap(ui, "period", |ui| ui.add(egui::Slider::new(&mut s.burst_period_ms, 10.0..=100.0).text("a burst every").suffix(" ms").fixed_decimals(0)).on_hover_text("How often a new burst starts."));
            });
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "capacity", |ui| ui.add(egui::Slider::new(&mut s.capacity, 1..=8).text("queue capacity").suffix(" items")).on_hover_text("How many items the queue can hold. Its memory is reserved once, when the queue is created."));
                widgets::wrap(ui, "work", |ui| ui.add(egui::Slider::new(&mut s.work_ms, 0.5..=30.0).logarithmic(true).text("consumer needs per item").suffix(" ms").fixed_decimals(1)).on_hover_text("How long the consumer thread works on one item."));
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("When the queue is full, the producer:");
                ui.selectable_value(&mut s.when_full, WhenFull::DropNew, "throws the new item away").on_hover_text("k_msgq_put with K_NO_WAIT returns an error at once; the producer gives up on this item.");
                ui.selectable_value(&mut s.when_full, WhenFull::Wait, "waits for space").on_hover_text("k_msgq_put with K_FOREVER: the producer thread sleeps until the consumer has taken an item. Not possible in an interrupt handler.");
                ui.selectable_value(&mut s.when_full, WhenFull::PurgeOld, "empties the queue first").on_hover_text("k_msgq_purge, then put: all older items are thrown away so that the newest is kept.");
            });
            ui.add(egui::Slider::new(&mut self.inspect_ms, 0.0..=DURATION_MS - 0.1).text("look into the queue at").suffix(" ms").fixed_decimals(1)).on_hover_text("Pick a moment. The row of boxes below shows which items are in the queue then.");
        });
        let s = self.s;
        let tr = simulate_msgq(&s, DURATION_MS, self.inspect_ms);

        ui.label(RichText::new("Items made, and the consumer working through them (numbers = item numbers)").strong());
        plots::timeline("lanes", vec!["consumer".to_owned(), "producer".to_owned()], "time [ms]", DURATION_MS).show(ui, |p| {
            for &(from, to) in &tr.blocked {
                p.polygon(Polygon::new("producer blocked", plots::bar(from, to, 0.9, 1.1)).fill_color(pal.warn).stroke((0.0, pal.warn)));
            }
            for &(t, _, fate) in &tr.made {
                let fill = [pal.trace[1], pal.bad, pal.warn][fate as usize];
                p.polygon(Polygon::new("item", plots::bar(t - 0.15, t + 0.15, 0.7, 1.3)).fill_color(fill).stroke((0.0, fill)));
            }
            for &(id, from, to) in &tr.working {
                let fill = pal.trace[2].gamma_multiply(0.8);
                p.polygon(Polygon::new("working", plots::bar(from, to.min(DURATION_MS), -0.3, 0.3)).fill_color(fill).stroke((1.0, pal.canvas)));
                if to - from >= 2.0 {
                    p.text(Text::new("item number", PlotPoint::new((from + to) / 2.0, 0.0), format!("{id}")).color(pal.text));
                }
            }
            p.vline(VLine::new("inspected moment", self.inspect_ms).color(pal.dim));
        });
        widgets::key(ui, &[(pal.trace[1], "item put into the queue"), (pal.bad, "item thrown away"), (pal.warn, "producer has to wait"), (pal.trace[2], "consumer works on an item")]);

        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(format!("The queue at {:.1} ms, oldest item first:", self.inspect_ms)).strong());
            for slot in 0..s.capacity {
                let (rect, _) = ui.allocate_exact_size(vec2(34.0, 26.0), egui::Sense::hover());
                let item = tr.snapshot.get(slot);
                ui.painter().rect_filled(rect, 3.0, if item.is_some() { pal.trace[1].gamma_multiply(0.6) } else { pal.canvas });
                ui.painter().rect_stroke(rect, 3.0, (1.0, pal.dim), egui::StrokeKind::Inside);
                if let Some(id) = item {
                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, format!("{id}"), egui::FontId::monospace(14.0), pal.text);
                }
            }
        });
        plots::plot_bare("fill", 130.0).x_axis_label("time [ms]").y_axis_label("items queued").include_x(0.0).include_x(DURATION_MS).include_y(0.0).include_y(s.capacity as f64 + 0.5).show(ui, |p| {
            p.line(plots::line("items in the queue", tr.fill.clone(), pal.trace[0]).width(2.0));
            p.hline(HLine::new("capacity", s.capacity as f64).color(pal.bad).style(egui_plot::LineStyle::dashed_dense()));
            p.vline(VLine::new("inspected moment", self.inspect_ms).color(pal.dim));
        });

        let made = tr.made.len() as u32;
        widgets::status(
            ui,
            tr.discarded == 0,
            if tr.discarded == 0 {
                format!("all {} items handled so far arrived; the slowest waited {:.1} ms{}", tr.delivered, tr.worst_latency_ms, if tr.blocked.is_empty() { "" } else { " - but the producer was held up" })
            } else {
                format!("{} of {made} items were thrown away", tr.discarded)
            },
        );
        widgets::formula(
            ui,
            format!(
                "queue slots a burst needs = (items - 1) - items the consumer finishes meanwhile\n\
                 \x20                         = ({} - 1) - floor({} x {:.1} ms / {:.1} ms) = {}        the queue has {}\n\n\
                 /* name, size of one item, capacity, alignment */\n\
                 K_MSGQ_DEFINE(my_msgq, sizeof(struct reading), {}, 4);\n\
                 k_msgq_put(&my_msgq, &reading, {});   /* producer: copies the item in */\n\
                 k_msgq_get(&my_msgq, &reading, K_FOREVER);   /* consumer: sleeps until one is there */",
                s.burst,
                s.burst.saturating_sub(1),
                s.spacing_ms,
                s.work_ms,
                (s.burst.saturating_sub(1) as f64 - (s.burst.saturating_sub(1) as f64 * s.spacing_ms / s.work_ms).floor()).max(0.0),
                s.capacity,
                s.capacity,
                if s.when_full == WhenFull::Wait { "K_FOREVER" } else { "K_NO_WAIT" },
            ),
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "bigger" => self.s.capacity = 4,
            "wait" => (self.s.when_full, self.s.burst) = (WhenFull::Wait, 7),
            "purge" => (self.s.when_full, self.inspect_ms) = (WhenFull::PurgeOld, 9.2),
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "zephyr-msgq",
        title: "Message queues: passing data between threads",
        blurb: "A fast producer, a slow consumer, and a queue in between. What happens when the queue is full?",
        path: &["Zephyr"],
        make: || Box::new(Msgq::default()),
        explain: Explanation {
            what: "The top row shows a producer making numbered data items, five in quick succession. The row below shows a \
                consumer [[thread]] working through them one by one; the number in each bar is the item it is working on. \
                Between the two is a [[message queue]].\n\n\
                The row of boxes shows what is in the queue at the moment marked by the vertical line - move it with the last \
                slider. The lower graph is the number of queued items over time, with the queue's capacity as a dashed line.",
            why: "A [[semaphore]] can tell a thread that something happened, but not what. When data has to travel - a sensor \
                reading, a received byte, a command - [[Zephyr]] offers the message queue: a fixed number of slots, each big \
                enough for one item of a fixed size.\n\n\
                Putting an item copies it into the next free slot; getting copies the oldest item out and frees its slot. \
                Because the data is copied, producer and consumer never touch the same memory, and no [[lock]] is needed. \
                First in, first out: the consumer sees the items in the order they were made.\n\n\
                A consumer that finds the queue empty simply sleeps until an item arrives. The interesting question is the \
                opposite one: what should a producer do when the queue is full? There are three honest answers, and choosing \
                one is a design decision.\n\n\
                Throw the new item away: the put fails at once and the producer carries on. Old data is kept, the newest is \
                lost. This is the only choice inside an interrupt handler, which must never wait.\n\n\
                Wait: the producer thread sleeps until there is room. Nothing is lost, but the producer is slowed down to \
                the consumer's pace - fine for a log, fatal for something that must keep sampling on time.\n\n\
                Empty the queue first: everything old is thrown out to make room for the newest. Right when only fresh data \
                matters, such as a position or a temperature.\n\n\
                A bigger queue helps with bursts, and only with bursts. If the consumer is too slow on average, any queue \
                fills up eventually.",
            formulas: &[(
                "queue slots a burst needs = (items - 1) - items the consumer finishes while the burst arrives",
                "5 items 1 ms apart, 4.5 ms each to handle: the first goes straight to the consumer, and it finishes none before the last one arrives. So 4 slots are needed, and the queue has 3.",
            )],
            try_this: &[
                Experiment { action: "Raise the capacity to 4.", watch: "no more red bars: the queue now swallows the whole burst, exactly as the formula says.", preset: Some("bigger") },
                Experiment {
                    action: "Back to capacity 3, bursts of 7, and let the producer wait for space.",
                    watch: "no item lost, but a long yellow bar: the producer is stuck in k_msgq_put and is slowed down to the consumer's pace.",
                    preset: Some("wait"),
                },
                Experiment {
                    action: "Choose \"empties the queue first\" and look into the queue at 9.2 ms.",
                    watch: "the queue holding only item 5: items 2, 3 and 4 were thrown away for it, and the consumer goes from item 1 straight to item 5.",
                    preset: Some("purge"),
                },
                Experiment { action: "Drag the inspection time slowly through the first burst.", watch: "items entering the boxes on the right and leaving on the left, in order.", preset: None },
            ],
            leaves_out: "One producer and one consumer; several of each may share a queue, and waiting threads are served by \
                priority, then by how long they have waited. Putting and getting can also give up after a time limit. When \
                the consumer is already waiting, an item is handed to it directly without touching the queue's memory. For \
                large data, real code queues a pointer to the data instead of copying it. Zephyr has more ways to pass data \
                (FIFOs, pipes, mailboxes); the message queue is the usual starting point.",
            refs: &[
                "Zephyr documentation, Kernel Services > Data Passing > Message Queues: ring buffer, item size, maximum quantity; behaviour when full or empty; \"A data item can be sent to a message queue by a thread or an ISR\"; producer example with k_msgq_purge; K_MSGQ_DEFINE.",
            ],
        },
    }
}
