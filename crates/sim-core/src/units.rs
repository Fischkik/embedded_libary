//! Physical constants, SI formatting, decibels and hex helpers.

/// Speed of light in vacuum, m/s.
pub const C: f64 = 299_792_458.0;

/// Format a value with an SI prefix: `si(2.402e9, "Hz") == "2.402 GHz"`.
pub fn si(value: f64, unit: &str) -> String {
    if value == 0.0 || !value.is_finite() {
        return format!("{value} {unit}");
    }
    const PREFIXES: [(f64, &str); 9] = [
        (1e12, "T"),
        (1e9, "G"),
        (1e6, "M"),
        (1e3, "k"),
        (1.0, ""),
        (1e-3, "m"),
        (1e-6, "µ"),
        (1e-9, "n"),
        (1e-12, "p"),
    ];
    let a = value.abs();
    let (scale, prefix) = PREFIXES
        .iter()
        .copied()
        .find(|(s, _)| a >= *s * 0.99995)
        .unwrap_or(PREFIXES[PREFIXES.len() - 1]);
    let v = value / scale;
    let decimals = if v.abs() >= 100.0 {
        1
    } else if v.abs() >= 10.0 {
        2
    } else {
        3
    };
    let mut s = format!("{v:.decimals$}");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    format!("{s} {prefix}{unit}")
}

/// Power ratio to decibels.
pub fn db(power_ratio: f64) -> f64 {
    10.0 * power_ratio.max(1e-30).log10()
}

/// Amplitude ratio to decibels.
pub fn db_amp(amplitude_ratio: f64) -> f64 {
    20.0 * amplitude_ratio.max(1e-15).log10()
}

/// Wavelength in metres of a wave with frequency `f_hz` travelling at light speed.
pub fn wavelength(f_hz: f64) -> f64 {
    C / f_hz
}

/// `[0xde, 0xad]` -> `"de ad"`.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
}

/// Parse hex text, ignoring spaces, commas and `0x`. Returns `None` on any bad digit
/// or an odd number of digits.
pub fn unhex(text: &str) -> Option<Vec<u8>> {
    let cleaned: String = text
        .replace("0x", "")
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',' && *c != ':')
        .collect();
    if cleaned.len() % 2 != 0 {
        return None;
    }
    (0..cleaned.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(cleaned.get(i..i + 2)?, 16).ok())
        .collect()
}

/// Bytes to bits, least significant bit of each byte first (the order BLE sends them).
pub fn bits_lsb_first(bytes: &[u8]) -> Vec<bool> {
    bytes.iter().flat_map(|b| (0..8).map(move |i| (b >> i) & 1 == 1)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn si_formatting() {
        assert_eq!(si(2.402e9, "Hz"), "2.402 GHz");
        assert_eq!(si(250e3, "Hz"), "250 kHz");
        assert_eq!(si(0.0125, "m"), "12.5 mm");
        assert_eq!(si(1.0, "s"), "1 s");
        assert_eq!(si(-3.3e-6, "s"), "-3.3 µs");
    }

    #[test]
    fn hex_round_trip() {
        assert_eq!(unhex("de ad BE,ef").unwrap(), vec![0xde, 0xad, 0xbe, 0xef]);
        assert_eq!(hex(&[0xde, 0xad]), "de ad");
        assert!(unhex("abc").is_none());
        assert!(unhex("zz").is_none());
        assert_eq!(bits_lsb_first(&[0x42]), [false, true, false, false, false, false, true, false]);
    }
}
