//! Techniques that turn up in every firmware project: state machines, ring buffers,
//! fixed-point arithmetic, checksums, and reading a serial line from its waveform.

// ---------------------------------------------------------------------------------
// State machine: a lamp controlled by a motion sensor
// ---------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightState {
    Off,
    On,
    /// Dimmed: a warning that the lamp is about to go out.
    Warning,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightEvent {
    Motion,
    Button,
    /// The running timer has expired.
    Timeout,
}

impl LightState {
    pub const ALL: [LightState; 3] = [LightState::Off, LightState::On, LightState::Warning];

    pub fn name(self) -> &'static str {
        ["OFF", "ON", "WARNING"][self as usize]
    }

    /// Lamp brightness in this state, 0..=1.
    pub fn brightness(self) -> f64 {
        [0.0, 1.0, 0.3][self as usize]
    }
}

impl LightEvent {
    pub const ALL: [LightEvent; 3] = [LightEvent::Motion, LightEvent::Button, LightEvent::Timeout];

    pub fn name(self) -> &'static str {
        ["motion", "button", "timer expired"][self as usize]
    }
}

/// The whole behaviour as one table: in `state`, when `event` happens, go to the
/// returned state and do the returned action. `None`: the event is ignored.
pub fn light_transition(state: LightState, event: LightEvent) -> Option<(LightState, &'static str)> {
    use LightEvent::*;
    use LightState::*;
    match (state, event) {
        (Off, Motion) | (Off, Button) => Some((On, "lamp 100 %, start hold timer")),
        (Off, Timeout) => None,
        (On, Motion) => Some((On, "restart hold timer")),
        (On, Button) => Some((Off, "lamp off")),
        (On, Timeout) => Some((Warning, "lamp 30 %, start warning timer")),
        (Warning, Motion) => Some((On, "lamp 100 %, start hold timer")),
        (Warning, Button) | (Warning, Timeout) => Some((Off, "lamp off")),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LightFsm {
    pub state: LightState,
    /// Seconds left on the running timer (meaningless in `Off`).
    pub timer_s: f64,
    pub hold_s: f64,
    pub warn_s: f64,
    pub now_s: f64,
    /// `(time, state before, event, state after)`, newest last.
    pub log: Vec<(f64, LightState, LightEvent, LightState)>,
}

impl LightFsm {
    pub fn new(hold_s: f64, warn_s: f64) -> Self {
        Self { state: LightState::Off, timer_s: 0.0, hold_s, warn_s, now_s: 0.0, log: Vec::new() }
    }

    /// Feed one event into the machine.
    pub fn fire(&mut self, event: LightEvent) {
        if let Some((next, _)) = light_transition(self.state, event) {
            self.log.push((self.now_s, self.state, event, next));
            if self.log.len() > 200 {
                self.log.remove(0);
            }
            self.timer_s = match next {
                LightState::On => self.hold_s,
                LightState::Warning => self.warn_s,
                LightState::Off => 0.0,
            };
            self.state = next;
        }
    }

    /// Let `dt` seconds pass; fires `Timeout` when the timer runs out.
    pub fn advance(&mut self, dt: f64) {
        let mut left = dt;
        while left > 0.0 {
            if self.state == LightState::Off || self.timer_s > left {
                self.timer_s -= left;
                self.now_s += left;
                break;
            }
            left -= self.timer_s.max(0.0);
            self.now_s += self.timer_s.max(0.0);
            self.timer_s = 0.0;
            self.fire(LightEvent::Timeout);
        }
    }
}

// ---------------------------------------------------------------------------------
// Ring buffer
// ---------------------------------------------------------------------------------

/// A fixed block of memory used as a queue of bytes: `head` is where the next byte is
/// written, `tail` where the next byte is read, and both wrap around at the end.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ring {
    pub slots: Vec<u8>,
    pub head: usize,
    pub tail: usize,
    pub count: usize,
    /// Bytes that were refused because the buffer was full.
    pub dropped: u32,
}

impl Ring {
    pub fn new(capacity: usize) -> Self {
        Self { slots: vec![0; capacity.max(1)], head: 0, tail: 0, count: 0, dropped: 0 }
    }

    pub fn capacity(&self) -> usize {
        self.slots.len()
    }

    pub fn is_full(&self) -> bool {
        self.count == self.capacity()
    }

    /// Write a byte. Returns false (and counts a drop) if the buffer is full.
    pub fn put(&mut self, byte: u8) -> bool {
        if self.is_full() {
            self.dropped += 1;
            return false;
        }
        self.slots[self.head] = byte;
        self.head = (self.head + 1) % self.capacity();
        self.count += 1;
        true
    }

    /// Read the oldest byte.
    pub fn get(&mut self) -> Option<u8> {
        if self.count == 0 {
            return None;
        }
        let byte = self.slots[self.tail];
        self.tail = (self.tail + 1) % self.capacity();
        self.count -= 1;
        Some(byte)
    }

    /// Is the slot at `index` holding a byte that has not been read yet?
    pub fn is_used(&self, index: usize) -> bool {
        (index + self.capacity() - self.tail) % self.capacity() < self.count
    }
}

// ---------------------------------------------------------------------------------
// Fixed-point arithmetic in 16 bits
// ---------------------------------------------------------------------------------

/// The value of one step with `frac_bits` bits after the binary point.
pub fn q_step(frac_bits: u32) -> f64 {
    1.0 / (1u32 << frac_bits) as f64
}

/// Store `x` as a 16-bit fixed-point number, rounded to the nearest step. The flag is
/// true if `x` was out of range and had to be clipped.
pub fn to_q(x: f64, frac_bits: u32) -> (i16, bool) {
    let raw = (x / q_step(frac_bits)).round();
    (raw.clamp(i16::MIN as f64, i16::MAX as f64) as i16, raw < i16::MIN as f64 || raw > i16::MAX as f64)
}

pub fn from_q(raw: i16, frac_bits: u32) -> f64 {
    raw as f64 * q_step(frac_bits)
}

/// Multiply two fixed-point numbers of the same format: multiply the raw integers in
/// 32 bits, then shift right by the number of fraction bits. The flag is true if the
/// result did not fit into 16 bits and was clipped.
pub fn q_mul(a: i16, b: i16, frac_bits: u32) -> (i16, bool) {
    let wide = (a as i32 * b as i32) >> frac_bits;
    (wide.clamp(i16::MIN as i32, i16::MAX as i32) as i16, wide < i16::MIN as i32 || wide > i16::MAX as i32)
}

// ---------------------------------------------------------------------------------
// Checksums and CRCs
// ---------------------------------------------------------------------------------

/// The parameters of a CRC as the CRC catalogue (reveng.sourceforge.io) writes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CrcSpec {
    pub name: &'static str,
    pub width: u32,
    pub poly: u32,
    pub init: u32,
    pub refin: bool,
    pub refout: bool,
    pub xorout: u32,
}

