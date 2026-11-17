//! Radio basics: antennas, the link budget, and who else uses the 2.4 GHz band.

use crate::units::C;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

// ---------------------------------------------------------------------------------
// Antennas
// ---------------------------------------------------------------------------------

/// Field strength of a half-wave dipole in a direction `theta` radians away from the
/// wire's axis, relative to its maximum: `cos(pi/2 cos theta) / sin theta`.
pub fn dipole_field(theta: f64) -> f64 {
    let s = theta.sin();
    if s.abs() < 1e-9 { 0.0 } else { (FRAC_PI_2 * theta.cos()).cos() / s }
}

/// Two vertical half-wave dipoles side by side, `spacing` wavelengths apart along x,
/// the second fed `phase` radians later than the first.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Antenna {
    /// One dipole only (`spacing` and `phase` are then ignored).
    pub single: bool,
    pub spacing_wl: f64,
    pub phase: f64,
}

impl Antenna {
    /// Relative field in the direction (`theta` from the vertical, `phi` around it,
    /// measured from the x axis).
    pub fn field(&self, theta: f64, phi: f64) -> f64 {
        let element = dipole_field(theta);
        if self.single {
            return element.abs();
        }
        // Path difference between the two elements, as a phase: k d cos(angle to x axis).
        let psi = TAU * self.spacing_wl * theta.sin() * phi.cos() - self.phase;
        (element * (psi / 2.0).cos()).abs()
    }

    /// Directivity: the maximum intensity divided by the average over all directions.
    /// Found by summing over a grid of directions.
    pub fn directivity(&self) -> f64 {
        let (nt, np) = (180, if self.single { 1 } else { 180 });
        let (dt, dp) = (PI / nt as f64, TAU / np as f64);
        let (mut sum, mut max) = (0.0, 0.0f64);
        for i in 0..nt {
            let theta = (i as f64 + 0.5) * dt;
            for j in 0..np {
                let phi = (j as f64 + 0.5) * dp;
                let u = self.field(theta, phi).powi(2);
                max = max.max(u);
                sum += u * theta.sin() * dt * dp;
            }
        }
        4.0 * PI * max / sum
    }

    /// Gain in dBi in the horizontal plane (theta = 90 degrees) at azimuth `phi`,
    /// for a lossless antenna.
    pub fn gain_dbi(&self, phi: f64, directivity: f64) -> f64 {
        let max = (0..720).map(|j| self.field(FRAC_PI_2, j as f64 * TAU / 720.0)).fold(0.0f64, f64::max);
        let max = if self.single { 1.0 } else { max };
        let rel = self.field(FRAC_PI_2, phi) / max.max(1e-12);
        10.0 * (directivity * rel * rel).max(1e-12).log10()
    }
}

// ---------------------------------------------------------------------------------
// Link budget
// ---------------------------------------------------------------------------------

