//! Wired buses: how bytes travel between chips and devices over a few wires.
//!
//! Signals are returned as lists of `(time, new level)`. Time is counted in bit
//! periods (UART) or clock periods (SPI, I2C), so the pictures do not depend on speed.

/// A change of level on a wire.
pub type Edge = (f64, bool);

/// The level of a signal at time `t`.
pub fn level_at(edges: &[Edge], initial: bool, t: f64) -> bool {
    edges.iter().take_while(|e| e.0 <= t).last().map_or(initial, |e| e.1)
}

fn push(edges: &mut Vec<Edge>, level: &mut bool, t: f64, new: bool) {
    if new != *level {
        edges.push((t, new));
        *level = new;
    }
}

// ---------------------------------------------------------------------------------
// UART: one byte as a frame of bits, and a receiver with its own clock
// ---------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Parity {
    None,
    Even,
    Odd,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UartBit {
    Start,
    Data(u8),
    Parity,
    Stop,
}

/// The bits of one UART frame in the order they are sent: a low start bit, eight data
/// bits least significant first, an optional parity bit, a high stop bit.
pub fn uart_frame(byte: u8, parity: Parity) -> Vec<(UartBit, bool)> {
    let mut bits = vec![(UartBit::Start, false)];
    bits.extend((0..8).map(|i| (UartBit::Data(i), byte >> i & 1 == 1)));
    let ones_odd = byte.count_ones() % 2 == 1;
    match parity {
        Parity::None => {}
        // Even parity: the parity bit makes the total number of ones even.
        Parity::Even => bits.push((UartBit::Parity, ones_odd)),
        Parity::Odd => bits.push((UartBit::Parity, !ones_odd)),
    }
    bits.push((UartBit::Stop, true));
    bits
}

#[derive(Clone, Debug, PartialEq)]
pub struct UartRx {
    /// When the receiver looked at the line (in the sender's bit periods) and what it saw.
    pub samples: Vec<(f64, bool)>,
    pub byte: u8,
    /// `None` without a parity bit.
    pub parity_ok: Option<bool>,
    pub stop_ok: bool,
}

/// A receiver whose bit period is `1 + clock_error` times the sender's. It starts
/// counting at the falling edge of the start bit and looks at the middle of each bit
/// as it believes them to be. After the frame the line is idle (high).
pub fn uart_receive(frame: &[(UartBit, bool)], clock_error: f64) -> UartRx {
    let samples: Vec<(f64, bool)> = (0..frame.len())
        .map(|k| {
            let t = (k as f64 + 0.5) * (1.0 + clock_error);
            (t, frame.get(t.floor() as usize).map_or(true, |b| b.1))
        })
        .collect();
    let byte = (0..8).fold(0u8, |v, i| v | (samples[1 + i].1 as u8) << i);
    let has_parity = frame.iter().any(|b| b.0 == UartBit::Parity);
    let parity_ok = has_parity.then(|| {
        let sent_even = frame.iter().filter(|b| matches!(b.0, UartBit::Data(_) | UartBit::Parity) && b.1).count() % 2 == 0;
        let seen_even = (byte.count_ones() as usize + samples[9].1 as usize) % 2 == 0;
        sent_even == seen_even
    });
    UartRx { byte, parity_ok, stop_ok: samples[frame.len() - 1].1, samples }
}

// ---------------------------------------------------------------------------------
// SPI: a clock, two data wires, and four ways to agree on the clock
// ---------------------------------------------------------------------------------

/// CPOL: the level of the clock while idle. CPHA: false = data is sampled on the first
/// (leading) clock edge of each bit, true = on the second (trailing) edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpiMode {
    pub cpol: bool,
    pub cpha: bool,
}

impl SpiMode {
    pub fn number(self) -> u8 {
        (self.cpol as u8) << 1 | self.cpha as u8
    }

    pub fn from_number(n: u8) -> Self {
        Self { cpol: n & 2 != 0, cpha: n & 1 != 0 }
    }

