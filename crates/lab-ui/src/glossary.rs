//! Plain-language definitions of the words the demos use.
//!
//! Explanations mark a term as `[[term]]`; the panel then underlines it and shows the
//! definition on hover. A test makes sure every marked term has an entry here.

pub static GLOSSARY: &[(&str, &str)] = &[
    ("access address", "A 32-bit number at the start of every Bluetooth LE packet that says which conversation the packet belongs to. All advertising packets share one fixed value."),
    ("AD structure", "One labelled item inside advertising data: a length byte, a type byte, then the data (a name, a flag, a mesh message ...)."),
    ("ADC", "Analog-to-digital converter: the part of a microcontroller that measures a voltage and turns it into a number."),
    ("advertising", "Bluetooth LE's way of broadcasting: a device repeatedly sends a short packet on three fixed channels, for anyone listening."),
    ("AES", "The Advanced Encryption Standard: a recipe that scrambles a 16-byte block with a key so that only someone with the same key can unscramble it."),
    ("amplitude", "How strong a wave is: the height of its peaks."),
    ("antenna", "A piece of metal shaped so that electric current in it sends out radio waves, and radio waves passing by cause current in it."),
    ("AppKey", "Application key: the secret that encrypts what a mesh message means (for example \"lamp on\"). Relays do not need it."),
    ("bandwidth", "How wide a range of frequencies a signal occupies, or a receiver lets in."),
    ("baseband", "A signal described relative to its carrier. Zero means \"exactly the carrier frequency\"."),
    ("beat frequency", "The slow tone you get when two nearby frequencies are mixed: its frequency is the difference between the two."),
    ("bit", "The smallest piece of information: a 0 or a 1."),
    ("bounce", "A mechanical switch does not close cleanly: the contacts chatter open and closed for a few milliseconds."),
    ("byte", "Eight bits. Usually written as two hexadecimal digits, such as 4A."),
    ("carrier", "A steady radio wave of one fixed frequency. On its own it carries no information; a transmitter changes it slightly to send data."),
    ("channel", "One of the fixed frequencies a radio system may use. Bluetooth LE has 40, each 2 MHz apart."),
    ("chirp", "A signal whose frequency sweeps steadily from low to high. FMCW radars send one chirp after another."),
    ("compare value", "A number a timer keeps comparing its counter against. When they match, something happens - for PWM, the output pin switches."),
    ("connection event", "One short meeting of two connected Bluetooth LE devices: they wake up on an agreed channel at an agreed time, exchange packets, and sleep again."),
    ("connection interval", "The time from the start of one connection event to the start of the next."),
    ("counter", "The part of a timer that counts clock ticks: 0, 1, 2 ... up to a limit, then starts again."),
    ("CRC", "Cyclic redundancy check: a few extra bits computed from the data and sent along with it. The receiver computes them again; if they differ, the packet was damaged."),
    ("dB", "Decibel: a way of writing ratios. +10 dB is ten times the power, +3 dB about double, -20 dB a hundredth."),
    ("debounce", "Ignoring the chatter of a switch so that one press counts as one press."),
    ("deviation", "How far a frequency-modulated transmitter moves away from its centre frequency to signal a bit."),
    ("Doppler shift", "The change in frequency of a wave reflected by something that moves: higher if it approaches, lower if it moves away."),
    ("duty cycle", "The share of time a signal is high. 25 % means high for a quarter of each period."),
    ("FFT", "Fast Fourier transform: a calculation that takes a stretch of signal and tells you how much of each frequency is in it."),
    ("flooding", "Delivering a message by having every relay repeat it, so it spreads through the network like a rumour."),
    ("frequency", "How many times per second a wave repeats. Measured in hertz (Hz)."),
    ("GFSK", "Gaussian frequency-shift keying: sending bits by nudging the carrier frequency up or down, with the changes smoothed so the signal stays narrow. Used by Bluetooth LE."),
    ("GPIO", "General-purpose input/output: a pin of a microcontroller that software can read or drive high or low."),
    ("handler", "The function a microcontroller runs when an interrupt arrives. Also called an interrupt service routine (ISR)."),
    ("hertz", "The unit of frequency: repetitions per second. kHz is thousands, MHz millions, GHz billions."),
    ("hop", "In a connection: moving to a different channel for the next meeting. In a mesh: one step from one node to a neighbour."),
    ("interference", "What happens where waves meet: they add. Peaks on peaks give more, peaks on troughs cancel."),
    ("interrupt", "A signal that makes the processor stop what it is doing, run a short handler, and then carry on where it left off."),
    ("IQ", "Two numbers per sample (in-phase and quadrature) that together give a signal's strength and phase. Think of them as the x and y of an arrow."),
    ("IV index", "A 32-bit counter shared by a whole mesh network that goes into every encryption, so sequence numbers can be reused after it is raised."),
    ("latency", "The delay between something being requested and it happening."),
    ("LFSR", "Linear-feedback shift register: a row of bits that shifts along and feeds some of them back in. It produces a fixed scrambled-looking sequence."),
    ("LSB", "Least significant bit: the bit worth 1, the rightmost when a number is written out."),
    ("main loop", "The ordinary code of a firmware that runs round and round when no interrupt handler is running."),
    ("message cache", "A short list a mesh node keeps of messages it has already seen, so it does not act on or relay the same message twice."),
    ("MIC", "Message integrity check: a few bytes that prove a message was made with the right key and has not been changed."),
    ("modulation", "Changing a carrier - its strength, frequency or phase - so that it carries information."),
    ("NetKey", "Network key: the secret that every node of a mesh network shares. It hides and protects who is talking to whom."),
    ("NID", "A 7-bit number derived from the NetKey and sent unencrypted, so a node can quickly tell which key a packet needs."),
    ("noise", "Random unwanted signal that is always present. The weaker the wanted signal, the more the noise spoils it."),
    ("nonce", "A number used once: mixed into each encryption so that the same message never produces the same scrambled bytes twice."),
    ("NVIC", "Nested vectored interrupt controller: the part of an ARM Cortex-M processor that decides which interrupt runs and when."),
    ("obfuscation", "Hiding fields of a mesh packet (who sent it, its sequence number) so an eavesdropper cannot track devices."),
    ("packet", "One self-contained burst of data sent over the air."),
    ("PDU", "Protocol data unit: the meaningful content of a packet at one layer, without the wrapping the lower layers add."),
    ("pending", "An interrupt that has been requested but whose handler has not started yet."),
    ("period", "The time one repetition of a wave or signal takes. It is 1 divided by the frequency."),
    ("phase", "Where in its cycle a wave is, written as an angle from 0 to 360 degrees."),
    ("preamble", "The alternating bits at the very start of a packet that let the receiver settle and find the bit timing."),
    ("preemption", "A more urgent interrupt cutting into a less urgent handler that is already running."),
    ("prescaler", "A divider in front of a timer that slows the clock down, so the counter ticks less often."),
    ("priority", "How urgent an interrupt is. On ARM Cortex-M a lower number means more urgent."),
    ("pull-up", "A resistor that gently holds a pin high, so the pin reads 1 until something (like a pressed button) pulls it low."),
    ("PWM", "Pulse-width modulation: switching a pin on and off quickly and varying the share of on-time, to dim a lamp or set a motor's speed."),
    ("range resolution", "The smallest distance between two objects at which a radar still sees two separate objects."),
    ("relay", "A mesh node that repeats messages it hears, so they reach nodes that are out of the sender's range."),
    ("reload value", "The number at which a timer's counter starts over. It sets how long one period lasts."),
    ("RSSI", "Received signal strength indicator: how strong a packet arrived."),
    ("sample", "One measurement of a signal at one instant."),
    ("scanning", "Listening on the advertising channels for packets."),
    ("sequence number", "A counter a mesh node raises for every message it sends. Receivers refuse numbers they have already seen."),
    ("spectrum", "A picture of a signal sorted by frequency: how much of each frequency it contains."),
    ("stack", "An area of memory where the processor notes what it was doing so it can return to it later."),
    ("timer", "A counter inside the microcontroller driven by a clock. The basis of delays, PWM, and measuring time."),
    ("TTL", "Time to live: a number in a mesh packet that every relay lowers by one. When it runs out, the packet is not relayed any further."),
    ("wavelength", "The distance from one wave peak to the next. Higher frequency means shorter wavelength."),
    ("whitening", "Scrambling the bits of a packet with a fixed pattern so there are no long runs of identical bits, which radios handle badly."),
];

/// Find a definition. Case-insensitive, and forgiving about a plural "s".
pub fn lookup(term: &str) -> Option<&'static str> {
    let t = term.trim().to_lowercase();
    let find = |t: &str| GLOSSARY.iter().find(|(k, _)| k.to_lowercase() == t).map(|(_, d)| *d);
    find(&t).or_else(|| t.strip_suffix('s').and_then(find)).or_else(|| t.strip_suffix("es").and_then(find))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_is_forgiving() {
        assert!(lookup("CRC").is_some());
        assert!(lookup("crc").is_some());
        assert!(lookup("packets").is_some());
        assert!(lookup("Carrier").is_some());
        assert!(lookup("no such thing").is_none());
    }

    #[test]
    fn sorted_and_unique() {
        for w in GLOSSARY.windows(2) {
            assert!(w[0].0.to_lowercase() < w[1].0.to_lowercase(), "{} / {}", w[0].0, w[1].0);
        }
    }
}
