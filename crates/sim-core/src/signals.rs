//! Signals and the analog world around a microcontroller: sampling, noise, filters,
//! and the circuits that bring a sensor's signal to the converter.

use crate::rng::Rng;
use std::f64::consts::TAU;

// ---------------------------------------------------------------------------------
// Sampling and aliasing
// ---------------------------------------------------------------------------------

/// The frequency a sine of `f_hz` appears to have after sampling at `fs_hz`: the
/// distance from `f_hz` to the nearest multiple of the sample rate. Below half the
/// sample rate (the Nyquist frequency) it is the true frequency.
pub fn alias_frequency(f_hz: f64, fs_hz: f64) -> f64 {
    (f_hz - (f_hz / fs_hz).round() * fs_hz).abs()
}

/// The same with its sign: negative means the sampled wave looks mirrored in time.
pub fn alias_signed(f_hz: f64, fs_hz: f64) -> f64 {
    f_hz - (f_hz / fs_hz).round() * fs_hz
}

// ---------------------------------------------------------------------------------
// Noise and averaging
// ---------------------------------------------------------------------------------

pub fn mean(x: &[f64]) -> f64 {
    x.iter().sum::<f64>() / x.len().max(1) as f64
}

/// Standard deviation: the typical distance of the values from their mean.
pub fn std_dev(x: &[f64]) -> f64 {
    let m = mean(x);
    (x.iter().map(|v| (v - m).powi(2)).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NoisyReadings {
    pub truth: Vec<f64>,
    /// One reading per sample: the truth plus random noise.
    pub raw: Vec<f64>,
    /// The mean of the last `n` readings at each sample (fewer at the very start).
    pub averaged: Vec<f64>,
}

/// `count` readings of a value that is `before` and jumps to `after` at sample
/// `step_at`, each with Gaussian noise of standard deviation `sigma`, and their
/// moving average over `n` readings.
pub fn noisy_readings(count: usize, before: f64, after: f64, step_at: usize, sigma: f64, n: usize, seed: u64) -> NoisyReadings {
    let mut rng = Rng::new(seed);
    let truth: Vec<f64> = (0..count).map(|i| if i < step_at { before } else { after }).collect();
    let raw: Vec<f64> = truth.iter().map(|t| t + sigma * rng.gauss()).collect();
    let n = n.max(1);
    let mut sum = 0.0;
    let averaged = (0..count)
        .map(|i| {
            sum += raw[i];
            if i >= n {
                sum -= raw[i - n];
            }
            sum / (i + 1).min(n) as f64
        })
        .collect();
    NoisyReadings { truth, raw, averaged }
}

// ---------------------------------------------------------------------------------
// The simplest digital filter: a first-order low-pass
// ---------------------------------------------------------------------------------

/// The smoothing factor that gives a cutoff frequency of `fc_hz` at a sample rate of
/// `fs_hz`: `alpha = 1 - e^(-2 pi fc / fs)`.
pub fn lowpass_alpha(fc_hz: f64, fs_hz: f64) -> f64 {
    1.0 - (-TAU * fc_hz / fs_hz).exp()
}

/// `y = y + alpha x (x - y)` for every sample: each output moves a fraction `alpha`
/// of the way from the previous output to the new input.
pub fn lowpass(x: &[f64], alpha: f64) -> Vec<f64> {
    let mut y = x.first().copied().unwrap_or(0.0);
    x.iter()
        .map(|&v| {
            y += alpha * (v - y);
            y
        })
        .collect()
}

/// How much of a sine of `f_hz` gets through that filter: output amplitude divided by
/// input amplitude.
pub fn lowpass_gain(f_hz: f64, fs_hz: f64, alpha: f64) -> f64 {
    let b = 1.0 - alpha;
    alpha / (1.0 - 2.0 * b * (TAU * f_hz / fs_hz).cos() + b * b).sqrt()
}

// ---------------------------------------------------------------------------------
// PLL: a feedback loop that makes one frequency an exact multiple of another
// ---------------------------------------------------------------------------------

/// A phase-locked loop: a reference frequency, an oscillator whose frequency follows a
/// control value, a divider by N on the way back, and a loop filter between them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pll {
    pub f_ref_hz: f64,
    /// The range the oscillator can be tuned over.
    pub vco_min_hz: f64,
    pub vco_max_hz: f64,
    /// How fast the loop corrects: its natural frequency, Hz.
    pub loop_hz: f64,
    /// How well the correction is damped. Around 0.7 settles fastest; low values ring.
    pub damping: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PllTrace {
    /// Microseconds since the divider was changed.
    pub t_us: Vec<f64>,
    pub f_vco_hz: Vec<f64>,
    /// Phase difference at the phase detector, in turns of the reference (1 = 360 degrees).
    pub phase_error_turns: Vec<f64>,
    pub target_hz: f64,
    /// When the output came within `tolerance_hz` of the target for good.
    pub lock_time_us: Option<f64>,
    /// How far the output went past the target on the way, Hz.
    pub overshoot_hz: f64,
}

impl Pll {
    /// The loop is locked with the divider at `n_from`; at time 0 the divider changes to
    /// `n_to`. The phase detector's output is proportional to the phase difference
    /// between the reference and the divided oscillator; the loop filter adds a share of
    /// that error and a share of its running sum; the result tunes the oscillator.
    pub fn hop(&self, n_from: u32, n_to: u32, duration_us: f64, tolerance_hz: f64) -> PllTrace {
        const STEPS: usize = 20_000;
        let dt = duration_us * 1e-6 / STEPS as f64;
        let n = n_to.max(1) as f64;
        let wn = TAU * self.loop_hz;
        // Work in "hertz of oscillator" for the control value: f_vco = centre + control.
        // de/dt = 2 pi (f_ref - f_vco / n);  control = kp e + ki sum(e)
        let (kp, ki) = (2.0 * self.damping * wn * n / TAU, wn * wn * n / TAU);
        let centre = (self.vco_min_hz + self.vco_max_hz) / 2.0;
        let start = (n_from as f64 * self.f_ref_hz).clamp(self.vco_min_hz, self.vco_max_hz);
        let target = n * self.f_ref_hz;
        let (mut e, mut sum) = (0.0f64, (start - centre) / ki);
        let mut tr = PllTrace { target_hz: target, ..Default::default() };
        let mut outside_until = 0.0;
        let rising = target >= start;
        for k in 0..=STEPS {
            let f = (centre + kp * e + ki * sum).clamp(self.vco_min_hz, self.vco_max_hz);
            let t_us = k as f64 * dt * 1e6;
            if (f - target).abs() > tolerance_hz {
                outside_until = t_us;
            }
            let past = if rising { f - target } else { target - f };
            tr.overshoot_hz = tr.overshoot_hz.max(past);
            if k % 25 == 0 {
                tr.t_us.push(t_us);
                tr.f_vco_hz.push(f);
                tr.phase_error_turns.push(e / TAU);
            }
            e += TAU * (self.f_ref_hz - f / n) * dt;
            sum += e * dt;
        }
        tr.lock_time_us = (outside_until < duration_us * 0.9).then_some(outside_until);
        tr
    }
}

// ---------------------------------------------------------------------------------
// Op-amp: a non-inverting amplifier
// ---------------------------------------------------------------------------------

/// An op-amp with two resistors: R2 from the output back to the inverting input, R1
/// from there to ground. The signal goes to the non-inverting input.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NonInverting {
    pub r1_ohm: f64,
    pub r2_ohm: f64,
    /// The output cannot leave the range 0 .. supply.
    pub supply_v: f64,
    /// Gain-bandwidth product of the op-amp, Hz.
    pub gbw_hz: f64,
}

impl NonInverting {
    /// The gain for slow signals: 1 + R2 / R1.
    pub fn gain(&self) -> f64 {
        1.0 + self.r2_ohm / self.r1_ohm
    }

    /// The frequency at which the gain has dropped to 71 %: gain-bandwidth product / gain.
    pub fn bandwidth_hz(&self) -> f64 {
        self.gbw_hz / self.gain()
    }

    /// The gain for a sine of `f_hz` (a single-pole roll-off).
    pub fn gain_at(&self, f_hz: f64) -> f64 {
        self.gain() / (1.0 + (f_hz / self.bandwidth_hz()).powi(2)).sqrt()
    }

    /// How far the output lags behind the input at `f_hz`, radians.
    pub fn lag_at(&self, f_hz: f64) -> f64 {
        (f_hz / self.bandwidth_hz()).atan()
    }

    /// The output for an input that swings between 0 and `swing_v` as a sine of `f_hz`,
    /// at time `t_s`: `(ideal, real)`. The real output is limited by the bandwidth and
    /// clipped at the supply.
    pub fn output(&self, swing_v: f64, f_hz: f64, t_s: f64) -> (f64, f64) {
        let half = swing_v / 2.0;
        let ideal = self.gain() * half * (1.0 + (TAU * f_hz * t_s).sin());
        let real = self.gain() * half + self.gain_at(f_hz) * half * (TAU * f_hz * t_s - self.lag_at(f_hz)).sin();
        (ideal, real.clamp(0.0, self.supply_v))
    }
}

// ---------------------------------------------------------------------------------
// Buck converter: a lower voltage by switching
// ---------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Buck {
    pub vin: f64,
    /// Share of each period the switch is on, 0..=1.
    pub duty: f64,
    pub f_sw_hz: f64,
    pub l_h: f64,
    pub c_f: f64,
    pub load_ohm: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BuckTrace {
    /// Time in switching periods, over the last few periods of the run.
    pub t: Vec<f64>,
    /// Voltage at the switch side of the inductor.
    pub v_switch: Vec<f64>,
    pub i_l: Vec<f64>,
    pub v_out: Vec<f64>,
    pub v_out_mean: f64,
    pub v_ripple: f64,
    pub i_ripple: f64,
    /// True if the inductor current falls to zero in every period.
    pub discontinuous: bool,
}

impl Buck {
    /// Output voltage while the inductor current never reaches zero.
    pub fn ideal_vout(&self) -> f64 {
        self.duty * self.vin
    }

    /// Peak-to-peak inductor current: (V_in - V_out) x duty / (L x f).
    pub fn ideal_ripple_a(&self) -> f64 {
        (self.vin - self.ideal_vout()) * self.duty / (self.l_h * self.f_sw_hz)
    }

    /// An ideal buck converter (lossless switch and diode) run until it has settled;
    /// the last `show` periods are returned.
    pub fn simulate(&self, show: usize) -> BuckTrace {
        const PER: usize = 200;
        let dt = 1.0 / self.f_sw_hz / PER as f64;
        // Long enough for the L-C circuit to stop ringing: several of its time constants.
        let settle = (20.0 * self.load_ohm * self.c_f).max(40.0 * self.l_h / self.load_ohm) * self.f_sw_hz;
        let periods = (settle.ceil() as usize).clamp(200, 20_000) + show;
        let (mut i, mut v) = (self.ideal_vout() / self.load_ohm, self.ideal_vout());
        let mut tr = BuckTrace::default();
        let (mut sum, mut count) = (0.0, 0usize);
        let (mut v_lo, mut v_hi, mut i_lo, mut i_hi) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
        for p in 0..periods {
            for k in 0..PER {
                let on = k < (self.duty * PER as f64).round() as usize;
                // Off: the current flows on through the diode until it reaches zero.
                let v_sw = if on { self.vin } else if i > 0.0 { 0.0 } else { v };
                i = (i + (v_sw - v) / self.l_h * dt).max(0.0);
                v += (i - v / self.load_ohm) / self.c_f * dt;
                if p >= periods - show {
                    tr.t.push((p - (periods - show)) as f64 + k as f64 / PER as f64);
                    tr.v_switch.push(v_sw);
                    tr.i_l.push(i);
                    tr.v_out.push(v);
                    sum += v;
                    count += 1;
                    (v_lo, v_hi, i_lo, i_hi) = (v_lo.min(v), v_hi.max(v), i_lo.min(i), i_hi.max(i));
                }
            }
        }
        tr.v_out_mean = sum / count.max(1) as f64;
        tr.v_ripple = v_hi - v_lo;
        tr.i_ripple = i_hi - i_lo;
        tr.discontinuous = i_lo <= 1e-9;
        tr
    }
}

// ---------------------------------------------------------------------------------
// PIR motion sensor: two elements, a lens with many zones, a band-pass amplifier
// ---------------------------------------------------------------------------------

/// A person walking in front of a PIR sensor. The sensor is at the origin and looks
/// along +y; positions are in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PirScene {
    /// Where the walk starts.
    pub start: (f64, f64),
    /// Walking direction in degrees: 0 = to the right (across the view), 90 = away from
    /// the sensor, -90 = straight towards it.
    pub heading_deg: f64,
    pub speed_mps: f64,
    /// Angle between neighbouring lens zones, degrees. Zones alternate between the two
    /// sensor elements.
    pub zone_pitch_deg: f64,
    /// A warm draught that heats the sensor window, as a share of a walking person's
    /// signal at 5 m. It reaches both elements equally.
    pub draught: f64,
    /// False: the usual dual sensor (element A minus element B). True: one element only.
    pub single_element: bool,
}