    /// Does this mode sample on rising clock edges?
    pub fn samples_on_rising(self) -> bool {
        self.cpol == self.cpha
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpiTransfer {
    /// Chip select: low from `cs.0` to `cs.1`.
    pub cs: (f64, f64),
    pub sck: Vec<Edge>,
    pub mosi: Vec<Edge>,
    pub miso: Vec<Edge>,
    /// The instants the controller reads MISO, and the peripheral reads MOSI.
    pub controller_samples: Vec<f64>,
    pub peripheral_samples: Vec<f64>,
    pub controller_read: u8,
    pub peripheral_read: u8,
    /// The shortest time between one of a side's sampling instants and a change of the
    /// data wire it reads, in clock periods. Half a period when the modes agree; zero
    /// means "looking exactly while the data changes", which no real circuit survives.
    pub controller_margin: f64,
    pub peripheral_margin: f64,
    pub duration: f64,
}

/// One byte each way, most significant bit first. The controller makes the clock in
/// its mode; the peripheral interprets the clock edges in its own mode. A side that
/// samples "on an edge" sees the level just before that edge.
pub fn spi_transfer(controller: SpiMode, peripheral: SpiMode, mosi_byte: u8, miso_byte: u8) -> SpiTransfer {
    const CS_FALL: f64 = 0.5;
    // Sixteen clock edges: the leading edge of bit k at 1 + k, the trailing at 1.5 + k.
    let edge_times: Vec<f64> = (0..16).map(|j| 1.0 + 0.5 * j as f64).collect();
    let mut sck = Vec::new();
    let mut level = controller.cpol;
    for &t in &edge_times {
        level = !level;
        sck.push((t, level));
    }
    let rising = |j: usize| sck[j].1;
    // A sender in `mode` puts out its next bit on every edge it does not sample on; with
    // CPHA = 0 the first bit is already there when chip select falls.
    let drive = |mode: SpiMode, byte: u8| -> Vec<Edge> {
        let bit = |i: usize| i < 8 && byte >> (7 - i) & 1 == 1;
        let mut edges = Vec::new();
        let mut level = true;
        let mut next = 0;
        if !mode.cpha {
            push(&mut edges, &mut level, CS_FALL, bit(0));
            next = 1;
        }
        for (j, &t) in edge_times.iter().enumerate() {
            if rising(j) != mode.samples_on_rising() && next < 8 {
                push(&mut edges, &mut level, t, bit(next));
                next += 1;
            }
        }
        edges
    };
    let sample_times = |mode: SpiMode| -> Vec<f64> { edge_times.iter().enumerate().filter(|(j, _)| rising(*j) == mode.samples_on_rising()).map(|(_, &t)| t).take(8).collect() };
    let read = |edges: &[Edge], times: &[f64]| times.iter().fold(0u8, |v, &t| v << 1 | level_at(edges, true, t - 1e-6) as u8);
    // Where a side could change its data: at chip select (CPHA = 0) and on every edge it
    // does not sample on. Taken from the mode, not from the bytes, so that the margin does
    // not depend on which bits happen to differ.
    let change_times = |mode: SpiMode| -> Vec<f64> {
        let first = if mode.cpha { None } else { Some(CS_FALL) };
        first.into_iter().chain(edge_times.iter().enumerate().filter(|(j, _)| rising(*j) != mode.samples_on_rising()).map(|(_, &t)| t)).collect()
    };
    let margin = |samples: &[f64], changes: &[f64]| samples.iter().flat_map(|s| changes.iter().map(move |c| (s - c).abs())).fold(f64::INFINITY, f64::min);
    let (mosi, miso) = (drive(controller, mosi_byte), drive(peripheral, miso_byte));
    let (controller_samples, peripheral_samples) = (sample_times(controller), sample_times(peripheral));
    SpiTransfer {
        controller_margin: margin(&controller_samples, &change_times(peripheral)),
        peripheral_margin: margin(&peripheral_samples, &change_times(controller)),
        cs: (CS_FALL, 9.5),
        controller_read: read(&miso, &controller_samples),
        peripheral_read: read(&mosi, &peripheral_samples),
        sck,
        mosi,
        miso,
        controller_samples,
        peripheral_samples,
        duration: 10.0,
    }
}

// ---------------------------------------------------------------------------------
// I2C: two wires, an address, and an acknowledge bit
// ---------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum I2cSlot {
    Address(u8),
    /// The read/write bit: low = the controller writes, high = it reads.
    ReadWrite,
    /// The ninth clock of a byte: the receiver of the byte pulls the data line low to
    /// acknowledge. `true` = acknowledged.
    Ack(bool),
    Data(u8),
}

#[derive(Clone, Debug, PartialEq)]
pub struct I2cTransfer {
    pub scl: Vec<Edge>,
    pub sda: Vec<Edge>,
    /// One entry per clock pulse: what the bit is, its value, and whether the device
    /// (not the controller) is driving the data line. Slot `k` spans `1 + k .. 2 + k`.
    pub slots: Vec<(I2cSlot, bool, bool)>,
    /// Did a device answer to the address?
    pub address_acked: bool,
    pub stop_at: f64,
    pub duration: f64,
}

/// One transaction on a bus with a single device at `device_address`: START, seven
/// address bits (most significant first), the read/write bit, the acknowledge bit,
/// and - if a device answered - one data byte with its acknowledge bit, then STOP.
/// When reading, the controller answers the last byte with "not acknowledge".
pub fn i2c_transfer(address: u8, read: bool, data: u8, device_address: u8) -> I2cTransfer {
    let address = address & 0x7f;
    let acked = address == device_address & 0x7f;
    let mut slots: Vec<(I2cSlot, bool, bool)> = (0..7).map(|i| (I2cSlot::Address(i), address >> (6 - i) & 1 == 1, false)).collect();
    slots.push((I2cSlot::ReadWrite, read, false));
    // An acknowledge is the line pulled LOW, so its level is `!acked`.
    slots.push((I2cSlot::Ack(acked), !acked, true));
    if acked {
        slots.extend((0..8).map(|i| (I2cSlot::Data(i), data >> (7 - i) & 1 == 1, read)));
        // Write: the device acknowledges. Read: the controller ends with a NACK.
        slots.push((I2cSlot::Ack(!read), read, !read));
    }
    let (mut scl, mut sda) = (Vec::new(), Vec::new());
    let (mut scl_level, mut sda_level) = (true, true);
    // START: data falls while the clock is high, then the clock falls.
    push(&mut sda, &mut sda_level, 0.5, false);
    push(&mut scl, &mut scl_level, 0.75, false);
    for (k, slot) in slots.iter().enumerate() {
        let t = 1.0 + k as f64;
        push(&mut sda, &mut sda_level, t + 0.1, slot.1);
        push(&mut scl, &mut scl_level, t + 0.5, true);
        push(&mut scl, &mut scl_level, t + 1.0, false);
    }
    // STOP: data is brought low, the clock rises, then data rises while the clock is high.
    let end = 1.0 + slots.len() as f64;
    push(&mut sda, &mut sda_level, end + 0.1, false);
    push(&mut scl, &mut scl_level, end + 0.5, true);
    push(&mut sda, &mut sda_level, end + 0.75, true);
    I2cTransfer { scl, sda, slots, address_acked: acked, stop_at: end + 0.75, duration: end + 1.5 }
}

// ---------------------------------------------------------------------------------
// CAN: several nodes start at once, and the lowest identifier wins
// ---------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct CanArbitration {
    /// Per node, per bit: the level it puts on the bus (`true` = recessive, a 1), or
    /// `None` once it has dropped out and only listens. Bit 0 is the start-of-frame
    /// bit, bits 1..=11 are the identifier, most significant bit first.
    pub sent: Vec<Vec<Option<bool>>>,
    /// The level on the bus: dominant (false) if any node sends dominant.
    pub bus: Vec<bool>,
    /// Per node: the bit at which it lost.
    pub lost_at: Vec<Option<usize>>,
    /// Nodes that are still sending after the identifier.
    pub winners: Vec<usize>,
}

