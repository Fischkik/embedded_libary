# Instructions for coding agents

Embedded Lab is a learning tool: a library of live, editable simulations ("demos") that
explain embedded systems to one person, each with a beginner-level explanation beside it.
`README.md` describes the crates and how to run and test.

## Status

On 2026-11-23 the open points from `HANDOFF.md` section 6 were worked through: slider rows,
clippy (now clean), light mode, small windows. What is left is listed there.

The job "build every planned demo" (2026-11-16 to 2026-11-17) is **finished**: 71 demos in 7
learning paths, `planned()` is empty, `cargo test --workspace` passes (141 tests). `HANDOFF.md`
keeps the record of that job: a note and the sources for each demo, and in section 6 what
was not verified. Nothing of it is committed yet; the user commits.

To register new demos, `tools/register_demo.py` still saves work. It adds the module line,
the registry entry and the learning-path step, removes the title from `planned()`, and
inserts glossary entries in alphabetical order:
`python3 tools/register_demo.py <field_module> <path_id> "<demo-id>:<module>:<title>" <<< '{"term": "One sentence."}'`
Afterwards put the learning path into a sensible teaching order by hand, and add the demo to
the table in `README.md`.

## When the user asks "how does X work?"

If X is an embedded-systems topic (radio, a protocol, a bus, something inside a
microcontroller, a sensor, a firmware technique), the user wants more than a chat answer.
They want to be able to open the app and play with it. Do this:

1. **Look for an existing demo.** Search the titles and blurbs in `registry()` in
   `crates/demos/src/lib.rs`, and the demo files under `crates/demos/src/`.
2. **If a demo covers it:** answer the question in a few plain sentences and tell the user
   which demo to open (its title, and which learning path it is in). Do not build anything.
   If the demo covers the topic only in passing, say so and offer to build a dedicated one.
3. **If no demo covers it:** build one, following the rest of this file. Then answer the
   question briefly and tell the user where the new demo is and what to try first.

