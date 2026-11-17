# Handoff: finish the planned demos

## FINISHED (2026-11-17)

Every planned demo is built: **71 demos**, `planned()` is empty, `cargo test --workspace`
**140 passed, 0 failed**. Nothing is committed; the user commits. This file is kept as the
record of the job: the "Done since" list below has a note and the sources for each demo, and
section 6 lists what was not verified. Section 7 lists optional follow-up work. `AGENTS.md`
holds the rules for new demos.


The text below was written while the job was in progress and is kept unchanged as a record.

## 1. The job

The user asked (2026-11-16): **build every topic that is still greyed out in the library**,
and keep the documentation current so that the work survives a change of agent.

- The user is new to embedded systems. Explanations start from the basics, define every
  technical word, and stay correct: simplify by leaving out, never by saying something false.
- The user works under a usage limit. Work inline, in batches, without subagents.
- **Do not commit.** The user commits. Everything since the first commit (`35466a1`) is
  uncommitted on purpose.

## 2. State on 2026-11-17 (checked, not remembered)

- `cargo test --workspace`: **140 passed, 0 failed**.
- **71 demos** registered, **7 learning paths**. 0 topics remain in `planned()`.
- Finished sections: Inside the microcontroller (12), Zephyr (13), Firmware craft (5),
  Wired buses (7). Signals and analog has 3 of 7.
- Every demo built in this job had its screenshot looked at (`target/shots/<id>.png`).
- 27 changed or new files are uncommitted. `git status` shows them; nothing is half-written.

The single source of truth for what is left is the `planned()` list in
`crates/demos/src/lib.rs`. A demo leaves that list in the same step that registers it.

### Done since this file was first written

Skip these in section 3: their briefs are kept only for reference.

