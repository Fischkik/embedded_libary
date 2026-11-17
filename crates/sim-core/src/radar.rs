//! Radar: continuous-wave Doppler and FMCW (frequency-modulated continuous wave).
//!
//! Targets are ideal points. `approach_mps` is the speed towards the radar
//! (negative means moving away).

use crate::dsp::{C64, fft, hann};
use crate::rng::Rng;
use crate::units::{C, db_amp};
use std::f64::consts::{PI, TAU};

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

// ---------------------------------------------------------------------------------
// Angle of arrival with two receive antennas
// ---------------------------------------------------------------------------------

/// Wrap an angle into (-pi, pi]: a phase meter cannot tell 30 degrees from 390.
pub fn wrap_pi(x: f64) -> f64 {
    let y = (x + PI).rem_euclid(TAU) - PI;
    if y <= -PI { y + TAU } else { y }
}

/// Phase difference between two receive antennas `spacing_m` apart for an echo arriving at
/// `theta` from straight ahead: the far antenna's path is longer by `d sin(theta)`, so
/// `dphi = 2 pi d sin(theta) / lambda`. Not wrapped.
pub fn aoa_phase(spacing_m: f64, theta_rad: f64, lambda_m: f64) -> f64 {
    TAU * spacing_m * theta_rad.sin() / lambda_m
}

/// The angle a receiver computes from a measured phase difference: `asin(dphi lambda / (2 pi d))`.
/// `None` when no angle produces that phase (possible only for spacings over lambda / 2).
pub fn aoa_angle(phase_rad: f64, spacing_m: f64, lambda_m: f64) -> Option<f64> {
    let s = phase_rad * lambda_m / (TAU * spacing_m);
    (s.abs() <= 1.0).then(|| s.asin())
}

/// Half-width of the field of view inside which the angle is unambiguous: the measured phase
/// stays within +-pi as long as `|sin(theta)| <= lambda / (2 d)`. 90 degrees for d <= lambda/2.
pub fn aoa_unambiguous_rad(spacing_m: f64, lambda_m: f64) -> f64 {
    (lambda_m / (2.0 * spacing_m)).min(1.0).asin()
}

/// `n` angle estimates of a target at `theta`, each from a phase measurement disturbed by
/// Gaussian noise of `noise_rad` rms.
pub fn aoa_estimates(theta_rad: f64, spacing_m: f64, lambda_m: f64, noise_rad: f64, n: usize, rng: &mut Rng) -> Vec<Option<f64>> {
    (0..n).map(|_| aoa_angle(wrap_pi(aoa_phase(spacing_m, theta_rad, lambda_m) + noise_rad * rng.gauss()), spacing_m, lambda_m)).collect()
}

// ---------------------------------------------------------------------------------
// Presence: a still person's breathing seen in the phase of the echo
// ---------------------------------------------------------------------------------