Just answer, without building, when X is about this codebase itself ("how does the registry
work?") or is not an embedded topic.

Build **one focused demo per request**. If X is large (for example "USB"), pick the single
idea a newcomer needs first, build that, and add the remaining ideas to the `planned()` list
in `crates/demos/src/lib.rs` so they show up greyed out in the library. Say which part you
built and which parts you listed as planned.

Work inline. Do not fan out to many subagents for one demo unless the user asks for that.

## Who the demos are for

The user is new to embedded systems. Every demo and explanation must:

- start from the basics and define each technical word the first time it is used;
- stay technically correct — simplify by leaving things out, never by saying something false,
  and list what was left out;
- be in English.

## Rules every demo follows

1. One idea per demo.
2. It opens in a state that already shows something happening. No empty screens.
3. Everything the user can change has a hover text in plain language.
4. Numbers carry units. Use the SI helpers so values read as "2.4 GHz", not "2400000000".
5. Show the key formula with the current numbers filled in (`widgets::formula`).
6. At least two "try this" experiments, each saying what to do and what to watch for, with a
   preset where a "Show me" button makes sense.
7. A "what this leaves out" section that is honest about the simplifications.
8. Editing means: sliders and inputs, direct editing of packet bytes and single bits, and
   dragging things in a scene. Offer whichever of these fits the topic.

## Accuracy

- **Protocols are bit-exact.** Implement them from the specification and test them against
  published sample data or test vectors. Fetch the vectors from the source. Never reconstruct
  hex test vectors or spec constants from memory.
- **Physics is textbook level.** Check each formula against a datasheet or standard reference,
  and put a numeric test on it where possible (see the HB100 Doppler test in
  `crates/sim-core/src/radar.rs`).
- Put the source of the facts in the explanation's `refs`.
- Microcontroller topics are generic ARM Cortex-M concept models: no vendor register names,
  no instruction-level emulation.
- Inside an `egui::Grid`, text wraps letter by letter when a column gets squeezed. Add such
  labels as `ui.add(egui::Label::new(text).extend())`.
- Zephyr topics (the "Zephyr" section) are the exception: they describe that one RTOS and use
  its real names (`k_sleep`, `CONFIG_TIMESLICING`). Take every rule from the Zephyr
  documentation and quote it next to the code that implements it (see
  `crates/sim-core/src/zephyr.rs`). Where the documentation is silent, read the kernel source,
  and say in `leaves_out` that the behaviour comes from the source and may change.

## How to build a demo

### 1. Simulation logic goes in `crates/sim-core`

Add to an existing module or create a new one and list it in `src/lib.rs`. Rules for this
crate, which also keep a later browser build possible:

- no GUI dependencies, no threads, no `std::time`, no file or network access;
- deterministic: randomness comes from `rng::Rng` with a seed;
- tests in the same file, including the known-answer vectors.

Run `cargo test -p sim-core` until it passes before touching the UI. It takes seconds.

### 2. The demo goes in `crates/demos/src/<field>/<name>.rs`

A demo is a struct that implements `Demo`, plus an `entry()` function. Copy
`crates/demos/src/wireless/spectrum.rs` (small) or `mcu/pwm.rs` as a starting point. Minimal
shape:

```rust
use crate::prelude::*;

pub struct MyDemo {
    f_hz: f64,
}

impl Default for MyDemo {
    fn default() -> Self {
        Self { f_hz: 1000.0 }
    }
}

impl Demo for MyDemo {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "Settings", |ui| {
            ui.add(widgets::si_slider(&mut self.f_hz, 10.0..=1e6, "frequency", "Hz").logarithmic(true))
                .on_hover_text("How many times per second the signal repeats.");
        });
        let points: Vec<[f64; 2]> = (0..400).map(|i| i as f64 * 1e-5).map(|t| [t * 1e3, (std::f64::consts::TAU * self.f_hz * t).sin()]).collect();
        plots::plot("signal", 220.0).x_axis_label("time [ms]").y_axis_label("value").show(ui, |p| {
            p.line(plots::line("signal", points, pal.trace[0]));
        });
        widgets::formula(ui, format!("period = 1 / frequency = {}", si(1.0 / self.f_hz, "s")));
    }

    fn apply_preset(&mut self, preset: &str) {
        if preset == "fast" {
            self.f_hz = 100e3;
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "my-demo",
        title: "A short title",
        blurb: "One sentence for the library list.",
        path: &["Field", "Sub-field"],
        make: || Box::new(MyDemo::default()),
        explain: Explanation {
            what: "What is on screen. Mark glossary words like [[frequency]].\n\nBlank line = new paragraph.",
            why: "Why it behaves this way, from the basics.",
            formulas: &[("period = 1 / frequency", "The same thing in words, with one worked number.")],
            try_this: &[
                Experiment { action: "What to do.", watch: "what to look for.", preset: Some("fast") },
                Experiment { action: "A second thing to do.", watch: "what to look for.", preset: None },
            ],
            leaves_out: "What the simulation simplifies or ignores.",
            refs: &["Specification and section, datasheet, or standard formula."],
        },
    }
}
```

What the framework gives you (`crates/lab-ui/src/`):

| Need | Use |
|---|---|
| Slider with SI units | `widgets::si_slider(&mut v, range, "label", "Hz")` |
| Titled group, live formula, OK/FAIL line | `widgets::group`, `widgets::formula`, `widgets::status` |
| Play/pause/speed for animation | `widgets::Clock` — call `tick(ui, cx.dt)`, then `controls(ui)` |
| Timeline with one labelled row per task/thread/signal | `plots::timeline(id, names, x_label, x_max)`, bars with `Polygon::new(name, plots::bar(..))`, colours explained with `widgets::key(ui, &[(colour, "text")])` |
| Any other graph where a legend box would cover data | `plots::plot_bare(id, height)` plus `widgets::key` below it |
| Picture made of boxes (memory, flash, block diagram) | paint it: `ui.allocate_exact_size`, then `ui.painter().rect_filled / text / arrow` - see `mcu/memory.rs`, `mcu/clocks.rs` |
| Graphs | `plots::plot(id, height)`, `plots::line`, `plots::series`, `plots::digital` (square waveforms), `plots::bar` (Gantt bars) |
| 2D picture from numbers | `heat::Heat` with `heat::diverging` (fields) or `heat::ramp` (levels) |
| Packet bits and bytes | `bits::bit_strip` (clickable), `bits::field_legend`, `bits::hex_edit`, `bits::hex_row` |
| Draggable things | `scene::scene(ui, id, world_rect, max_height, \|s\| ...)` and `s.handle(key, &mut pos, radius)` |
| Colours | `theme::palette(ui)` — never hard-code colours; both light and dark mode must work |
| Open another demo with data | set `cx.open = Some(("demo-id", bytes))`; the target implements `Demo::receive` |

### 3. Register it in `crates/demos/src/lib.rs`

- add the `pub mod` line;
- add `entry()` to the `demos` list in `registry()`;
- add its id to a learning path, at the point where its prerequisites have been covered, or
  start a new `LearningPath` for a new field;
- if the topic was in `planned()`, remove it from there.

### 4. Glossary

Every `[[term]]` must have an entry in `crates/lab-ui/src/glossary.rs`. Entries are one plain
sentence and the list must stay in alphabetical order (a test checks this). Lookup ignores
case and a trailing "s".

### 5. Verify

```sh
cargo test --workspace
```

This must pass. It runs the simulation tests, checks the content (a demo fails the check if an
explanation section is empty, it has fewer than two experiments, or a `[[term]]` is missing
from the glossary), and opens every demo headless, applies every preset, and writes
screenshots to `target/shots/<id>.png` and `target/shots/<id>--last-preset.png`.

**Open the screenshot of your demo and look at it.** Tests passing does not mean it looks
right: the square-waveform bug described below passed every test. Check that graphs show what
the explanation says they show, that numbers on screen match the formula, and that nothing is
cut off.

A demo is done when: `sim-core` tests cover its logic, `cargo test --workspace` passes, you
have looked at its screenshot, and you can state where its facts come from. Tell the user
which of these you did, and anything you could not verify.

## Things that will trip you up

The GUI crates are egui/eframe 0.37 and egui_plot 0.38. Their APIs differ from older versions
you may remember. When unsure, read the source under
`~/.cargo/registry/src/*/egui-0.37.0/` rather than guessing.

- `eframe::App` has `fn ui(&mut self, ui: &mut egui::Ui, frame)`. There is no `update`.
- `SidePanel` and `TopBottomPanel` are gone: use `egui::Panel::left/right/top/bottom`, and
  panels take a `&mut Ui`.
- `Painter::rect_stroke` takes a fourth argument, `StrokeKind`. `Rounding` is now
  `CornerRadius`. `Frame::none()` is `Frame::NONE`.
- `Id::new` and `ui.id()` are deprecated: use `ui.make_persistent_id(..)`, `Id::unique`, or
  `.id_salt(..)` on widgets.
- egui_plot items take a name first: `Line::new(name, points)`, `VLine::new(name, x)`.
- **Use `plots::line`, not `Line::new` directly.** egui_plot's default "band" mode draws
  square waveforms made of few points as filled wedges. `plots::line` switches it off.
- Close a context menu with `ui.close()`.
- **No emoji or unusual symbols in UI text.** The test fonts lack them and the test panics
  with "No glyph". Plain ASCII plus `µ` and common Latin-1 symbols are safe.
- Give every slider a `.text("label")`, and link a `TextEdit` to its label with
  `.labelled_by(label.id)`, so the accessibility check passes.
- **A demo must fit the width of its area.** The tests measure it and fail when a demo is
  wider than its area at the normal size (1500 x 950, both side panels open) or in the
  smallest window (900 x 600) with the explanation closed. The demo area can scroll
  sideways, but only as a fallback for small windows. Break long monospace lines (hex
  bytes: 16 per line). Use `ui.horizontal_wrapped` for rows of controls so
  they wrap in narrow windows instead of being cut off. **A slider does not wrap by itself**:
  inside such a row, add it as `widgets::wrap(ui, "key", |ui| ui.add(slider).on_hover_text(..));`.
  Otherwise it runs off the right edge and pulls every graph below it out of view with it.
  In the screenshot, check that the right edge of each graph and its legend is visible.
- Give plots with a long y-axis label a height of at least about 180.
- Object ids in a scene must come from a stable key, not the list index, or deleting an item
  moves the drag to its neighbour.
- In UI tests an animated demo never "settles": use `harness.run_steps(n)`, not `run()`.
- Under WSL the window needs the Vulkan backend; `main.rs` selects it automatically.
- The first full build takes over ten minutes on this machine. Later builds take seconds.
  Changing features of the GUI crates triggers the long build again, so avoid it.

- **Compute every number that appears in explanation text**; do not estimate. When an
  experiment promises an outcome ("the weak target is lost", "4 dB higher"), add a test in
  the demo file that applies the preset and checks it (see `radar/cfar.rs`). Random data can
  make a claim true for one seed and false for another.
- **Plot invented example values and check they are plausible.** A first set of sleep-state
  numbers once made the deepest sleep state cost more than the lighter one.
- **A preset must stay inside the slider's range**, or the slider silently clamps it and the
  experiment shows something else.
- **Painter text on a filled box**: use `ui.visuals().strong_text_color()`; `pal.text` is too
  dim there.
- **`painter.arrow`** draws a head a quarter of the arrow's length. For long arrows draw a
  line plus a short arrow at its end (see `firmware/state_machine.rs`).
- **Monospace boxes** (`widgets::formula`) wrap at about 95 characters in the screenshot
  width. Keep formula lines shorter.
- **Only the top 950 pixels are in the screenshot.** `HANDOFF.md` section 5 has a one-off
  test that takes a taller one.
- **Long specification pages** (Bluetooth Core, Mesh Protocol): WebFetch reads only about
  the first 450 000 characters. Download the page with curl into the scratchpad and strip
  the tags with Python to reach later sections.

## Other conventions

- Do not add dependencies without a reason the user would accept. Small things (a PRNG, AES)
  are written in-house on purpose, to stay deterministic and browser-compatible.
- `crates/sim-core/src/crypto.rs` is teaching code, checked against test vectors but not
  hardened. Do not present it as usable in a product.
- Commit only when the user asks.