/// Arbitration between nodes that start a frame at the same instant. "A 0 data bit
/// encodes a dominant state, while a 1 data bit encodes a recessive state"; the bus is
/// a wired AND. "When a node transmits a logical 1 but sees a logical 0, it realizes
/// that there is a contention and it quits transmitting."
pub fn can_arbitrate(ids: &[u16]) -> CanArbitration {
    let bit = |id: u16, k: usize| if k == 0 { false } else { id >> (11 - k) & 1 == 1 };
    let mut active = vec![true; ids.len()];
    let mut out = CanArbitration { sent: vec![Vec::new(); ids.len()], bus: Vec::new(), lost_at: vec![None; ids.len()], winners: Vec::new() };
    for k in 0..12 {
        let bus = ids.iter().zip(&active).filter(|(_, a)| **a).all(|(&id, _)| bit(id, k));
        out.bus.push(bus);
        for (i, &id) in ids.iter().enumerate() {
            out.sent[i].push(active[i].then(|| bit(id, k)));
            if active[i] && bit(id, k) && !bus {
                active[i] = false;
                out.lost_at[i] = Some(k);
            }
        }
    }
    out.winners = (0..ids.len()).filter(|&i| active[i]).collect();
    out
}

// ---------------------------------------------------------------------------------
// RS-485: a pair of wires and the difference between them
// ---------------------------------------------------------------------------------