pub const PIR_RATE_HZ: f64 = 100.0;
pub const PIR_SECONDS: f64 = 8.0;
/// Band-pass corners of the amplifier: human motion lies roughly in 0.5 .. 5 Hz.
pub const PIR_BAND_HZ: (f64, f64) = (0.5, 5.0);
pub const PIR_GAIN: f64 = 2000.0;
/// The window comparator reports motion when the amplified signal leaves +-this.
pub const PIR_THRESHOLD_V: f64 = 0.5;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PirTrace {
    pub t: Vec<f64>,
    pub position: Vec<(f64, f64)>,
    /// What the element(s) deliver, millivolts (illustrative size).
    pub raw_mv: Vec<f64>,
    /// After the band-pass amplifier, volts around its resting point.
    pub amplified_v: Vec<f64>,
    /// Stretches of time in which the comparator reports motion.
    pub detections: Vec<(f64, f64)>,
}

impl PirScene {
    /// Zone centres in radians, and to which element each belongs (+1 = A, -1 = B).
    pub fn zones(&self) -> Vec<(f64, f64)> {
        let pitch = self.zone_pitch_deg.to_radians();
        let count = (50f64.to_radians() / pitch).floor() as i32;
        (-count..=count).map(|k| (k as f64 * pitch, if k.rem_euclid(2) == 0 { 1.0 } else { -1.0 })).collect()
    }

