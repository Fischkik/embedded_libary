//! Basic signal processing: FFT, windows, spectra.

use rustfft::FftPlanner;
pub use rustfft::num_complex::Complex;

pub type C64 = Complex<f64>;

/// In-place forward FFT.
pub fn fft(buf: &mut [C64]) {
    if buf.is_empty() {
        return;
    }
    FftPlanner::new().plan_fft_forward(buf.len()).process(buf);
}

/// Hann window: fades the ends of a block to zero so the FFT does not see a jump.
pub fn hann(n: usize) -> Vec<f64> {
    (0..n)
        .map(|i| 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n.max(1) as f64).cos())
        .collect()
}

/// One-sided spectrum of a real signal as `[frequency_hz, level_db]` points.
/// A sine of amplitude 1 reads 0 dB.
pub fn spectrum_real(x: &[f64], fs: f64) -> Vec<[f64; 2]> {
    let n = x.len();
    if n < 2 {
        return Vec::new();
    }
    let w = hann(n);
    let wsum: f64 = w.iter().sum();
    let mut buf: Vec<C64> = x.iter().zip(&w).map(|(v, w)| C64::new(v * w, 0.0)).collect();
    fft(&mut buf);
    (0..=n / 2)
        .map(|k| [k as f64 * fs / n as f64, crate::units::db_amp(2.0 * buf[k].norm() / wsum)])
        .collect()
}

/// Two-sided spectrum of a complex (IQ) signal, from `-fs/2` to `+fs/2`.
/// A complex tone of amplitude 1 reads 0 dB.
pub fn spectrum_iq(x: &[C64], fs: f64) -> Vec<[f64; 2]> {
    let n = x.len();
    if n < 2 {
        return Vec::new();
    }
    let w = hann(n);
    let wsum: f64 = w.iter().sum();
    let mut buf: Vec<C64> = x.iter().zip(&w).map(|(v, w)| v * *w).collect();
    fft(&mut buf);
    (0..n)
        .map(|i| {
            let k = (i + n / 2) % n; // fftshift
            let f = (i as f64 - (n / 2) as f64) * fs / n as f64;
            [f, crate::units::db_amp(buf[k].norm() / wsum)]
        })
        .collect()
}

/// The strongest point of a spectrum.
pub fn peak(spec: &[[f64; 2]]) -> [f64; 2] {
    spec.iter().copied().fold([0.0, f64::NEG_INFINITY], |best, p| if p[1] > best[1] { p } else { best })
}

/// Width in Hz of the band that holds `fraction` (e.g. 0.99) of the power.
pub fn occupied_bandwidth(spec: &[[f64; 2]], fraction: f64) -> f64 {
    let p: Vec<f64> = spec.iter().map(|s| 10f64.powf(s[1] / 10.0)).collect();
    let total: f64 = p.iter().sum();
    if total <= 0.0 || spec.len() < 2 {
        return 0.0;
    }
    let tail = total * (1.0 - fraction) / 2.0;
    let mut acc = 0.0;
    let mut lo = 0;
    for (i, v) in p.iter().enumerate() {
        acc += v;
        if acc >= tail {
            lo = i;
            break;
        }
    }
    acc = 0.0;
    let mut hi = spec.len() - 1;
    for (i, v) in p.iter().enumerate().rev() {
        acc += v;
        if acc >= tail {
            hi = i;
            break;
        }
    }
    (spec[hi][0] - spec[lo][0]).max(0.0)
}

/// Taps of a Gaussian low-pass filter, normalised to sum to 1.
/// `bt` is bandwidth x symbol time (0.5 for Bluetooth LE), `sps` samples per symbol.
pub fn gaussian_taps(bt: f64, sps: usize, span_symbols: usize) -> Vec<f64> {
    let half = (span_symbols * sps) as isize / 2;
    let k = 2.0 * std::f64::consts::PI.powi(2) * bt * bt / std::f64::consts::LN_2;
    let mut taps: Vec<f64> = (-half..=half)
        .map(|i| {
            let t = i as f64 / sps as f64; // in symbol periods
            (-k * t * t).exp()
        })
        .collect();
    let sum: f64 = taps.iter().sum();
    taps.iter_mut().for_each(|t| *t /= sum);
    taps
}

/// Convolve and keep the output aligned with the input (same length).
pub fn filter_same(x: &[f64], taps: &[f64]) -> Vec<f64> {
    let half = taps.len() as isize / 2;
    (0..x.len() as isize)
        .map(|n| {
            taps.iter()
                .enumerate()
                .map(|(k, t)| {
                    let idx = (n + half - k as isize).clamp(0, x.len() as isize - 1);
                    t * x[idx as usize]
                })
                .sum()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sine_peak_is_at_its_frequency_and_0_db() {
        let fs = 1000.0;
        let x: Vec<f64> = (0..1000).map(|n| (std::f64::consts::TAU * 100.0 * n as f64 / fs).sin()).collect();
        let p = peak(&spectrum_real(&x, fs));
        assert!((p[0] - 100.0).abs() < 1.01, "{p:?}");
        assert!(p[1].abs() < 0.1, "{p:?}");
    }

    #[test]
    fn complex_tone_has_a_sign() {
        let fs = 1000.0;
        let x: Vec<C64> = (0..1000).map(|n| C64::from_polar(1.0, -std::f64::consts::TAU * 50.0 * n as f64 / fs)).collect();
        let p = peak(&spectrum_iq(&x, fs));
        assert!((p[0] + 50.0).abs() < 1.01, "{p:?}");
        assert!(p[1].abs() < 0.1);
    }

    #[test]
    fn gaussian_taps_sum_to_one_and_are_symmetric() {
        let t = gaussian_taps(0.5, 8, 4);
        assert!((t.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        assert!((t[0] - t[t.len() - 1]).abs() < 1e-15);
    }
}