- 2026-11-17: `signals-pll` - PLL: making frequencies. Simulation: signals::Pll::hop.
- 2026-11-17: `signals-opamp` - Op-amp circuits. Simulation: signals::NonInverting.
- 2026-11-17: `signals-pir` - PIR sensor front end. Simulation: signals::PirScene. Pass band and gain are typical published values (TI SNAA301, ST, Renesas TB512); signal sizes are illustrative and say so.
- 2026-11-17: `signals-buck` - Switching power supplies. Simulation: signals::Buck (result cached in the demo; it runs up to millions of steps). Section 5 'Signals and analog' is now COMPLETE (7 demos). Next: Wireless, Radio basics.
- 2026-11-17: `antenna-pattern` - Antennas and radiation patterns. Simulation: new module sim-core/src/radio.rs (Antenna, directivity by numeric integration, checked independently in Python).
- 2026-11-17: `link-budget` - Link budget: power, distance, sensitivity. Simulation: radio::Link, radio::fspl_db. Transmit power range and -70 dBm sensitivity fetched from Core 5.4 Vol 6 Part A (the page needs WebFetch with offset 400000).
- 2026-11-17: `band-2g4` - The 2.4 GHz band and its neighbours. Simulation: radio::free_ble_data_channels. 'Radio basics' is COMPLETE; wave-to-mesh path reordered. Next: Bluetooth LE (GATT, pairing, power).
- 2026-11-17: `ble-gatt` - GATT: services and characteristics. Simulation: new module sim-core/src/gatt.rs. ATT opcodes, error codes, property bits and UUIDs from BlueZ att-types.h and lib/bluetooth/uuid.h (raw.githubusercontent.com/bluez/bluez/master/...).
- 2026-11-17: `ble-pairing` - Pairing and encryption. Simulation: new module sim-core/src/smp.rs; c1 and s1 match the spec's sample data (Core 5.4 Vol 3 Part H 2.2.3/2.2.4, fetched via WebFetch offset 400000 of host/security-manager-specification.html). Secure Connections explained in words only, as planned.
- 2026-11-17: `ble-power` - Power: how long does the battery last?. Simulation: ble::AdvPower. Radio currents: nRF52832 electrical specification (7.1 mA TX, 6.5 mA RX, 1.6 µA idle+RTC); radio start-up 140 µs is an example value. 'Bluetooth LE' is COMPLETE. Next: Bluetooth Mesh (5 topics, protocol facts from Mesh Protocol 1.1).
- 2026-11-17: `mesh-addressing` - Addresses, publish and subscribe. Simulation: mesh::accepts; mesh::address_kind corrected to the spec's Table 3.7 (0xFFF9-0xFFFB added). Address ranges fetched from Mesh Protocol 1.1 (WebFetch offset 300000).
- 2026-11-17: `mesh-segmentation` - Segmented messages. Simulation: mesh::segment, segment_ack, sar_rounds; bit-exact against spec sample message #6 (segmented, from BlueZ unit/test-mesh-crypto.c, struct s8_3_6).
- 2026-11-17: `mesh-friend` - Friends and low power nodes. Simulation: mesh::Friendship. Ranges from Mesh Protocol 1.1 Tables 3.37/3.38/3.41 (WebFetch offset 400000).
- 2026-11-17: `mesh-provisioning` - Provisioning a new device. Simulation: mesh::prov_side, session_keys, mitm_accepted. Formulas from Zephyr subsys/bluetooth/mesh/crypto.c. NOT checked against the spec's provisioning sample data (section 8.7 not reachable: the spec page's text is cut before section 8 by WebFetch, and BlueZ's test file has no provisioning data). ECDH secret is a labelled stand-in. **Later the same day: now checked bit for bit against section 8.7 (test `provisioning_matches_spec_sample_data_8_7`); the full spec text was reached by downloading the page with curl.**
- 2026-11-17: `mesh-replay` - Replay protection and the IV index. Simulation: mesh::ReplayList, iviseq, rx_iv_index, tx_iv_index (tests use the spec's own IV Index examples from 3.11.5). Rules quoted from Mesh Protocol 1.1 3.9.3, 3.9.8, 3.11.5: WebFetch cannot reach them, download the page with curl and strip the tags (text is 1.6 M characters). 'Bluetooth Mesh' is COMPLETE. Next: Radar.
- 2026-11-17: `radar-angle` - Angle of arrival with two antennas. Simulation: radar::aoa_phase, aoa_angle, wrap_pi, aoa_estimates (tests: 30 deg at lambda/2 -> 90 deg; ghost -20.93 deg at d = lambda).
- 2026-11-17: `radar-presence` - Presence detection: breathing and micro-motion. Simulation: radar::Presence (phase 4 pi x / lambda, spectrum of the phase). Chest 1-12 mm, heartbeat 0.1-0.5 mm: TI mmWave Vital Signs lab guide. Demo tests check the experiment numbers.
- 2026-11-17: `radar-cfar` - Detection thresholds (CFAR). Simulation: radar::cfar_alpha, ca_cfar, CfarScene. alpha = N (Pfa^(-1/N) - 1) derived for exponential noise and checked by Monte Carlo (200 000 cells); demo test checks every experiment's claim on the exact noise shown.
- 2026-11-17: `radar-vs-pir` - Radar versus PIR sensors. Simulation: radar::doppler_motion on signals::PirScene walks, plus radar::Presence. Glass blocks PIR (Panasonic PIR FAQ); radar through non-metallic materials (Infineon Sense2GoL brief); glass loss and thresholds are example values. Section 7 Radar is COMPLETE: planned() is empty.

## 3. What is left: 0 demos

Order: **Signals and analog is finished.** Next **Wireless** (Radio basics, Bluetooth LE,
Bluetooth Mesh), then **Radar**. Within a group, in the order listed here.

Every brief gives: the one idea, what to simulate, what to draw, experiments, and where the
facts come from. Numbers marked **(verify)** are from my memory. The project rule is strict:
fetch them from the named source before they go into code or text.

### 3a. Signals and analog — 4 left

Simulation in `crates/sim-core/src/signals.rs`. Demos in `crates/demos/src/signals/`.
Register with `python3 tools/register_demo.py signals signals "<id>:<module>:<planned title>"`.

| Planned title (exact) | Suggested id / module |
|---|---|
| PLL: making frequencies | `signals-pll` / `pll` |
| Op-amp circuits | `signals-opamp` / `opamp` |
| PIR sensor front end | `signals-pir` / `pir` |
| Switching power supplies | `signals-buck` / `buck` |

**PLL: making frequencies.** Idea: a feedback loop steers an oscillator until its output,
divided by N, runs in step with a reference; then `f_out = N x f_ref`. `mcu-clocks` already
uses a PLL as a black box; this demo opens the box.
- Simulate in time steps: phase detector `e = phase_ref - phase_vco / N`; loop filter
  `control = Kp e + Ki sum(e)`; oscillator `f_vco = f_free + Kvco x control`, limited to a range.
- Draw: oscillator frequency over time, settling onto the line `N x f_ref`; a lock indicator.
- Experiments: change N and watch the output hop to the new multiple (this is how a radio
  changes channel); too much loop gain rings, too little is slow; N outside the oscillator's
  range never locks.
- Tests: settles within a tolerance of `N x f_ref`; does not lock outside the range.
- Sources: Analog Devices MT-086 "Fundamentals of Phase Locked Loops"; Gardner, "Phaselock
  Techniques". Leave out: fractional-N, phase noise, charge pumps.

**Op-amp circuits.** Idea: an amplifier with enormous gain becomes precise through negative
feedback; two resistors set the gain. Use the non-inverting amplifier: `gain = 1 + R2 / R1`.
- Simulate: `output = clamp(gain x input, 0, supply)`; bandwidth `= GBW / gain`.
- Draw: input and output sine; the circuit as a painted triangle with two resistors.
- Experiments: gain too high clips at the supply; high gain costs bandwidth; pick the gain
  that makes a 10 mV sensor signal fill the ADC's range (link to `mcu-adc`).
- Tests: 10 k / 1 k gives 11; clipping; 1 MHz GBW at gain 11 gives 90.9 kHz.
- Sources: Horowitz and Hill, "The Art of Electronics", ch. 4; TI, "Op Amps for Everyone".

**PIR sensor front end.** Idea: a pyroelectric sensor answers only to *changes* of infrared.
Two elements wired against each other see a warm body cross from one zone to the next as a
swing up then down, while whole-room changes cancel. A band-pass amplifier and a window
comparator turn that into "motion".
- Simulate: a person crossing lens zones at speed v gives a signal of frequency
  `v / zone pitch`; pass it through a high-pass and the existing `signals::lowpass`; compare
  with two thresholds.
- Experiments: walking straight towards the sensor crosses no zones and is barely seen
  (the classic PIR weakness, and the hook for "Radar versus PIR" later); too slow or too fast
  falls outside the band; a draught of warm air on both elements cancels.
- Tests: signal frequency; radial motion below threshold; common change cancels.
- **Facts must be fetched**: pass band, gain, zone geometry. Use a sensor datasheet and
  application note (Murata IRA-S210ST01, Excelitas, or Panasonic EKMB series). I have no
  verified numbers for this one.

**Switching power supplies.** Idea: a buck converter makes a lower voltage by switching
fast and smoothing with an inductor, wasting little; a linear regulator burns the difference
as heat. `V_out = duty x V_in`.
- Simulate an ideal buck in time steps (switch, diode, L, C, load R). Inductor ripple
  `dI = (V_in - V_out) x duty / (L x f)`. Linear regulator efficiency `= V_out / V_in`.
- Draw: switch node, inductor current, output voltage; a bar comparing heat lost.
- Experiments: change the duty; raise the frequency and the ripple shrinks; a small
  inductor gives large ripple; 12 V to 3.3 V linear versus switching.
- Tests: steady output equals `duty x V_in`; ripple formula.
- Sources: Erickson and Maksimovic, "Fundamentals of Power Electronics", ch. 2; TI
  application report SLVA477. Links to `mcu-pwm` (duty cycle).

After the four: order the `signals` path by hand, for example sampling, noise, filter,
op-amp, pir, pll, buck.

### 3b. Wireless — 11 left

Field module `wireless`, learning path id `wave-to-mesh`. Existing simulation code to build
on: `ble.rs` (packets, CRC, whitening, channel hopping, advertising and scanning),
`mesh.rs` (key derivation k1-k4, network and access layer, flooding), `crypto.rs` (AES-128,
CMAC, CCM), `modem.rs`, `field.rs` (2D wave field), `dsp.rs` (FFT).
Copy the `path: &[...]` value from a neighbouring wireless demo.

Specification sources (public, no login; fetched successfully on 2026-11-13):
- Core 5.4, LE controller volume:
  `https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-54/out/en/low-energy-controller/`
  with `physical-layer-specification.html`, `link-layer-specification.html`, `sample-data.html`.
  The host volume (ATT, GATT, Security Manager) is under the same `Core-54/out/en/` tree.
- Mesh Protocol 1.1 (one large page, sample data in section 8):
  `https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/MshPRT_v1.1/out/en/index-en.html`
- Mesh Model 1.1: `.../HTML/MMDL_v1.1/out/en/index-en.html`
- Assigned Numbers: `.../HTML/Assigned_Numbers/out/en/index-en.html`
- Cross-checks: BlueZ `unit/test-mesh-crypto.c`, Scapy `test/scapy/layers/bluetooth4LE.uts`.
- Some spec figures are images and cannot be read by a fetch; find the same table elsewhere.

**Radio basics**

1. *Antennas and radiation patterns* (`antenna-pattern`). Idea: an antenna does not radiate
   equally in all directions. Half-wave dipole: `E(theta) ~ cos(pi/2 cos theta) / sin theta`,
   gain 2.15 dBi. Two elements: `AF = 2 cos((k d cos theta + phase) / 2)`. Draw a polar
   pattern; let the user turn the antenna and move a receiver. Tests: null along the dipole's
   axis, maximum broadside, directivity 1.64 by numeric integration. Source: Balanis,
   "Antenna Theory", ch. 4 and 6.
2. *Link budget: power, distance, sensitivity* (`link-budget`). Idea: received power =
   transmit power + antenna gains - path loss; compare with the receiver's sensitivity.
   `FSPL [dB] = 20 log10(d) + 20 log10(f) - 147.55` (d in m, f in Hz): 40.2 dB at 1 m and
   2.44 GHz, 60.2 dB at 10 m. Draw received power against distance on a log scale with the
   sensitivity line. BLE's required sensitivity is -70 dBm **(verify**, Core Vol 6 Part A);
   typical chips reach about -95 dBm **(verify** in a datasheet). `units::db` exists.
3. *The 2.4 GHz band and its neighbours* (`band-2g4`). Idea: BLE's 40 channels share the band
   with Wi-Fi and Zigbee; the three advertising channels sit at the edges of and between
   Wi-Fi channels 1, 6 and 11; a connection avoids busy channels. `ble::channel_mhz` exists.
   Wi-Fi centre `= 2407 + 5 x channel MHz`; 802.15.4 channel k (11-26) at
   `2405 + 5 (k - 11) MHz` **(verify both)**. Draw the band as a strip; toggle Wi-Fi channels
   and show which BLE channels remain. Hand the channel map to `ble-connection`
   (`Csa1::new(hop, used)`).

**Bluetooth LE**

4. *GATT: services and characteristics* (`ble-gatt`). Idea: a device's data is a table of
   attributes (handle, type, value); services group characteristics; a client reads and writes
   by handle and can ask to be notified. Bit-exact ATT PDUs (opcodes for read, write,
   notification, error) from Core Vol 3 Part F; UUIDs from Assigned Numbers. Show the table,
   let the user click a row, show request and response bytes with `bits::hex_row`. No official
   sample data that I know of: cross-check against byte arrays in BlueZ `unit/test-gatt.c`.
5. *Pairing and encryption* (`ble-pairing`). **The hardest one left.** LE Secure Connections
   needs elliptic-curve arithmetic (P-256), which this project does not have and should not
   grow casually. Recommended cut: build LE *legacy* pairing, which needs only AES (functions
   c1 and s1, with sample data in Core Vol 3 Part H), and show the lesson that matters: an
   eavesdropper who recorded the pairing can try all 1 000 000 passkeys and recover the key.
   Explain Secure Connections in words, with a toy example in small numbers clearly labelled as
   a toy. If link-layer encryption does not fit, add it to `planned()` and say so.
6. *Power: how long does the battery last?* (`ble-power`). Idea: average current is set by
   how often the radio wakes. Reuse `mcu::SleepCycle` and `ble::AirPacket::duration_us`.
   One advertising event = three packets plus listening windows. Radio currents must come from
   a datasheet **(verify**, for example the nRF52832 product specification). Plot battery life
   against advertising interval, like `mcu/low_power.rs`.

**Bluetooth Mesh** (all facts from Mesh Protocol 1.1; test against its section 8)

7. *Addresses, publish and subscribe* (`mesh-addressing`). Unicast, group and virtual
   addresses (`mesh::address_kind` exists; check its ranges against section 3.4.2). A switch
   publishes to a group, lamps subscribed to it react. Use a scene like `mesh_flooding.rs`.
8. *Segmented messages* (`mesh-segmentation`). An access message longer than 11 bytes is cut
   into 12-byte segments with SeqZero, SegO, SegN; the receiver answers with a block
   acknowledgement bitmap and lost segments are sent again **(verify sizes and the header
   layout**, section 3.5.2). Test against the segmented message in section 8.
9. *Friends and low power nodes* (`mesh-friend`). A battery node sleeps; a friend node stores
   its messages; the node polls. Timeline of radio-on time, average current, and delivery
   delay. Parameter names and ranges from section 3.6.6.
10. *Provisioning a new device* (`mesh-provisioning`). Step-through like `mcu/boot.rs`.
    Do **not** implement P-256: the spec's provisioning sample data (section 8) gives the
    shared secret, so the key derivations and the encrypted provisioning data can still be
    tested bit-exactly. Say in `leaves_out` that the key exchange itself is not computed.
11. *Replay protection and the IV index* (`mesh-replay`). An attacker replays a recorded
    message; nodes remember the highest sequence number per sender and refuse anything older.
    24-bit sequence numbers run out (16 777 216 messages; 194 days at one per second), which
    is what the IV index is for. Use real encrypted bytes from `mesh::encode_network`.
    Experiment: a node that forgets its sequence number in a reboot is ignored by everyone.

### 3c. Radar — 4 left

Field module `radar`, learning path id `radar`, simulation in `radar.rs` and `dsp.rs`.

1. *Angle of arrival with two antennas* (`radar-angle`). `phase difference = 2 pi d sin(theta) / lambda`.
   Unambiguous only for spacing up to half a wavelength. Draggable target, two phasor arrows.
   Tests: `d = lambda/2`, 30 degrees gives 90 degrees of phase; ambiguity at `d = lambda`.
2. *Presence detection: breathing and micro-motion* (`radar-presence`). A still person's chest
   moves millimetres; the echo's phase turns by `4 pi x displacement / lambda` (4 mm at 24 GHz
   is about 4 rad). Show phase over time and its spectrum with a peak at the breathing rate.
