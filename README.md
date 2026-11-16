# Embedded Lab

An interactive tool for understanding embedded systems: a library of live, editable
simulations with a beginner-level explanation next to each one. It climbs from the physics
upward — the radio wave, then the bits of a packet, then the network.

## Run it

```sh
cargo run -p embedded-lab
```

The first build takes several minutes; later ones take seconds. Under WSL the app selects
the Vulkan graphics backend by itself, because the default OpenGL route fails to open a
window there. To choose another backend, set `WGPU_BACKEND` (`vulkan` or `gl`).

## What is in it

Three learning paths, 15 demos:

| Path | Demos |
|---|---|
| From radio wave to mesh message | What is a radio wave · Waves in space · Spectrum · Modulation · Anatomy of a BLE advertising packet · Advertising and scanning in time · A BLE connection: hopping · Bluetooth Mesh: how a message spreads · Inside a mesh packet |
| Radar: seeing with radio | (the first three above) · Doppler radar · FMCW radar · Range-Doppler map |
| Inside the microcontroller | A pin and a bouncing button · Timers and PWM · Interrupts and priorities |

"All topics" in the library panel shows the same demos as a tree, together with the planned
ones (greyed out).

Every demo can be edited: sliders and inputs, packet bytes and single bits, and things you
drag in a scene (antennas, walls, radar targets, mesh nodes). The explanation panel has
"Show me" buttons that set up each suggested experiment.

## How accurate is it

- **Protocols are bit-exact.** The Bluetooth LE CRC, whitening and packet layout are tested
  against the sample packets in the Bluetooth Core Specification (Vol 6 Part C). Bluetooth
  Mesh key derivation, encryption and obfuscation are tested against the sample data in the
  Mesh Protocol specification (section 8). AES, CMAC and CCM are tested against their
  standards' vectors.
- **Physics is textbook level.** Every demo has a "What this leaves out" section.
- `crates/sim-core/src/crypto.rs` is teaching code. Do not use it in a product.

## Test it

```sh
cargo test --workspace
```

This runs the simulation tests and also opens every demo headless, applies every experiment
preset, and writes a screenshot of each to `target/shots/`.

## Layout

| Crate | Contents |
|---|---|
| `crates/sim-core` | The simulations: signal processing, modem, wave field, radar, BLE, mesh, microcontroller models. No GUI code, no threads, no clocks, no file access. |
| `crates/lab-ui` | The framework: the `Demo` trait, explanation and glossary model, shared widgets (unit sliders, plots, heat map, bit strip, draggable scene). |
| `crates/demos` | One module per demo, plus the registry of demos, learning paths and planned topics. |
| `crates/app` | The window: library panel, demo area, explanation panel. |

## Add a demo

`AGENTS.md` has the full instructions for coding agents, including what to do when asked
"how does X work?". In short:

1. Put the simulation logic in `sim-core`, with tests.
2. Copy a small demo such as `crates/demos/src/wireless/spectrum.rs` and change it. A demo is
   a struct implementing `Demo` plus an `entry()` function that returns its title, tree
   position and explanation.
3. List it in `registry()` in `crates/demos/src/lib.rs` and add it to a learning path.
4. Mark glossary words in the explanation as `[[term]]` and add any new ones to
   `crates/lab-ui/src/glossary.rs`.
5. Run `cargo test --workspace` and look at the new screenshot in `target/shots/`.

The content test fails if a demo has an empty explanation section, fewer than two
experiments, or a `[[term]]` that the glossary does not define.
