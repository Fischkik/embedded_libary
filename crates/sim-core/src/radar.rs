//! Radar: continuous-wave Doppler and FMCW (frequency-modulated continuous wave).
//!
//! Targets are ideal points. `approach_mps` is the speed towards the radar
//! (negative means moving away).

use crate::dsp::{C64, fft, hann};
use crate::rng::Rng;
use crate::units::{C, db_amp};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    pub range_m: f64,
    pub approach_mps: f64,
    /// Strength of the echo at the receiver (1.0 = reference).
    pub amplitude: f64,
}

/// Doppler shift of an echo: `f_d = 2 * v * f0 / c`. Positive for an approaching target.
pub fn doppler_hz(approach_mps: f64, f0_hz: f64) -> f64 {
    2.0 * approach_mps * f0_hz / C
}

/// Output of a CW radar's IQ mixer: one rotating arrow per target. The arrow turns
/// once for every half wavelength the target moves.
pub fn cw_iq(targets: &[Target], f0_hz: f64, fs: f64, n: usize, t0: f64, noise: f64, rng: &mut Rng) -> Vec<C64> {
    let lambda = C / f0_hz;
    (0..n)
        .map(|i| {
            let t = t0 + i as f64 / fs;
            let mut s = C64::new(rng.gauss() * noise, rng.gauss() * noise);
            for tg in targets {
                let r = tg.range_m - tg.approach_mps * t;
                s += C64::from_polar(tg.amplitude, -2.0 * std::f64::consts::TAU * r / lambda);
            }
            s
        })
        .collect()
}

/// FMCW radar settings. One chirp sweeps the frequency up by `bandwidth_hz` in
/// `chirp_s` seconds; chirps follow each other without a pause.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fmcw {
    pub f0_hz: f64,
    pub bandwidth_hz: f64,
    pub chirp_s: f64,
    /// Sample rate of the (complex) receiver output.
    pub fs: f64,
}

/// A range-Doppler map: `data[doppler * n_range + range]` in dB.
/// Doppler row 0 is the fastest receding speed, the last row the fastest approaching.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RangeDoppler {
    pub data: Vec<f32>,
    pub n_range: usize,
    pub n_doppler: usize,
}

impl Fmcw {
    pub fn samples(&self) -> usize {
        ((self.chirp_s * self.fs).round() as usize).max(8)
    }

    /// How fast the frequency climbs, Hz per second.
    pub fn slope(&self) -> f64 {
        self.bandwidth_hz / self.chirp_s
    }

    pub fn wavelength(&self) -> f64 {
        C / self.f0_hz
    }

    /// Beat frequency for a target at `range_m`: `f_b = 2 * R * S / c`.
    pub fn beat_hz(&self, range_m: f64) -> f64 {
        2.0 * range_m * self.slope() / C
    }

    /// Smallest distance between two targets that still shows two peaks: `c / (2 B)`.
    pub fn range_resolution(&self) -> f64 {
        C / (2.0 * self.bandwidth_hz)
    }

    /// Farthest range whose beat frequency the receiver can still sample: `fs * c / (2 S)`.
    pub fn max_range(&self) -> f64 {
        self.fs * C / (2.0 * self.slope())
    }

    /// Fastest speed that is measured without ambiguity: `lambda / (4 T_c)`.
    pub fn max_velocity(&self) -> f64 {
        self.wavelength() / (4.0 * self.chirp_s)
    }

    /// Smallest speed difference that can be told apart with `n_chirps`: `lambda / (2 N T_c)`.
    pub fn velocity_resolution(&self, n_chirps: usize) -> f64 {
        self.wavelength() / (2.0 * n_chirps.max(1) as f64 * self.chirp_s)
    }

    /// Range that FFT bin `k` of one chirp stands for.
    pub fn range_of_bin(&self, k: usize) -> f64 {
        k as f64 * self.max_range() / self.samples() as f64
    }

    /// Receiver output for chirp number `chirp`: transmitted chirp mixed with the echoes.
    /// Every target becomes one tone; its frequency encodes the range.
    pub fn beat(&self, targets: &[Target], chirp: usize, noise: f64, rng: &mut Rng) -> Vec<C64> {
        let slope = self.slope();
        (0..self.samples())
            .map(|i| {
                let t = i as f64 / self.fs;
                let mut s = C64::new(rng.gauss() * noise, rng.gauss() * noise);
                for tg in targets {
                    let r = tg.range_m - tg.approach_mps * (chirp as f64 * self.chirp_s + t);
                    let tau = 2.0 * r / C;
                    s += C64::from_polar(tg.amplitude, std::f64::consts::TAU * (self.f0_hz * tau + slope * tau * t));
                }
                s
            })
            .collect()
    }

    /// FFT of one chirp: level in dB for each range bin.
    pub fn range_profile(&self, beat: &[C64]) -> Vec<f64> {
        let w = hann(beat.len());
        let wsum: f64 = w.iter().sum();
        let mut buf: Vec<C64> = beat.iter().zip(&w).map(|(s, w)| s * *w).collect();
        fft(&mut buf);
        buf.iter().map(|c| db_amp(c.norm() / wsum)).collect()
    }