3. *Detection thresholds (CFAR)* (`radar-cfar`). A fixed threshold fails when the noise floor
   moves. Cell-averaging CFAR: threshold `= alpha x mean of N neighbouring cells`,
   `alpha = N (Pfa^(-1/N) - 1)`; N = 16 and Pfa = 0.0001 give 12.45. Test the measured false
   alarm rate on pure noise. Source: Richards, "Fundamentals of Radar Signal Processing", ch. 6.
4. *Radar versus PIR sensors* (`radar-vs-pir`). One scene, both sensors, a table of verdicts.
   Radar sees motion towards it best; PIR sees motion across it best; radar sees through thin
   walls, PIR does not; a still person is visible only to radar. Needs the PIR model from 3a.

## 4. The recipe for one batch (three demos)

```sh
# 1. simulation + tests, in crates/sim-core/src/<module>.rs
cargo test -p sim-core <module>

# 2. one file per demo in crates/demos/src/<field>/<name>.rs, then register all three:
python3 tools/register_demo.py <field_module> <path_id> \
  "<demo-id>:<rust_module>:<exact planned title>" ... <<'EOF'
{"new glossary term": "One plain sentence without double quotes."}
EOF

# 3. everything
cargo test --workspace 2>&1 | grep -E "^error|^warning|FAILED|panicked" -A14
cargo test --workspace 2>&1 | grep "test result" | awk '{ok+=$4; fail+=$6} END {print ok" passed, "fail" failed"}'

# 4. LOOK at target/shots/<demo-id>.png and <demo-id>--last-preset.png
```