    pub fn simulate(&self) -> PirTrace {
        let n = (PIR_SECONDS * PIR_RATE_HZ) as usize;
        let zones = self.zones();
        let width = self.zone_pitch_deg.to_radians() / 4.0;
        let (dx, dy) = (self.heading_deg.to_radians().cos(), self.heading_deg.to_radians().sin());
        let mut tr = PirTrace::default();
        for i in 0..n {
            let t = i as f64 / PIR_RATE_HZ;
            let (x, y) = (self.start.0 + dx * self.speed_mps * t, self.start.1 + dy * self.speed_mps * t);
            let r2 = (x * x + y * y).max(0.25);
            let angle = x.atan2(y);
            // The infrared a person sends into one zone falls with the square of the distance.
            let seen = |sign: f64| -> f64 {
                zones.iter().filter(|z| z.1 == sign).map(|z| (-((angle - z.0) / width).powi(2) / 2.0).exp()).sum::<f64>() * 25.0 / r2
            };
            // The draught: a smooth warm puff from 2 to 5 s, the same on both elements.
            let puff = if (2.0..5.0).contains(&t) { self.draught * (std::f64::consts::PI * (t - 2.0) / 3.0).sin().powi(2) } else { 0.0 };
            let raw = if self.single_element { seen(1.0) + puff } else { (seen(1.0) + puff) - (seen(-1.0) + puff) };
            tr.t.push(t);
            tr.position.push((x, y));
            tr.raw_mv.push(0.5 * raw);
        }
        // Band-pass: a high-pass (the input minus its slow part) followed by a low-pass.
        let slow = lowpass(&tr.raw_mv, lowpass_alpha(PIR_BAND_HZ.0, PIR_RATE_HZ));
        let high: Vec<f64> = tr.raw_mv.iter().zip(&slow).map(|(x, s)| x - s).collect();
        tr.amplified_v = lowpass(&high, lowpass_alpha(PIR_BAND_HZ.1, PIR_RATE_HZ)).iter().map(|v| v * PIR_GAIN / 1000.0).collect();
        let mut start = None;
        for (i, v) in tr.amplified_v.iter().enumerate() {
            let out = v.abs() > PIR_THRESHOLD_V;
            match (out, start) {
                (true, None) => start = Some(tr.t[i]),
                (false, Some(s)) => {
                    tr.detections.push((s, tr.t[i]));
                    start = None;
                }
                _ => {}
            }
        }
        if let Some(s) = start {
            tr.detections.push((s, PIR_SECONDS));
        }
        tr
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alias_frequencies() {
        assert_eq!(alias_frequency(400.0, 1000.0), 400.0);
        assert_eq!(alias_frequency(900.0, 1000.0), 100.0);
        assert_eq!(alias_frequency(1100.0, 1000.0), 100.0);
        assert_eq!(alias_frequency(1000.0, 1000.0), 0.0);
        assert_eq!(alias_frequency(2400.0, 1000.0), 400.0);
        assert_eq!(alias_signed(900.0, 1000.0), -100.0);
        // The samples of a 900 Hz sine are the samples of a mirrored 100 Hz sine.
        for n in 0..50 {
            let t = n as f64 / 1000.0;
            assert!(((TAU * 900.0 * t).sin() - (TAU * -100.0 * t).sin()).abs() < 1e-9);
        }
    }

    #[test]
    fn averaging_n_readings_divides_the_noise_by_root_n() {
        let r = noisy_readings(8000, 1.0, 1.0, 0, 0.1, 16, 42);
        let (raw, avg) = (std_dev(&r.raw[100..]), std_dev(&r.averaged[100..]));
        assert!((raw - 0.1).abs() < 0.01, "{raw}");
        // sigma / sqrt(16) = 0.025
        assert!((avg - 0.025).abs() < 0.005, "{avg}");
        assert!((mean(&r.averaged[100..]) - 1.0).abs() < 0.01);
        // The price: after a jump, the average needs n readings to arrive.
        let r = noisy_readings(200, 1.0, 2.0, 100, 0.0, 16, 1);
        assert_eq!(r.averaged[99], 1.0);
        assert!((r.averaged[107] - 1.5).abs() < 1e-9, "halfway after n/2 readings");
        assert!((r.averaged[115] - 2.0).abs() < 1e-9, "there after n readings");
    }

    #[test]
    fn lowpass_passes_slow_and_blocks_fast() {
        let (fs, fc) = (1000.0, 10.0);
        let alpha = lowpass_alpha(fc, fs);
        assert!((alpha - 0.0609).abs() < 1e-3, "{alpha}");
        assert!((lowpass_gain(0.0, fs, alpha) - 1.0).abs() < 1e-12);
        // At the cutoff frequency about 71 % gets through (-3 dB).
        assert!((lowpass_gain(fc, fs, alpha) - 0.707).abs() < 0.02, "{}", lowpass_gain(fc, fs, alpha));
        // Ten times the cutoff: about a tenth.
        assert!((lowpass_gain(100.0, fs, alpha) - 0.1).abs() < 0.02);
        // The formula agrees with actually filtering a sine.
        for f in [2.0, 10.0, 50.0] {
            let x: Vec<f64> = (0..4000).map(|n| (TAU * f * n as f64 / fs).sin()).collect();
            let y = lowpass(&x, alpha);
            let peak = y[2000..].iter().fold(0.0f64, |m, v| m.max(v.abs()));
            assert!((peak - lowpass_gain(f, fs, alpha)).abs() < 0.01, "{f} Hz: {peak}");
        }
    }

    const PLL: Pll = Pll { f_ref_hz: 1e6, vco_min_hz: 2.36e9, vco_max_hz: 2.52e9, loop_hz: 50e3, damping: 0.7 };

    #[test]
    fn pll_settles_on_n_times_the_reference() {
        let tr = PLL.hop(2402, 2480, 400.0, 10e3);
        assert_eq!(tr.target_hz, 2.48e9);
        assert_eq!(tr.f_vco_hz[0], 2.402e9);
        assert!((tr.f_vco_hz.last().unwrap() - 2.48e9).abs() < 1e3, "{}", tr.f_vco_hz.last().unwrap());
        assert!(tr.phase_error_turns.last().unwrap().abs() < 1e-3);
        let lock = tr.lock_time_us.unwrap();
        assert!(lock > 5.0 && lock < 100.0, "{lock}");
        // A slower loop takes longer; a badly damped one overshoots more and takes longer.
        assert!(Pll { loop_hz: 10e3, ..PLL }.hop(2402, 2480, 2000.0, 10e3).lock_time_us.unwrap() > 3.0 * lock);
        let ringing = Pll { damping: 0.15, ..PLL }.hop(2402, 2480, 400.0, 10e3);
        assert!(ringing.overshoot_hz > 2.0 * tr.overshoot_hz, "{} vs {}", ringing.overshoot_hz, tr.overshoot_hz);
        assert!(ringing.lock_time_us.unwrap() > lock);
        // Hopping down works as well.
        assert!(PLL.hop(2480, 2402, 400.0, 10e3).lock_time_us.is_some());
    }

    #[test]
    fn pll_cannot_lock_outside_the_oscillator_range() {
        let tr = PLL.hop(2402, 2560, 400.0, 10e3);
        assert_eq!(tr.lock_time_us, None);
        assert_eq!(*tr.f_vco_hz.last().unwrap(), 2.52e9, "stuck at the top of its range");
    }

    #[test]
    fn non_inverting_amplifier() {
        let a = NonInverting { r1_ohm: 1e3, r2_ohm: 10e3, supply_v: 3.3, gbw_hz: 1e6 };
        assert_eq!(a.gain(), 11.0);
        assert!((a.bandwidth_hz() - 90_909.0).abs() < 1.0);
        assert!((a.gain_at(a.bandwidth_hz()) - 11.0 / 2f64.sqrt()).abs() < 1e-9);
        // A slow 0..100 mV input: the output peaks at 1.1 V, no clipping.
        let peak = (0..1000).map(|k| a.output(0.1, 100.0, k as f64 * 1e-5).1).fold(0.0, f64::max);
        assert!((peak - 1.1).abs() < 0.01, "{peak}");
        // 0..400 mV would need 4.4 V: clipped at the 3.3 V supply.
        let peak = (0..1000).map(|k| a.output(0.4, 100.0, k as f64 * 1e-5).1).fold(0.0, f64::max);
        assert_eq!(peak, 3.3);
        // More gain, less bandwidth: gain 101 leaves 9.9 kHz.
        assert!((NonInverting { r2_ohm: 100e3, ..a }.bandwidth_hz() - 9_901.0).abs() < 1.0);
    }

    #[test]
    fn buck_converter_steady_state() {
        // 12 V in, 27.5 % duty: 3.3 V out. 47 µH at 500 kHz: ripple (12 - 3.3) x 0.275 / (47e-6 x 5e5) = 102 mA.
        let b = Buck { vin: 12.0, duty: 0.275, f_sw_hz: 500e3, l_h: 47e-6, c_f: 22e-6, load_ohm: 6.6 };
        assert!((b.ideal_vout() - 3.3).abs() < 1e-12);
        assert!((b.ideal_ripple_a() - 0.1018).abs() < 1e-3);
        let tr = b.simulate(3);
        assert!((tr.v_out_mean - 3.3).abs() < 0.03, "{}", tr.v_out_mean);
        assert!((tr.i_ripple - b.ideal_ripple_a()).abs() < 0.005, "{}", tr.i_ripple);
        assert!(!tr.discontinuous);
        assert!(tr.v_ripple < 0.01);
        // A faster switch halves the ripple; a smaller inductor raises it.
        assert!((Buck { f_sw_hz: 1e6, ..b }.simulate(3).i_ripple - 0.0509).abs() < 0.004);
        assert!(Buck { l_h: 10e-6, ..b }.simulate(3).i_ripple > 0.4);
        // A very light load: the current reaches zero and the output rises above duty x V_in.
        let light = Buck { load_ohm: 660.0, ..b }.simulate(3);
        assert!(light.discontinuous);
        assert!(light.v_out_mean > 4.0, "{}", light.v_out_mean);
    }

    const WALK: PirScene = PirScene { start: (-4.0, 5.0), heading_deg: 0.0, speed_mps: 1.0, zone_pitch_deg: 6.0, draught: 0.0, single_element: false };

    #[test]
    fn pir_sees_walking_across() {
        let tr = WALK.simulate();
        assert!(!tr.detections.is_empty());
        // Crossing zones gives an alternating signal: the raw signal changes sign many times.
        let flips = tr.raw_mv.windows(2).filter(|w| w[0].signum() != w[1].signum()).count();
        assert!(flips > 10, "{flips}");
    }

    #[test]
    fn pir_weaknesses() {
        // Straight towards the sensor along the middle of a zone: barely seen.
        let towards = PirScene { start: (0.0, 9.0), heading_deg: -90.0, speed_mps: 0.6, ..WALK }.simulate();
        assert!(towards.detections.is_empty(), "{:?}", towards.detections);
        let peak = |tr: &PirTrace| tr.amplified_v.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        assert!(peak(&towards) < 0.5 * peak(&WALK.simulate()));
        // Very slow across: below the band, not seen.
        let slow = PirScene { speed_mps: 0.1, ..WALK }.simulate();
        assert!(slow.detections.is_empty(), "{:?}", slow.detections);
        // A draught on an empty room: the dual sensor cancels it, a single element does not.
        let empty = PirScene { start: (30.0, 5.0), speed_mps: 0.0, draught: 2.0, ..WALK };
        assert!(empty.simulate().detections.is_empty());
        assert!(!PirScene { single_element: true, ..empty }.simulate().detections.is_empty());
        // Far away the signal is too weak.
        assert!(PirScene { start: (-6.0, 14.0), ..WALK }.simulate().detections.is_empty());
    }
}