pub const CRC8_SMBUS: CrcSpec = CrcSpec { name: "CRC-8/SMBUS", width: 8, poly: 0x07, init: 0x00, refin: false, refout: false, xorout: 0x00 };
pub const CRC16_IBM_3740: CrcSpec = CrcSpec { name: "CRC-16/IBM-3740 (CCITT-FALSE)", width: 16, poly: 0x1021, init: 0xffff, refin: false, refout: false, xorout: 0x0000 };
pub const CRC16_XMODEM: CrcSpec = CrcSpec { name: "CRC-16/XMODEM", width: 16, poly: 0x1021, init: 0x0000, refin: false, refout: false, xorout: 0x0000 };
pub const CRC16_ARC: CrcSpec = CrcSpec { name: "CRC-16/ARC", width: 16, poly: 0x8005, init: 0x0000, refin: true, refout: true, xorout: 0x0000 };
pub const CRC32_ISO_HDLC: CrcSpec = CrcSpec { name: "CRC-32/ISO-HDLC", width: 32, poly: 0x04c1_1db7, init: 0xffff_ffff, refin: true, refout: true, xorout: 0xffff_ffff };

/// Bit-by-bit CRC, straight from the definition: feed the message through a shift
/// register, and whenever a 1 falls out of the top, XOR the polynomial in.
pub fn crc(spec: &CrcSpec, data: &[u8]) -> u32 {
    let top = 1u64 << (spec.width - 1);
    let mask = (1u64 << spec.width) - 1;
    let mut reg = spec.init as u64;
    for &byte in data {
        let b = if spec.refin { byte.reverse_bits() } else { byte };
        reg ^= (b as u64) << (spec.width - 8);
        for _ in 0..8 {
            reg = if reg & top != 0 { (reg << 1) ^ spec.poly as u64 } else { reg << 1 };
        }
        reg &= mask;
    }
    if spec.refout {
        reg = reg.reverse_bits() >> (64 - spec.width);
    }
    (reg as u32) ^ spec.xorout
}