Then: order the learning path by hand in `crates/demos/src/lib.rs`, add a line to the
progress log in `AGENTS.md`, update the table and the counts in `README.md`.

The register script adds the `pub mod` line, the registry entry, the path step, removes the
title from `planned()` and inserts glossary entries in order. Pass `{}` when there are no new
glossary terms. A new field needs its `LearningPath` (with `steps: &[]`) added by hand first.

### Which existing file to copy

| The demo is mostly | Copy |
|---|---|
| Rows over time (tasks, wires, frames) | `mcu/rtos.rs`, `bus/i2c.rs`, `zephyr/semaphore.rs` |
| Curves plus a formula | `mcu/adc.rs`, `signals/filter.rs` |
| Log-scale graphs, sliders spanning decades | `mcu/low_power.rs`, `zephyr/pm.rs` |
| A painted diagram (boxes, arrows) | `mcu/memory.rs`, `mcu/clocks.rs`, `firmware/state_machine.rs` |
| Step-by-step story | `mcu/boot.rs` |
| Bits and bytes, clickable | `firmware/crc.rs`, `bus/dali.rs`, `wireless/ble_adv_packet.rs` |
| Things dragged in a scene | `wireless/mesh_flooding.rs`, `radar/range_doppler.rs` |
| Animated with play/pause | `firmware/state_machine.rs` |
| A table with verdicts | `zephyr/kconfig.rs`, `zephyr/isr.rs` |