/// A data link through an electrically noisy place, once as a single wire measured
/// against ground and once as an RS-485 pair.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoisyLink {
    /// Size of the wanted signal: the single wire swings 0..drive, the pair differs by +-drive.
    pub drive_v: f64,
    /// Interference picked up along the cable, peak volts.
    pub noise_v: f64,
    /// How much of that interference hits only one wire of the pair, 0..=1. A well
    /// twisted pair is near 0.
    pub imbalance: f64,
    /// Difference between the ground potentials of sender and receiver, volts.
    pub ground_shift_v: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LinkTrace {
    /// Time of each point, in bit periods.
    pub t: Vec<f64>,
    /// The single wire, and the two wires of the pair, as the receiver sees them.
    pub single: Vec<f64>,
    pub a: Vec<f64>,
    pub b: Vec<f64>,
    pub single_read: Vec<bool>,
    pub pair_read: Vec<bool>,
    pub single_errors: usize,
    pub pair_errors: usize,
    /// Highest and lowest voltage on either wire of the pair.
    pub pair_range: (f64, f64),
}

/// RS-485 limits: receivers "detect a differential input down to 200 mV", and "the
/// allowable common-mode voltage is in the range -7 V to +12 V".
pub const RS485_THRESHOLD_V: f64 = 0.2;
pub const RS485_COMMON_MODE_V: (f64, f64) = (-7.0, 12.0);

/// Send `bits` over both kinds of link, with the same interference on each. The
/// interference is a slow hum plus short spikes. Each receiver looks in the middle of
/// every bit: the single-ended one compares its wire with half the drive voltage, the
/// differential one compares the two wires with each other.
pub fn noisy_link(link: &NoisyLink, bits: &[bool], seed: u64) -> LinkTrace {
    const PER_BIT: usize = 20;
    let mut rng = crate::rng::Rng::new(seed);
    let mut tr = LinkTrace { pair_range: (f64::INFINITY, f64::NEG_INFINITY), ..Default::default() };
    let mut spike = 0.0;
    for (k, &bit) in bits.iter().enumerate() {
        for j in 0..PER_BIT {
            let t = k as f64 + j as f64 / PER_BIT as f64;
            if rng.f64() < 0.06 {
                spike = rng.range(-1.0, 1.0);
            }
            spike *= 0.6;
            let noise = link.noise_v * (0.6 * (t * 2.3).sin() + 0.8 * spike).clamp(-1.0, 1.0);
            let common = noise * (1.0 - link.imbalance) + link.ground_shift_v;
            // The pair idles around 2.5 V; a 1 makes B more positive than A.
            let half = link.drive_v / 2.0 * if bit { -1.0 } else { 1.0 };
            tr.t.push(t);
            tr.single.push(if bit { link.drive_v } else { 0.0 } + noise + link.ground_shift_v);
            tr.a.push(2.5 + half + common + noise * link.imbalance);
            tr.b.push(2.5 - half + common);
        }
        let mid = k * PER_BIT + PER_BIT / 2;
        let single = tr.single[mid] > link.drive_v / 2.0;
        // Between the two thresholds the receiver's output is undefined: count it as wrong.
        let diff = tr.a[mid] - tr.b[mid];
        let pair = if diff < -RS485_THRESHOLD_V { Some(true) } else if diff > RS485_THRESHOLD_V { Some(false) } else { None };
        tr.single_read.push(single);
        tr.pair_read.push(pair.unwrap_or(!bit));
        tr.single_errors += (single != bit) as usize;
        tr.pair_errors += (pair != Some(bit)) as usize;
    }
    for v in tr.a.iter().chain(&tr.b) {
        tr.pair_range = (tr.pair_range.0.min(*v), tr.pair_range.1.max(*v));
    }
    tr
}

