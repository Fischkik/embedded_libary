//! Putting bits on a radio wave and getting them back: OOK, FSK and GFSK.
//!
//! Everything works at "baseband": the signal is described relative to the carrier,
//! as IQ samples. Frequency +250 kHz at baseband means "carrier + 250 kHz" on air.

use crate::dsp::{C64, filter_same, gaussian_taps};
use crate::rng::Rng;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scheme {
    /// On-off keying: carrier on for 1, off for 0.
    Ook,
    /// Frequency-shift keying: carrier nudged up for 1, down for 0, switching abruptly.
    Fsk,
    /// FSK with the switching smoothed by a Gaussian filter (what Bluetooth LE uses).
    Gfsk,
}

/// Bits as a square wave: `+1` for a 1 and `-1` for a 0, `sps` samples per bit.
pub fn nrz(bits: &[bool], sps: usize) -> Vec<f64> {
    bits.iter().flat_map(|&b| std::iter::repeat_n(if b { 1.0 } else { -1.0 }, sps)).collect()
}

/// The "frequency command" of the transmitter, between -1 and +1.
/// For FSK it is the square wave itself; for GFSK it is the Gaussian-smoothed version.
pub fn frequency_shape(bits: &[bool], scheme: Scheme, sps: usize, bt: f64) -> Vec<f64> {
    let square = nrz(bits, sps);
    match scheme {
        Scheme::Gfsk => filter_same(&square, &gaussian_taps(bt, sps, 4)),
        _ => square,
    }
}

/// Turn a frequency command into IQ samples by accumulating phase (an FM modulator).
/// `deviation_hz` is the frequency offset for a command of 1.0.
pub fn fm_modulate(shape: &[f64], deviation_hz: f64, fs: f64) -> Vec<C64> {
    let mut phase = 0.0f64;
    shape
        .iter()
        .map(|s| {
            phase += std::f64::consts::TAU * deviation_hz * s / fs;
            C64::from_polar(1.0, phase)
        })
        .collect()
}

/// Modulate bits to IQ samples.
pub fn modulate(bits: &[bool], scheme: Scheme, sps: usize, deviation_hz: f64, fs: f64, bt: f64) -> Vec<C64> {
    match scheme {
        Scheme::Ook => bits
            .iter()
            .flat_map(|&b| std::iter::repeat_n(C64::new(if b { 1.0 } else { 0.0 }, 0.0), sps))
            .collect(),
        _ => fm_modulate(&frequency_shape(bits, scheme, sps, bt), deviation_hz, fs),
    }
}

/// Add white Gaussian noise for a given signal-to-noise ratio (signal power is 1).
pub fn add_noise(iq: &mut [C64], snr_db: f64, rng: &mut Rng) {
    let sigma = (10f64.powf(-snr_db / 10.0) / 2.0).sqrt();
    for s in iq {
        *s += C64::new(rng.gauss() * sigma, rng.gauss() * sigma);
    }
}

/// FM demodulator: how fast the phase turns between neighbouring samples, in Hz.
pub fn discriminate(iq: &[C64], fs: f64) -> Vec<f64> {
    let mut out = Vec::with_capacity(iq.len());
    let mut prev = iq.first().copied().unwrap_or(C64::new(1.0, 0.0));
    for &s in iq {
        out.push((s * prev.conj()).arg() * fs / std::f64::consts::TAU);
        prev = s;
    }
    out
}

/// Simple moving average over `len` samples (a crude receive filter).
pub fn moving_average(x: &[f64], len: usize) -> Vec<f64> {
    let len = len.max(1);
    let mut acc = 0.0;
    let mut out = Vec::with_capacity(x.len());
    for i in 0..x.len() {
        acc += x[i];
        if i >= len {
            acc -= x[i - len];
        }
        out.push(acc / len.min(i + 1) as f64);
    }
    out
}

/// Recover bits from IQ samples. Returns the decided bits and the per-sample
/// "decision signal" the receiver looked at (frequency in Hz, or amplitude for OOK).
pub fn demodulate(iq: &[C64], scheme: Scheme, sps: usize, fs: f64) -> (Vec<bool>, Vec<f64>) {
    let avg = (sps / 2).max(1);
    let (signal, threshold) = match scheme {
        Scheme::Ook => (moving_average(&iq.iter().map(|s| s.norm()).collect::<Vec<_>>(), avg), 0.5),
        _ => (moving_average(&discriminate(iq, fs), avg), 0.0),
    };
    // The moving average delays the signal by about avg/2 samples; sample each bit
    // a little after its middle to compensate.
    let offset = sps / 2 + avg / 2;
    let bits = (0..iq.len() / sps.max(1))
        .map(|k| signal[(k * sps + offset).min(signal.len() - 1)] > threshold)
        .collect();
    (bits, signal)
}

/// Count positions where two bit strings differ.
pub fn bit_errors(a: &[bool], b: &[bool]) -> usize {
    a.iter().zip(b).filter(|(x, y)| x != y).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pattern() -> Vec<bool> {
        let mut r = Rng::new(3);
        (0..200).map(|_| r.bool()).collect()
    }

    #[test]
    fn clean_round_trip_for_every_scheme() {
        let bits = pattern();
        for scheme in [Scheme::Ook, Scheme::Fsk, Scheme::Gfsk] {
            let iq = modulate(&bits, scheme, 16, 250e3, 16e6, 0.5);
            let (rx, _) = demodulate(&iq, scheme, 16, 16e6);
            assert_eq!(bit_errors(&bits, &rx), 0, "{scheme:?}");
        }
    }

    #[test]
    fn gfsk_deviation_matches_ble() {
        // A long run of ones must settle at exactly +deviation: 250 kHz for h = 0.5 at 1 Mbit/s.
        let bits = vec![true; 16];
        let iq = modulate(&bits, Scheme::Gfsk, 16, 250e3, 16e6, 0.5);
        let f = discriminate(&iq, 16e6);
        assert!((f[8 * 16] - 250e3).abs() < 1.0, "{}", f[8 * 16]);
    }

    #[test]
    fn heavy_noise_causes_errors() {
        let bits = pattern();
        let mut iq = modulate(&bits, Scheme::Gfsk, 16, 250e3, 16e6, 0.5);
        add_noise(&mut iq, -8.0, &mut Rng::new(1));
        let (rx, _) = demodulate(&iq, Scheme::Gfsk, 16, 16e6);
        assert!(bit_errors(&bits, &rx) > 5);
    }
}