## 5. Rules I learned the hard way in this job

`AGENTS.md` has the full list under "Things that will trip you up". The ones that cost
the most time:

1. **Compute every number that appears in explanation text.** Do not estimate. Several times
   a sentence like "the hum drops to 10 %" or "the queue needs 4 slots" was wrong until I ran
   the model. Write a throwaway test or a Python one-liner, then write the sentence.
2. **Plot invented example values and check they are plausible.** A first set of sleep-state
   numbers made the deepest state cost more than the lighter one.
3. **A preset must stay inside the slider's range**, or the slider silently clamps it and the
   experiment shows something else.
4. **Sliders inside `ui.horizontal_wrapped` need `widgets::wrap`**, or the row runs off the
   right edge and drags every graph below it out of view. In each screenshot, check the right
   edge of every graph.
5. **A legend box covers data** on timelines. Use `plots::timeline` or `plots::plot_bare`
   and explain colours with `widgets::key`.
6. **Text inside an `egui::Grid`** wraps letter by letter when squeezed: add it as
   `ui.add(egui::Label::new(text).extend())`.
7. **Painter text on a filled box**: `ui.visuals().strong_text_color()`, not `pal.text`.
8. **`painter.arrow`** draws a head a quarter of the arrow's length. For long arrows draw a
   line and a short arrow at its end.