/// The simplest check: add all bytes, keep the low 8 bits.
pub fn sum8(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |a, b| a.wrapping_add(*b))
}

/// XOR of all bytes (a parity bit per bit position).
pub fn xor8(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |a, b| a ^ *b)
}

// ---------------------------------------------------------------------------------
// A serial line seen through a logic analyzer
// ---------------------------------------------------------------------------------

/// The level of a UART line sending `bytes` at `baud` in the usual 8N1 framing: idle
/// high, a low start bit, eight data bits (least significant first), a high stop bit.
/// Returns `(time in seconds, new level)` and the total duration. `idle_bits` of idle
/// line come before the first and after every byte.
pub fn uart_wave(bytes: &[u8], baud: f64, idle_bits: f64) -> (Vec<(f64, bool)>, f64) {
    let bit = 1.0 / baud;
    let mut edges = Vec::new();
    let mut level = true;
    let mut t = idle_bits * bit;
    for &byte in bytes {
        let frame = std::iter::once(false).chain((0..8).map(|i| byte >> i & 1 == 1)).chain(std::iter::once(true));
        for (k, b) in frame.enumerate() {
            if b != level {
                edges.push((t + k as f64 * bit, b));
                level = b;
            }
        }
        t += (10.0 + idle_bits) * bit;
    }
    (edges, t)
}

/// What a logic analyzer records: the level at regular instants.
pub fn sample_wave(edges: &[(f64, bool)], sample_hz: f64, duration_s: f64) -> Vec<bool> {
    let n = (duration_s * sample_hz).floor() as usize;
    let mut next = 0;
    let mut level = true;
    (0..n)
        .map(|i| {
            let t = i as f64 / sample_hz;
            while next < edges.len() && edges[next].0 <= t {
                level = edges[next].1;
                next += 1;
            }
            level
        })
        .collect()
}

/// Length in samples of the shortest stretch between two changes of level. On a serial
/// line that is one bit, which gives the baud rate away: baud = sample rate / length.
pub fn shortest_pulse(samples: &[bool]) -> Option<usize> {
    let changes: Vec<usize> = (1..samples.len()).filter(|&i| samples[i] != samples[i - 1]).collect();
    changes.windows(2).map(|w| w[1] - w[0]).min()
}

/// One byte found by the decoder.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecodedByte {
    /// When its start bit began, seconds.
    pub start_s: f64,
    pub value: u8,
    /// False: the stop bit was not high where it should be (a framing error).
    pub stop_ok: bool,
}