pub const PRESENCE_FS: f64 = 16.0;
pub const PRESENCE_N: usize = 512;
/// A breathing peak counts as "someone is there" above this level (1 rad amplitude = 0 dB).
pub const PRESENCE_THRESHOLD_DB: f64 = -20.0;
/// Breathing rates searched for, per minute.
pub const BREATHING_BAND_PER_MIN: (f64, f64) = (6.0, 40.0);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Presence {
    pub f0_hz: f64,
    pub person: bool,
    pub breaths_per_min: f64,
    /// Peak-to-peak chest movement from breathing.
    pub chest_mm: f64,
    pub beats_per_min: f64,
    /// Peak-to-peak movement of the chest wall from the heartbeat.
    pub heart_mm: f64,
    /// Receiver noise relative to the echo (amplitude 1), per I and Q.
    pub noise: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PresenceTrace {
    pub t: Vec<f64>,
    pub displacement_mm: Vec<f64>,
    /// Phase of the echo, unwrapped, minus its average.
    pub phase_rad: Vec<f64>,
    /// Spectrum of the phase: `[per minute, dB]`, 1 rad amplitude = 0 dB.
    pub spectrum: Vec<[f64; 2]>,
    /// Strongest peak inside the breathing band.
    pub breathing_peak: [f64; 2],
}

impl Presence {
    /// The echo of one reflecting point (the chest, or the empty chair) is
    /// `exp(-j 4 pi (R + x) / lambda)`: moving by x changes the two-way path by 2x.
    pub fn simulate(&self, seed: u64) -> PresenceTrace {
        let lambda = C / self.f0_hz;
        let mut rng = Rng::new(seed);
        let mut tr = PresenceTrace::default();
        let (mut prev, mut acc) = (0.0, 0.0);
        for i in 0..PRESENCE_N {
            let t = i as f64 / PRESENCE_FS;
            let x_mm = if self.person {
                self.chest_mm / 2.0 * (TAU * self.breaths_per_min / 60.0 * t).sin() + self.heart_mm / 2.0 * (TAU * self.beats_per_min / 60.0 * t).sin()
            } else {
                0.0
            };
            let echo = C64::from_polar(1.0, -2.0 * TAU * x_mm * 1e-3 / lambda) + C64::new(rng.gauss(), rng.gauss()) * self.noise;
            let ph = echo.arg();
            acc += if i == 0 { ph } else { wrap_pi(ph - prev) };
            prev = ph;
            tr.t.push(t);
            tr.displacement_mm.push(x_mm);
            tr.phase_rad.push(acc);
        }
        let mean = tr.phase_rad.iter().sum::<f64>() / PRESENCE_N as f64;
        tr.phase_rad.iter_mut().for_each(|p| *p -= mean);
        tr.spectrum = crate::dsp::spectrum_real(&tr.phase_rad, PRESENCE_FS).into_iter().map(|[f, db]| [f * 60.0, db]).collect();
        let band: Vec<[f64; 2]> = tr.spectrum.iter().copied().filter(|p| (BREATHING_BAND_PER_MIN.0..=BREATHING_BAND_PER_MIN.1).contains(&p[0])).collect();
        tr.breathing_peak = crate::dsp::peak(&band);
        tr
    }
}

// ---------------------------------------------------------------------------------
// Detection thresholds: cell-averaging CFAR
// ---------------------------------------------------------------------------------

/// Threshold factor of cell-averaging CFAR for `n` training cells. With noise power that is
/// exponentially distributed (the output of a square-law detector on Gaussian noise) and the
/// threshold `alpha x mean of the n cells`, the false alarm probability is
/// `(1 + alpha / n)^(-n)`; solved for alpha: `n (Pfa^(-1/n) - 1)`.
pub fn cfar_alpha(n: usize, pfa: f64) -> f64 {
    n as f64 * (pfa.powf(-1.0 / n as f64) - 1.0)
}

/// CA-CFAR threshold for every cell: `train` cells on each side, skipping `guard` cells next
/// to the cell under test. Near the ends only the cells that exist are used, with alpha for
/// their number.
pub fn ca_cfar(power: &[f64], train: usize, guard: usize, pfa: f64) -> Vec<f64> {
    let n = power.len();
    (0..n)
        .map(|i| {
            let lo = i.saturating_sub(guard + train)..i.saturating_sub(guard);
            let hi = (i + guard + 1).min(n)..(i + guard + 1 + train).min(n);
            let cells: Vec<f64> = power[lo].iter().chain(&power[hi]).copied().collect();
            if cells.is_empty() {
                return f64::INFINITY;
            }
            cfar_alpha(cells.len(), pfa) * cells.iter().sum::<f64>() / cells.len() as f64
        })
        .collect()
}

/// A range profile: received power per range cell, noise power 1 on average, an area of
/// extra clutter, and point targets given as (cell, signal-to-noise ratio in dB).
#[derive(Clone, Debug, PartialEq)]
pub struct CfarScene {
    pub cells: usize,
    pub clutter_db: f64,
    pub clutter_cells: (usize, usize),
    pub targets: Vec<(usize, f64)>,
}

impl CfarScene {
    pub fn profile(&self, seed: u64) -> Vec<f64> {
        let mut rng = Rng::new(seed);
        let clutter = 10f64.powf(self.clutter_db / 10.0);
        (0..self.cells)
            .map(|i| {
                let floor = if (self.clutter_cells.0..self.clutter_cells.1).contains(&i) { 1.0 + clutter } else { 1.0 };
                let mut v = C64::new(rng.gauss(), rng.gauss()) * (floor / 2.0).sqrt();
                for &(cell, snr_db) in &self.targets {
                    if cell == i {
                        v += C64::from_polar(10f64.powf(snr_db / 20.0), TAU * rng.f64());
                    }
                }
                v.norm_sqr()
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------------
// Radar versus PIR: a CW Doppler motion sensor watching the same walk as the PIR model
// ---------------------------------------------------------------------------------

/// The motion sensor's filter removes Doppler frequencies below this (the echo of
/// everything that stands still is at 0 Hz).
pub const MOTION_HIGHPASS_HZ: f64 = 5.0;
/// Distance at which a walking person gives motion level 1.
pub const MOTION_REF_M: f64 = 5.0;
pub const MOTION_THRESHOLD: f64 = 0.15;

/// Per time step of the PIR model: `[t, Doppler frequency, motion level]`. The echo's
/// amplitude falls with the square of the distance (its power with the fourth power) and is
/// multiplied by `transmission` (amplitude) for an obstacle in the way; the high-pass then
/// weakens slow Doppler frequencies. Only the speed towards or away from the sensor counts.
pub fn doppler_motion(scene: &crate::signals::PirScene, f0_hz: f64, transmission: f64) -> Vec<[f64; 3]> {
    use crate::signals::{PIR_RATE_HZ, PIR_SECONDS};
    let (dx, dy) = (scene.heading_deg.to_radians().cos(), scene.heading_deg.to_radians().sin());
    (0..(PIR_SECONDS * PIR_RATE_HZ) as usize)
        .map(|i| {
            let t = i as f64 / PIR_RATE_HZ;
            let (x, y) = (scene.start.0 + dx * scene.speed_mps * t, scene.start.1 + dy * scene.speed_mps * t);
            let r = (x * x + y * y).sqrt().max(0.5);
            let approach = -(x * dx + y * dy) * scene.speed_mps / r;
            let f = doppler_hz(approach, f0_hz);
            let level = transmission * (MOTION_REF_M / r).powi(2) * f.abs() / (f * f + MOTION_HIGHPASS_HZ * MOTION_HIGHPASS_HZ).sqrt();
            [t, f, level]
        })
        .collect()
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
    #[test]
    fn angle_of_arrival() {
        let lambda = C / 24.125e9;
        // Half a wavelength apart, 30 degrees: a quarter turn of phase.
        assert!((aoa_phase(lambda / 2.0, 30f64.to_radians(), lambda) - PI / 2.0).abs() < 1e-12);
        // Up to lambda/2 every angle comes back exactly.
        for deg in (-89..=89).step_by(7) {
            let th = (deg as f64).to_radians();
            let back = aoa_angle(wrap_pi(aoa_phase(lambda / 2.0, th, lambda)), lambda / 2.0, lambda).unwrap();
            assert!((back - th).abs() < 1e-9, "{deg}");
        }
        // One wavelength apart: unambiguous only to +-30 degrees; 40 degrees reads as -20.93.
        assert!((aoa_unambiguous_rad(lambda, lambda).to_degrees() - 30.0).abs() < 1e-9);
        let ghost = aoa_angle(wrap_pi(aoa_phase(lambda, 40f64.to_radians(), lambda)), lambda, lambda).unwrap().to_degrees();
        assert!((ghost + 20.93).abs() < 0.01, "{ghost}");
        // Wider spacing: the same phase noise gives a smaller angle error.
        let spread = |d: f64| {
            let e = aoa_estimates(0.0, d, lambda, 0.1, 400, &mut Rng::new(3));
            (e.iter().map(|a| a.unwrap().powi(2)).sum::<f64>() / 400.0).sqrt()
        };
        assert!(spread(lambda) < 0.6 * spread(lambda / 2.0));
        assert_eq!(wrap_pi(3.0 * PI), PI);
    }

    #[test]
    fn breathing_shows_in_the_phase() {
        let p = Presence { f0_hz: 24.125e9, person: true, breaths_per_min: 15.0, chest_mm: 4.0, beats_per_min: 72.0, heart_mm: 0.0, noise: 0.02 };
        let tr = p.simulate(1);
        // 4 mm peak to peak at 24 GHz: 4 pi x 4 mm / 12.4 mm = 4.04 rad peak to peak.
        let (lo, hi) = tr.phase_rad.iter().fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
        assert!((hi - lo - 4.04).abs() < 0.15, "{}", hi - lo);
        assert!((tr.breathing_peak[0] - 15.0).abs() < 1.0, "{:?}", tr.breathing_peak);
        assert!(tr.breathing_peak[1] > PRESENCE_THRESHOLD_DB);
        // The empty room: nothing in the breathing band.
        let empty = Presence { person: false, ..p }.simulate(1);
        assert!(empty.breathing_peak[1] < PRESENCE_THRESHOLD_DB, "{:?}", empty.breathing_peak);
        // The heartbeat adds a second, smaller peak.
        let heart = Presence { heart_mm: 0.4, ..p }.simulate(1);
        let at = |tr: &PresenceTrace, f: f64| tr.spectrum.iter().filter(|q| (q[0] - f).abs() < 1.5).fold(f64::MIN, |m, q| m.max(q[1]));
        assert!(at(&heart, 72.0) > at(&tr, 72.0) + 20.0);
    }

    #[test]
    fn cfar_keeps_its_false_alarm_rate() {
        assert!((cfar_alpha(16, 1e-4) - 12.45).abs() < 0.01);
        assert!((cfar_alpha(20, 1e-3) - 8.25).abs() < 0.01);
        // Pure noise, one target-free profile of 200 000 cells: measured rate near Pfa.
        let scene = CfarScene { cells: 200_000, clutter_db: 0.0, clutter_cells: (0, 0), targets: vec![] };
        let p = scene.profile(7);
        let th = ca_cfar(&p, 8, 2, 1e-2);
        let rate = p.iter().zip(&th).filter(|(v, t)| v > t).count() as f64 / p.len() as f64;
        assert!((rate / 1e-2 - 1.0).abs() < 0.1, "{rate}");
        // A clutter area 15 dB up: a fixed threshold set for Pfa 1e-2 on plain noise
        // (ln 100 = 4.6) fires everywhere in it; CFAR does not.
        let cl = CfarScene { cells: 2000, clutter_db: 15.0, clutter_cells: (500, 1500), targets: vec![] };
        let p = cl.profile(9);
        let fixed = p[600..1400].iter().filter(|v| **v > 100f64.ln()).count();
        let th = ca_cfar(&p, 8, 2, 1e-2);
        let adaptive = (600..1400).filter(|&i| p[i] > th[i]).count();
        assert!(fixed > 500 && adaptive < 25, "{fixed} {adaptive}");
    }

    #[test]
    fn doppler_motion_sees_approach_best() {
        use crate::signals::PirScene;
        let walk = PirScene { start: (-4.0, 5.0), heading_deg: 0.0, speed_mps: 1.0, zone_pitch_deg: 6.0, draught: 0.0, single_element: false };
        let towards = PirScene { start: (0.0, 9.0), heading_deg: -90.0, speed_mps: 0.6, ..walk };
        let m = doppler_motion(&towards, 24.125e9, 1.0);
        // Straight towards the sensor: the full speed is radial, 0.6 m/s -> 96.6 Hz.
        assert!((m[0][1] - doppler_hz(0.6, 24.125e9)).abs() < 1e-9);
        assert!(m.iter().all(|s| s[2] > MOTION_THRESHOLD));
        // Across: no radial speed straight in front of the sensor.
        let across = doppler_motion(&walk, 24.125e9, 1.0);
        let mid = across.iter().min_by(|a, b| a[1].abs().total_cmp(&b[1].abs())).unwrap();
        assert!(mid[2] < MOTION_THRESHOLD, "{mid:?}");
    }

}