9. **Monospace boxes** (`widgets::formula`) wrap at roughly 95 characters in the screenshot
   width. Keep code and formula lines shorter than that.
10. **Only the top 950 pixels are in the screenshot.** To see a tall demo, add a one-off test
    like the one below, look at the image, then delete the test file again.

```rust
// crates/app/tests/tmp_tall.rs  (delete after use)
use egui_kittest::Harness;
use embedded_lab::{Saved, Shell};
#[test]
fn tall_shot() {
    let reg = demos::registry();
    let index = reg.index_of("<demo-id>").unwrap();
    let mut shell = Shell::new(reg, Saved::default());
    shell.select(index, None);
    let mut h = Harness::builder().with_size(egui::vec2(1500.0, 1750.0)).with_step_dt(1.0 / 30.0).wgpu().build_ui_state(|ui, shell: &mut Shell| shell.ui(ui), shell);
    h.run_steps(20);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/shots");
    h.render().expect("render").save(dir.join("tmp-tall.png")).unwrap();
}
```

## 6. What is not verified

Be honest with the user about these; do not present them as checked.

- **Sources were read through page summaries**, not line by line: the Zephyr documentation and
  kernel source, the CRC catalogue, and the Wikipedia articles behind CAN, RS-485 and DALI.
  The CRC check values and the Zephyr scheduling rules are additionally pinned by tests.
- **CAN, RS-485 and DALI constants come from Wikipedia**, not from the paid standards. Their
  `refs` say so.
- **The real window was never opened in this job.** Everything was judged from headless
  screenshots in dark mode at 1500 x 950. Light mode and the small window run in the tests
  without a panic, but nobody looked at them for the new demos.
- **Only some `--last-preset` screenshots were looked at.** Each demo's opening screenshot was.
- **Older demos may still have the slider cut-off** from rule 4: spectrum, doppler,
  range_doppler, mesh_flooding, mesh_pdu, modulation, wave_basics, pwm. Not checked.
- `cargo clippy --workspace` reports 20 warnings. I did not review them.
- The browser (wasm) build is not wired up and was never compiled.
- **Radar example values**: the glass/plasterboard loss (-3 dB), the radar motion threshold
  and 5 Hz filter in `radar-vs-pir`, and the presence threshold (-20 dB) are illustrative and
  say so. The CFAR formula is derived in the code and checked by Monte Carlo, not quoted from
  a book (Wikipedia's CFAR page does not give it; MathWorks' page renders it garbled).
- **Not checked against a datasheet**: the claim in `radar-presence` that 60 GHz is used by
  newer presence sensors is general knowledge, not sourced.

## 7. When all 19 are built

1. `planned()` returns an empty list: check the library tree still renders (the test does).
2. Replace the "Job in progress" section in `AGENTS.md` with one line saying it is done, and
   delete this file or mark it finished.
3. Then, if the user wants more: go through the eight older demos from section 6 for the
   slider cut-off, look at light mode, and review the clippy warnings.
4. Tell the user what was built, what was verified and how, and what was not.