/// Free-space path loss in dB between two isotropic antennas `d_m` metres apart:
/// `20 log10(4 pi d f / c)` (Friis).
pub fn fspl_db(d_m: f64, f_hz: f64) -> f64 {
    20.0 * (4.0 * PI * d_m * f_hz / C).log10()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Link {
    pub tx_dbm: f64,
    pub tx_gain_dbi: f64,
    pub rx_gain_dbi: f64,
    /// Walls, bodies, a metal housing ...: everything on top of free space, dB.
    pub extra_loss_db: f64,
    pub f_hz: f64,
    /// The weakest signal the receiver still decodes, dBm.
    pub sensitivity_dbm: f64,
}

impl Link {
    pub fn received_dbm(&self, d_m: f64) -> f64 {
        self.tx_dbm + self.tx_gain_dbi + self.rx_gain_dbi - fspl_db(d_m, self.f_hz) - self.extra_loss_db
    }

    /// Distance at which the received power falls to the sensitivity.
    pub fn range_m(&self) -> f64 {
        let allowed = self.tx_dbm + self.tx_gain_dbi + self.rx_gain_dbi - self.extra_loss_db - self.sensitivity_dbm;
        C / (4.0 * PI * self.f_hz) * 10f64.powf(allowed / 20.0)
    }
}

// ---------------------------------------------------------------------------------
// The 2.4 GHz band
// ---------------------------------------------------------------------------------

/// The licence-free band: 2400 to 2483.5 MHz.
pub const ISM_MHZ: (f64, f64) = (2400.0, 2483.5);

/// Centre of Wi-Fi channel 1..=14, MHz: 5 MHz apart from 2412 MHz, and 2484 MHz for 14.
pub fn wifi_mhz(channel: u8) -> f64 {
    if channel == 14 { 2484.0 } else { 2407.0 + 5.0 * channel as f64 }
}

/// Width actually occupied by an OFDM Wi-Fi channel (802.11g/n), MHz.
pub const WIFI_OCCUPIED_MHZ: f64 = 16.25;

/// Centre of an IEEE 802.15.4 channel 11..=26 (Zigbee, Thread), MHz.
pub fn zigbee_mhz(channel: u8) -> f64 {
    2405.0 + 5.0 * (channel as f64 - 11.0)
}

pub const ZIGBEE_WIDTH_MHZ: f64 = 2.0;
/// A Bluetooth LE channel at 1 Mbit/s, about 2 MHz wide.
pub const BLE_WIDTH_MHZ: f64 = 2.0;

/// Do two signals of the given centres and widths overlap?
pub fn overlaps(a_mhz: f64, a_width: f64, b_mhz: f64, b_width: f64) -> bool {
    (a_mhz - b_mhz).abs() < (a_width + b_width) / 2.0
}

/// Which of the 37 Bluetooth LE data channels (0..=36) are free of the given Wi-Fi
/// and 802.15.4 channels.
pub fn free_ble_data_channels(wifi: &[u8], zigbee: &[u8]) -> [bool; 37] {
    let mut free = [true; 37];
    for (i, f) in free.iter_mut().enumerate() {
        let mhz = crate::ble::channel_mhz(i as u8) as f64;
        *f = !wifi.iter().any(|&w| overlaps(mhz, BLE_WIDTH_MHZ, wifi_mhz(w), WIFI_OCCUPIED_MHZ)) && !zigbee.iter().any(|&z| overlaps(mhz, BLE_WIDTH_MHZ, zigbee_mhz(z), ZIGBEE_WIDTH_MHZ));
    }
    free
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_wave_dipole() {
        assert_eq!(dipole_field(0.0), 0.0, "nothing along the wire");
        assert!((dipole_field(FRAC_PI_2) - 1.0).abs() < 1e-12, "maximum broadside");
        let d = Antenna { single: true, spacing_wl: 0.0, phase: 0.0 }.directivity();
        assert!((d - 1.641).abs() < 0.002, "{d}");
        assert!((10.0 * d.log10() - 2.15).abs() < 0.01);
        let one = Antenna { single: true, spacing_wl: 0.0, phase: 0.0 };
        assert!((one.gain_dbi(1.0, d) - 2.15).abs() < 0.01, "the same in every horizontal direction");
    }

    #[test]
    fn two_dipoles_shape_the_beam() {
        // Half a wavelength apart, fed in step: strongest broadside (along y), nothing along x.
        let a = Antenna { single: false, spacing_wl: 0.5, phase: 0.0 };
        assert!(a.field(FRAC_PI_2, FRAC_PI_2) > 0.999);
        assert!(a.field(FRAC_PI_2, 0.0) < 1e-9);
        let d = a.directivity();
        // 3.96 (6.0 dBi), checked by an independent integration; mutual coupling between
        // the two dipoles, which changes this a little in reality, is ignored.
        assert!((d - 3.96).abs() < 0.01, "{d}");
        // Fed half a period apart: the beam turns to point along x instead.
        let b = Antenna { phase: PI, ..a };
        assert!(b.field(FRAC_PI_2, 0.0) > 0.999);
        assert!(b.field(FRAC_PI_2, FRAC_PI_2) < 1e-9);
        // A quarter wavelength apart and a quarter period late: one direction only (cardioid).
        let c = Antenna { single: false, spacing_wl: 0.25, phase: FRAC_PI_2 };
        assert!(c.field(FRAC_PI_2, 0.0) > 0.999 && c.field(FRAC_PI_2, PI) < 1e-9);
    }

    #[test]
    fn free_space_path_loss_and_range() {
        assert!((fspl_db(1.0, 2.44e9) - 40.2).abs() < 0.05, "{}", fspl_db(1.0, 2.44e9));
        assert!((fspl_db(10.0, 2.44e9) - 60.2).abs() < 0.05);
        // Twice the distance: 6 dB more loss.
        assert!((fspl_db(20.0, 2.44e9) - fspl_db(10.0, 2.44e9) - 6.02).abs() < 0.01);
        let link = Link { tx_dbm: 0.0, tx_gain_dbi: 0.0, rx_gain_dbi: 0.0, extra_loss_db: 0.0, f_hz: 2.44e9, sensitivity_dbm: -70.0 };
        assert!((link.received_dbm(10.0) + 60.2).abs() < 0.05);
        // -70 dBm is reached where the loss is 70 dB: about 31 m.
        assert!((link.range_m() - 31.0).abs() < 0.3, "{}", link.range_m());
        assert!((link.received_dbm(link.range_m()) + 70.0).abs() < 1e-9);
        // 20 dB more budget: ten times the range.
        let better = Link { sensitivity_dbm: -90.0, ..link };
        assert!((better.range_m() / link.range_m() - 10.0).abs() < 1e-9);
    }

    #[test]
    fn band_plan() {
        assert_eq!((wifi_mhz(1), wifi_mhz(6), wifi_mhz(11), wifi_mhz(13), wifi_mhz(14)), (2412.0, 2437.0, 2462.0, 2472.0, 2484.0));
        assert_eq!((zigbee_mhz(11), zigbee_mhz(26)), (2405.0, 2480.0));
        // The three advertising channels sit outside Wi-Fi channels 1, 6 and 11.
        for adv in [2402.0, 2426.0, 2480.0] {
            assert!([1, 6, 11].iter().all(|&w| !overlaps(adv, BLE_WIDTH_MHZ, wifi_mhz(w), WIFI_OCCUPIED_MHZ)), "{adv}");
        }
        // With 1, 6 and 11 busy, 9 data channels stay free.
        let free = free_ble_data_channels(&[1, 6, 11], &[]);
        assert_eq!(free.iter().filter(|f| **f).count(), 9);
        // Nothing busy: all 37 free. Channel 13 also covers the top data channels.
        assert_eq!(free_ble_data_channels(&[], &[]).iter().filter(|f| **f).count(), 37);
        assert!(overlaps(2480.0, BLE_WIDTH_MHZ, wifi_mhz(13), WIFI_OCCUPIED_MHZ));
    }
}