// ---------------------------------------------------------------------------------
// DALI: the lighting bus
// ---------------------------------------------------------------------------------

/// DALI's fixed speed: 1200 bit/s.
pub const DALI_BIT_S: f64 = 1.0 / 1200.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DaliAddress {
    /// One device, 0..=63.
    Short(u8),
    /// A group of devices, 0..=15.
    Group(u8),
    Broadcast,
}

/// The first byte of a frame: `0AAA AAAS` for a single device, `100A AAAS` for a
/// group, `1111 111S` for everyone. S = 0: the second byte is a light level; S = 1: it
/// is a command.
pub fn dali_address_byte(address: DaliAddress, command: bool) -> u8 {
    let s = command as u8;
    match address {
        DaliAddress::Short(a) => (a & 0x3f) << 1 | s,
        DaliAddress::Group(g) => 0x80 | (g & 0x0f) << 1 | s,
        DaliAddress::Broadcast => 0xfe | s,
    }
}

/// A 16-bit forward frame as bits: a start bit (1), then the address byte and the
/// data byte, each most significant bit first.
pub fn dali_frame_bits(address_byte: u8, data_byte: u8) -> Vec<bool> {
    let word = (address_byte as u16) << 8 | data_byte as u16;
    std::iter::once(true).chain((0..16).rev().map(move |i| word >> i & 1 == 1)).collect()
}

/// The bus level over time (in bit periods) for a row of bits in DALI's Manchester
/// code: "a 1 bit is low for the first half of the bit time, and high for the second,
/// while 0 is the reverse". The bus is high when idle.
pub fn dali_manchester(bits: &[bool], start: f64) -> Vec<Edge> {
    let mut edges = Vec::new();
    let mut level = true;
    for (k, &bit) in bits.iter().enumerate() {
        let t = start + k as f64;
        push(&mut edges, &mut level, t, !bit);
        push(&mut edges, &mut level, t + 0.5, bit);
    }
    push(&mut edges, &mut level, start + bits.len() as f64, true);
    edges
}

/// Light output for a DALI level on the standard logarithmic curve, as a fraction of
/// full power: level 0 is off, 1 is 0.1 %, 254 is 100 %, and each step in between
/// multiplies the output by the same factor: `10^(3 (level - 254) / 253)`.
pub fn dali_output(level: u8) -> f64 {
    if level == 0 { 0.0 } else { 10f64.powf(3.0 * (level.min(254) as f64 - 254.0) / 253.0) }
}

// ---------------------------------------------------------------------------------
// USB: the host asks, the device answers
// ---------------------------------------------------------------------------------

/// What a device answered when the host asked it for data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsbAnswer {
    /// "Nothing to report": a NAK handshake.
    Nak,
    /// A data packet, which the host acknowledges.
    Data,
}

/// One poll of a device: the host sends an IN token at the start of frame `frame`
/// (frames are 1 ms long at full speed) and the device answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsbPoll {
    pub frame: u32,
    pub answer: UsbAnswer,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct UsbTrace {
    pub polls: Vec<UsbPoll>,
    /// Per event: `(when it happened, when the host learned of it)`, milliseconds.
    pub reported: Vec<(f64, Option<f64>)>,
    pub worst_delay_ms: f64,
}