    /// Approach speed that Doppler row `row` of a map with `n_doppler` rows stands for.
    pub fn velocity_of_row(&self, row: usize, n_doppler: usize) -> f64 {
        (row as f64 - (n_doppler / 2) as f64 + 1.0) * 2.0 * self.max_velocity() / n_doppler as f64
    }

    /// Process `n_chirps` chirps into a range-Doppler map: one FFT along each chirp
    /// (range), then one FFT across the chirps for every range bin (speed).
    pub fn range_doppler(&self, targets: &[Target], n_chirps: usize, noise: f64, rng: &mut Rng) -> RangeDoppler {
        let n_range = self.samples();
        let wr = hann(n_range);
        let wd = hann(n_chirps);
        let norm = wr.iter().sum::<f64>() * wd.iter().sum::<f64>();
        let mut rows: Vec<Vec<C64>> = (0..n_chirps)
            .map(|k| {
                let mut buf: Vec<C64> = self.beat(targets, k, noise, rng).iter().zip(&wr).map(|(s, w)| s * *w * wd[k]).collect();
                fft(&mut buf);
                buf
            })
            .collect();
        let mut data = vec![-200f32; n_range * n_chirps];
        let mut col = vec![C64::new(0.0, 0.0); n_chirps];
        for r in 0..n_range {
            for (k, row) in rows.iter_mut().enumerate() {
                col[k] = row[r];
            }
            fft(&mut col);
            for (bin, c) in col.iter().enumerate() {
                // An approaching target makes the phase fall from chirp to chirp, which is a
                // negative FFT frequency. Flip so that "approaching" ends up at the top rows.
                let signed = if bin < n_chirps.div_ceil(2) { bin as isize } else { bin as isize - n_chirps as isize };
                let row = (n_chirps / 2) as isize - 1 - signed;
                if (0..n_chirps as isize).contains(&row) {
                    data[row as usize * n_range + r] = db_amp(c.norm() / norm) as f32;
                }
            }
        }
        RangeDoppler { data, n_range, n_doppler: n_chirps }
    }
}

/// Index of the largest value.
pub fn argmax<T: PartialOrd + Copy>(v: &[T]) -> usize {
    let mut best = 0;
    for i in 1..v.len() {
        if v[i] > v[best] {
            best = i;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::{peak, spectrum_iq};

    #[test]
    fn doppler_formula_matches_hb100_datasheet() {
        // HB100 datasheet: 19.49 Hz per km/h at 10.525 GHz.
        let per_kmh = doppler_hz(1.0 / 3.6, 10.525e9);
        assert!((per_kmh - 19.49).abs() < 0.02, "{per_kmh}");
    }

    #[test]
    fn cw_spectrum_peaks_at_the_doppler_shift_with_the_right_sign() {
        let f0 = 24.125e9;
        for v in [1.5, -0.8] {
            let iq = cw_iq(&[Target { range_m: 3.0, approach_mps: v, amplitude: 1.0 }], f0, 2000.0, 2000, 0.0, 0.0, &mut Rng::new(1));
            let p = peak(&spectrum_iq(&iq, 2000.0));
            assert!((p[0] - doppler_hz(v, f0)).abs() < 1.5, "v={v}: {p:?}");
        }
    }

    fn radar() -> Fmcw {
        Fmcw { f0_hz: 60e9, bandwidth_hz: 1e9, chirp_s: 128e-6, fs: 1e6 }
    }

    #[test]
    fn textbook_numbers() {
        let r = radar();
        assert!((r.range_resolution() - 0.1499).abs() < 1e-3);
        assert!((Fmcw { bandwidth_hz: 4e9, ..r }.range_resolution() - 0.0375).abs() < 1e-4); // TI: 4 GHz -> 3.75 cm
        assert!((r.max_range() - 19.19).abs() < 0.05);
        assert!((r.max_velocity() - 9.76).abs() < 0.05);
    }

    #[test]
    fn range_profile_peaks_at_the_target() {
        let r = radar();
        for range in [2.0, 7.3, 12.0] {
            let beat = r.beat(&[Target { range_m: range, approach_mps: 0.0, amplitude: 1.0 }], 0, 0.0, &mut Rng::new(1));
            let k = argmax(&r.range_profile(&beat));
            assert!((r.range_of_bin(k) - range).abs() <= r.range_resolution(), "{range}: bin {k}");
        }
    }

    #[test]
    fn range_doppler_peak_sits_at_range_and_speed() {
        let r = radar();
        for v in [3.0, -4.5] {
            let map = r.range_doppler(&[Target { range_m: 6.0, approach_mps: v, amplitude: 1.0 }], 64, 0.0, &mut Rng::new(1));
            let i = argmax(&map.data);
            let (row, col) = (i / map.n_range, i % map.n_range);
            assert!((r.range_of_bin(col) - 6.0).abs() <= 2.0 * r.range_resolution(), "col {col}");
            let got = r.velocity_of_row(row, 64);
            assert!((got - v).abs() <= 1.5 * r.velocity_resolution(64), "v={v} got {got}");
        }
    }
}