/// Decode 8N1 bytes from samples, the way a logic analyzer's protocol decoder does:
/// wait for a falling edge, then look at the middle of each of the following bits,
/// assuming `baud`.
pub fn uart_decode(samples: &[bool], sample_hz: f64, baud: f64) -> Vec<DecodedByte> {
    let per_bit = sample_hz / baud;
    let mut out = Vec::new();
    let mut i = 1;
    while i < samples.len() {
        if samples[i - 1] && !samples[i] {
            let at = |bits: f64| (i as f64 + bits * per_bit).floor() as usize;
            if at(9.5) >= samples.len() {
                break;
            }
            let value = (0..8).fold(0u8, |v, k| v | (samples[at(1.5 + k as f64)] as u8) << k);
            out.push(DecodedByte { start_s: i as f64 / sample_hz, value, stop_ok: samples[at(9.5)] });
            i = at(9.5).max(i + 1);
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_state_machine_walks_through_its_states() {
        let mut fsm = LightFsm::new(10.0, 3.0);
        fsm.advance(5.0);
        assert_eq!(fsm.state, LightState::Off);
        fsm.fire(LightEvent::Motion);
        assert_eq!((fsm.state, fsm.timer_s), (LightState::On, 10.0));
        // Motion again after 4 s restarts the hold time.
        fsm.advance(4.0);
        fsm.fire(LightEvent::Motion);
        assert_eq!(fsm.timer_s, 10.0);
        // 10 s without motion: warning. 3 s more: off.
        fsm.advance(10.5);
        assert_eq!(fsm.state, LightState::Warning);
        assert!((fsm.timer_s - 2.5).abs() < 1e-9);
        fsm.advance(2.6);
        assert_eq!(fsm.state, LightState::Off);
        assert!((fsm.now_s - 22.1).abs() < 1e-9);
        // The log has every transition with its time.
        let states: Vec<LightState> = fsm.log.iter().map(|l| l.3).collect();
        assert_eq!(states, [LightState::On, LightState::On, LightState::Warning, LightState::Off]);
        assert!((fsm.log[2].0 - 19.0).abs() < 1e-9 && (fsm.log[3].0 - 22.0).abs() < 1e-9);
        // Motion during the warning brings the light back; the button always switches off.
        fsm.fire(LightEvent::Button);
        fsm.advance(10.0);
        fsm.fire(LightEvent::Motion);
        assert_eq!(fsm.state, LightState::On);
        fsm.fire(LightEvent::Button);
        assert_eq!(fsm.state, LightState::Off);
        assert_eq!(light_transition(LightState::Off, LightEvent::Timeout), None);
    }

    #[test]
    fn ring_buffer_wraps_and_refuses_when_full() {
        let mut r = Ring::new(4);
        assert!(r.put(1) && r.put(2) && r.put(3));
        assert_eq!(r.get(), Some(1));
        assert_eq!(r.get(), Some(2));
        // head is at 3, tail at 2: the next two writes wrap around to slot 0.
        assert!(r.put(4) && r.put(5) && r.put(6));
        assert_eq!((r.head, r.tail, r.count), (2, 2, 4));
        assert_eq!(r.slots, [5, 6, 3, 4]);
        assert!(r.is_full() && !r.put(7));
        assert_eq!(r.dropped, 1);
        // Bytes come out in the order they went in.
        assert_eq!((0..4).map(|_| r.get().unwrap()).collect::<Vec<_>>(), [3, 4, 5, 6]);
        assert_eq!(r.get(), None);
        r.put(9);
        assert!(r.is_used(2) && !r.is_used(3));
    }

    #[test]
    fn fixed_point_known_answers() {
        // Q15: 0.5 is 16384; 0.5 x 0.5 = 0.25 = 8192.
        assert_eq!(to_q(0.5, 15), (16384, false));
        assert_eq!(q_mul(16384, 16384, 15), (8192, false));
        assert_eq!(from_q(8192, 15), 0.25);
        // Q8.8: 3.75 is 960, 2.5 is 640; 3.75 x 2.5 = 9.375 = 2400.
        assert_eq!(to_q(3.75, 8), (960, false));
        assert_eq!(q_mul(960, 640, 8), (2400, false));
        assert_eq!(from_q(2400, 8), 9.375);
        // 0.1 cannot be stored exactly: with 8 fraction bits it becomes 26/256.
        assert_eq!(to_q(0.1, 8).0, 26);
        assert!((from_q(26, 8) - 0.1015625).abs() < 1e-12);
        // Out of range: Q8.8 holds -128 .. 127.996.
        assert_eq!(to_q(200.0, 8), (i16::MAX, true));
        assert_eq!(q_mul(to_q(100.0, 8).0, to_q(2.0, 8).0, 8), (i16::MAX, true));
        assert_eq!(q_step(8), 1.0 / 256.0);
    }

    #[test]
    fn crc_check_values_from_the_catalogue() {
        // The catalogue's "check" column: the CRC of the ASCII string "123456789".
        let msg = b"123456789";
        assert_eq!(crc(&CRC8_SMBUS, msg), 0xf4);
        assert_eq!(crc(&CRC16_IBM_3740, msg), 0x29b1);
        assert_eq!(crc(&CRC16_XMODEM, msg), 0x31c3);
        assert_eq!(crc(&CRC16_ARC, msg), 0xbb3d);
        assert_eq!(crc(&CRC32_ISO_HDLC, msg), 0xcbf4_3926);
    }

    #[test]
    fn what_simple_checksums_miss() {
        let good = [0x10, 0x20, 0x30, 0x40];
        // Two bytes swapped: the sum and the XOR do not notice, the CRC does.
        let swapped = [0x20, 0x10, 0x30, 0x40];
        assert_eq!(sum8(&good), sum8(&swapped));
        assert_eq!(xor8(&good), xor8(&swapped));
        assert_ne!(crc(&CRC8_SMBUS, &good), crc(&CRC8_SMBUS, &swapped));
        // One byte up by one, another down by one: the sum stays the same.
        let shifted = [0x11, 0x1f, 0x30, 0x40];
        assert_eq!(sum8(&good), sum8(&shifted));
        assert_ne!(crc(&CRC16_IBM_3740, &good), crc(&CRC16_IBM_3740, &shifted));
        // Any single flipped bit changes all of them.
        let flipped = [0x10, 0x21, 0x30, 0x40];
        assert_ne!(sum8(&good), sum8(&flipped));
        assert_ne!(xor8(&good), xor8(&flipped));
    }

    #[test]
    fn uart_waveform_round_trip() {
        // 'H' = 0x48 = 0100 1000: start(0), then 0 0 0 1 0 0 1 0 (LSB first), stop(1).
        let (edges, duration) = uart_wave(b"H", 9600.0, 2.0);
        let bit = 1.0 / 9600.0;
        let times: Vec<f64> = edges.iter().map(|e| (e.0 / bit).round()).collect();
        // Low at 2 (start), high at 6 (bit 3), low at 7, high at 9 (bit 6), low at 10 (bit 7), high at 11 (stop).
        assert_eq!(times, [2.0, 6.0, 7.0, 9.0, 10.0, 11.0]);
        assert_eq!(edges.iter().map(|e| e.1).collect::<Vec<_>>(), [false, true, false, true, false, true]);
        assert!((duration / bit - 14.0).abs() < 1e-9);
    }

    #[test]
    fn decoding_needs_enough_samples_and_the_right_baud() {
        let (edges, duration) = uart_wave(b"Hi!", 9600.0, 3.0);
        let decode = |sample_hz: f64, baud: f64| uart_decode(&sample_wave(&edges, sample_hz, duration), sample_hz, baud);
        // Ten samples per bit: clean.
        let bytes = decode(96_000.0, 9600.0);
        assert_eq!(bytes.iter().map(|b| b.value).collect::<Vec<_>>(), b"Hi!");
        assert!(bytes.iter().all(|b| b.stop_ok));
        // The shortest pulse is one bit: 10 samples give or take one, so about 96 000 / 10 = 9600 baud.
        assert!(matches!(shortest_pulse(&sample_wave(&edges, 96_000.0, duration)), Some(9..=11)));
        // A decoder set 3 % off still works; 10 % off does not.
        assert_eq!(decode(96_000.0, 9600.0 * 1.03).iter().map(|b| b.value).collect::<Vec<_>>(), b"Hi!");
        let off = decode(96_000.0, 9600.0 * 1.10);
        assert!(off.iter().map(|b| b.value).collect::<Vec<_>>() != b"Hi!" || off.iter().any(|b| !b.stop_ok));
        // Fewer than about two samples per bit: garbage.
        let coarse = decode(12_000.0, 9600.0);
        assert!(coarse.iter().map(|b| b.value).collect::<Vec<_>>() != b"Hi!" || coarse.iter().any(|b| !b.stop_ok));
    }
}