/// A device such as a keyboard on a full-speed USB port. It cannot speak by itself:
/// the host polls it once every `interval_ms` frames. Something that happens at one
/// of `events_ms` (a key press) is reported at the next poll after it; several events
/// between two polls go out together.
pub fn usb_polling(interval_ms: u32, events_ms: &[f64], duration_ms: u32) -> UsbTrace {
    let interval = interval_ms.max(1);
    let mut tr = UsbTrace { reported: events_ms.iter().map(|e| (*e, None)).collect(), ..Default::default() };
    for frame in (0..duration_ms).step_by(interval as usize) {
        let now = frame as f64;
        let mut any = false;
        for r in tr.reported.iter_mut().filter(|r| r.1.is_none() && r.0 <= now) {
            r.1 = Some(now);
            tr.worst_delay_ms = tr.worst_delay_ms.max(now - r.0);
            any = true;
        }
        tr.polls.push(UsbPoll { frame, answer: if any { UsbAnswer::Data } else { UsbAnswer::Nak } });
    }
    tr
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uart_frame_and_parity() {
        // 0x41 'A' = 0100 0001, two ones.
        let bits: Vec<bool> = uart_frame(0x41, Parity::None).iter().map(|b| b.1).collect();
        assert_eq!(bits, [false, true, false, false, false, false, false, true, false, true]);
        // Even parity: two ones already, so the parity bit is 0. Odd: 1.
        assert_eq!(uart_frame(0x41, Parity::Even)[9], (UartBit::Parity, false));
        assert_eq!(uart_frame(0x41, Parity::Odd)[9], (UartBit::Parity, true));
        assert_eq!(uart_frame(0x43, Parity::Even)[9], (UartBit::Parity, true));
        assert_eq!(uart_frame(0x41, Parity::Even).len(), 11);
    }

    #[test]
    fn uart_receiver_tolerates_small_clock_errors_only() {
        let frame = uart_frame(0x55, Parity::None);
        for error in [0.0, 0.03, -0.03] {
            let rx = uart_receive(&frame, error);
            assert_eq!((rx.byte, rx.stop_ok), (0x55, true), "{error}");
        }
        // 8 % fast: the sample meant for data bit 7 (at 8.5 x 0.92 = 7.82) lands in bit 6.
        let rx = uart_receive(&frame, -0.08);
        assert_ne!(rx.byte, 0x55);
        // 8 % slow: the sample meant for the stop bit is fine, but bit 7 is read at 9.18: the stop bit.
        let rx = uart_receive(&uart_frame(0x00, Parity::None), 0.08);
        assert_eq!(rx.byte, 0x80);
        // A misread bit shows up as a parity error.
        let rx = uart_receive(&uart_frame(0x00, Parity::Even), 0.08);
        assert_eq!(rx.parity_ok, Some(false));
        assert_eq!(uart_receive(&uart_frame(0x35, Parity::Odd), 0.0).parity_ok, Some(true));
    }

    #[test]
    fn spi_works_in_every_mode_when_both_sides_agree() {
        for n in 0..4 {
            let mode = SpiMode::from_number(n);
            let t = spi_transfer(mode, mode, 0xA5, 0x3C);
            assert_eq!((t.controller_read, t.peripheral_read), (0x3C, 0xA5), "mode {n}");
            // The idle level of the clock is CPOL.
            assert_eq!(t.sck[0].1, !mode.cpol);
            assert_eq!(t.controller_samples.len(), 8);
            // Each side looks half a clock period away from any change of the data.
            assert_eq!((t.controller_margin, t.peripheral_margin), (0.5, 0.5));
        }
        // Mode 0 samples on the leading (rising) edges at 1, 2 ... 8; mode 1 on the trailing ones.
        assert_eq!(spi_transfer(SpiMode::from_number(0), SpiMode::from_number(0), 0, 0).controller_samples[0], 1.0);
        assert_eq!(spi_transfer(SpiMode::from_number(1), SpiMode::from_number(1), 0, 0).controller_samples[0], 1.5);
        assert!(SpiMode::from_number(0).samples_on_rising() && SpiMode::from_number(3).samples_on_rising());
        assert!(!SpiMode::from_number(1).samples_on_rising() && !SpiMode::from_number(2).samples_on_rising());
    }

    #[test]
    fn spi_mode_mismatch_garbles_the_data() {
        // Controller in mode 0, peripheral in mode 1: the peripheral puts out each bit on
        // the very edge the controller samples on, so the controller reads every bit one
        // place late (and the idle level first).
        let t = spi_transfer(SpiMode::from_number(0), SpiMode::from_number(1), 0xA5, 0x3C);
        assert_eq!(t.controller_read, 0x80 | 0x3C >> 1);
        // The peripheral samples at the last instant before the controller changes its
        // data: right in this ideal model, with no margin at all on a real wire.
        assert_eq!(t.peripheral_read, 0xA5);
        assert_eq!((t.controller_margin, t.peripheral_margin), (0.0, 0.0));
        // Every mismatch that mixes up the sampling edge leaves no margin at all.
        for (c, p) in [(0, 2), (1, 3), (3, 2), (2, 0)] {
            let t = spi_transfer(SpiMode::from_number(c), SpiMode::from_number(p), 0xA5, 0x3C);
            assert_eq!(t.controller_margin, 0.0, "{c} vs {p}");
        }
        // Modes 0 and 3 sample on the same edges; they differ in when the first bit appears.
        let t = spi_transfer(SpiMode::from_number(0), SpiMode::from_number(3), 0xA5, 0x3C);
        assert_eq!(t.controller_margin, 0.5);
        assert_ne!(t.controller_read, 0x3C);
    }

    #[test]
    fn i2c_write_to_a_present_device() {
        // Address 0x48 = 100 1000, write, data 0xA5.
        let t = i2c_transfer(0x48, false, 0xA5, 0x48);
        let levels: Vec<bool> = t.slots.iter().map(|s| s.1).collect();
        let bits = |s: &str| -> Vec<bool> { s.chars().filter(|c| *c != ' ').map(|c| c == '1').collect() };
        //                      address  rw ack  data      ack
        assert_eq!(levels, bits("1001000 0  0    10100101  0"));
        assert!(t.address_acked);
        assert_eq!(t.slots.len(), 18);
        // START: data falls at 0.5 while the clock is still high (it falls at 0.75).
        assert_eq!(t.sda[0], (0.5, false));
        assert_eq!(t.scl[0], (0.75, false));
        // STOP: data rises while the clock is high.
        let (last_sda, last_scl) = (*t.sda.last().unwrap(), *t.scl.last().unwrap());
        assert!(last_sda.1 && last_scl.1 && last_sda.0 > last_scl.0);
        // Data only changes while the clock is low, except for START and STOP.
        for &(time, _) in &t.sda[1..t.sda.len() - 1] {
            assert!(!level_at(&t.scl, true, time), "data changed at {time} with the clock high");
        }
        // The device drives the two acknowledge bits; the controller everything else.
        assert_eq!(t.slots.iter().filter(|s| s.2).count(), 2);
    }

    #[test]
    fn i2c_nobody_home_and_reading() {
        // No device at 0x49: the acknowledge bit stays high and the transaction ends.
        let t = i2c_transfer(0x49, false, 0xA5, 0x48);
        assert!(!t.address_acked);
        assert_eq!(t.slots.len(), 9);
        assert_eq!(t.slots[8], (I2cSlot::Ack(false), true, true));
        // Reading: the read/write bit is high, the device drives the data, the controller answers NACK.
        let t = i2c_transfer(0x48, true, 0x3C, 0x48);
        assert_eq!(t.slots[7], (I2cSlot::ReadWrite, true, false));
        assert!(t.slots[9..17].iter().all(|s| s.2));
        assert_eq!(t.slots[17], (I2cSlot::Ack(false), true, false));
    }

    #[test]
    fn can_lowest_identifier_wins() {
        // The textbook example: IDs 15 and 16. At identifier bit 4 (position 1 + 10 - 4 = 7)
        // node 16 sends a recessive 1, sees the dominant 0 of node 15, and stops.
        let a = can_arbitrate(&[16, 15]);
        assert_eq!(a.lost_at, [Some(7), None]);
        assert_eq!(a.winners, [1]);
        assert!(!a.bus[0], "the start-of-frame bit is dominant");
        assert_eq!(a.sent[0][7], Some(true));
        assert_eq!(a.sent[0][8], None, "after losing, a node only listens");
        // The bus carries exactly the winner's identifier.
        let id = a.bus[1..].iter().fold(0u16, |v, &b| v << 1 | b as u16);
        assert_eq!(id, 15);
        // Three nodes: 0x65 beats 0x123 and 0x7FF; the all-ones ID loses first.
        let a = can_arbitrate(&[0x123, 0x7ff, 0x065]);
        assert_eq!(a.winners, [2]);
        assert!(a.lost_at[1].unwrap() < a.lost_at[0].unwrap());
        // Two nodes with the same identifier both think they have won.
        assert_eq!(can_arbitrate(&[0x100, 0x100]).winners, [0, 1]);
    }

    #[test]
    fn differential_pair_shrugs_off_what_kills_a_single_wire() {
        let bits: Vec<bool> = (0..24).map(|i| i % 3 == 0 || i % 7 == 2).collect();
        let quiet = NoisyLink { drive_v: 3.0, noise_v: 0.0, imbalance: 0.0, ground_shift_v: 0.0 };
        let tr = noisy_link(&quiet, &bits, 1);
        assert_eq!((tr.single_errors, tr.pair_errors), (0, 0));
        assert_eq!(tr.pair_read, bits);
        // 6 V of interference common to both wires: the single wire fails, the pair does not.
        let tr = noisy_link(&NoisyLink { noise_v: 6.0, ..quiet }, &bits, 1);
        assert!(tr.single_errors > 0);
        assert_eq!(tr.pair_errors, 0);
        // A ground difference of 4 V shifts the single wire past its threshold for good.
        let tr = noisy_link(&NoisyLink { ground_shift_v: 4.0, ..quiet }, &bits, 1);
        assert!(tr.single_errors > 0);
        assert_eq!(tr.pair_errors, 0);
        // If the interference hits only one wire, the difference no longer cancels it.
        let tr = noisy_link(&NoisyLink { noise_v: 6.0, imbalance: 1.0, ..quiet }, &bits, 1);
        assert!(tr.pair_errors > 0);
        // The wires themselves must stay inside -7 .. +12 V.
        let tr = noisy_link(&NoisyLink { ground_shift_v: 11.0, ..quiet }, &bits, 1);
        assert!(tr.pair_range.1 > RS485_COMMON_MODE_V.1);
    }

    #[test]
    fn dali_frames_and_dimming_curve() {
        assert_eq!(dali_address_byte(DaliAddress::Short(5), false), 0b0000_1010);
        assert_eq!(dali_address_byte(DaliAddress::Short(63), true), 0b0111_1111);
        assert_eq!(dali_address_byte(DaliAddress::Group(3), true), 0b1000_0111);
        assert_eq!(dali_address_byte(DaliAddress::Broadcast, false), 0xfe);
        assert_eq!(dali_address_byte(DaliAddress::Broadcast, true), 0xff);
        // Start bit, then 0xFE 0x80, most significant bit first.
        let bits = dali_frame_bits(0xfe, 0x80);
        assert_eq!(bits.len(), 17);
        let s: String = bits.iter().map(|b| if *b { '1' } else { '0' }).collect();
        assert_eq!(s, "11111111010000000");
        // Manchester: a 1 is low then high. From idle high, the start bit pulls the bus low at once.
        let edges = dali_manchester(&[true, false], 2.0);
        assert_eq!(edges, [(2.0, false), (2.5, true), (3.5, false), (4.0, true)]);
        // The dimming curve: 1 -> 0.1 %, 254 -> 100 %, 2.77 % more per step.
        assert_eq!(dali_output(0), 0.0);
        assert!((dali_output(1) - 0.001).abs() < 1e-12);
        assert_eq!(dali_output(254), 1.0);
        assert!((dali_output(200) / dali_output(199) - 1.0277).abs() < 1e-4);
        // Half of the level range is only 3 % of the light; 50 % of the light is level 229.
        assert!((dali_output(127) - 0.0312).abs() < 1e-3);
        assert!((dali_output(229) - 0.505).abs() < 1e-3);
        // 17 bits at 1200 bit/s take 14.2 ms.
        assert!((17.0 * DALI_BIT_S - 0.014167).abs() < 1e-6);
    }

    #[test]
    fn usb_device_waits_to_be_asked() {
        // Polled every 8 ms. A key pressed at 13.2 ms is reported at the poll at 16 ms.
        let tr = usb_polling(8, &[13.2], 40);
        assert_eq!(tr.polls.iter().map(|p| p.frame).collect::<Vec<_>>(), [0, 8, 16, 24, 32]);
        assert_eq!(tr.polls.iter().map(|p| p.answer).collect::<Vec<_>>(), [UsbAnswer::Nak, UsbAnswer::Nak, UsbAnswer::Data, UsbAnswer::Nak, UsbAnswer::Nak]);
        assert_eq!(tr.reported, [(13.2, Some(16.0))]);
        assert!((tr.worst_delay_ms - 2.8).abs() < 1e-9);
        // Just after a poll is the worst moment: almost a whole interval of waiting.
        assert!((usb_polling(8, &[8.1], 40).worst_delay_ms - 7.9).abs() < 1e-9);
        // Polling every frame brings that down to under a millisecond.
        assert!(usb_polling(1, &[8.1], 40).worst_delay_ms < 1.0);
        // Too late for the last poll: never reported within the window.
        assert_eq!(usb_polling(8, &[39.0], 40).reported, [(39.0, None)]);
    }
}
