//! Phase-continuous Gaussian-shaped MFSK synthesis (SPEC §3.2, §11).
//!
//! The tone index sequence is expanded to 32 ms shaping units, convolved
//! with the FT8-style erf pulse, turned into instantaneous frequency and
//! integrated into a phase accumulator. Amplitude is constant apart from
//! raised-cosine ramps of one unit at each end.

use std::f64::consts::PI;

use crate::error::ModemError;
use crate::params::{BIN_HZ, GAUSS_BT, check_lane, samples_per_unit, tone_hz};
use crate::profile::Profile;

/// Abramowitz & Stegun 7.1.26, max error 1.5e-7.
pub fn erf(x: f64) -> f64 {
    let t = 1.0 / (1.0 + 0.3275911 * x.abs());
    let poly =
        ((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t + 0.254829592;
    let y = 1.0 - poly * t * (-x * x).exp();
    if x >= 0.0 { y } else { -y }
}

/// Gaussian-smoothed unit box centred on 0 with support about ±1.
pub fn pulse(x: f64) -> f64 {
    let kappa = PI * (2.0 / 2f64.ln()).sqrt() * GAUSS_BT;
    0.5 * (erf(kappa * (x + 0.5)) - erf(kappa * (x - 0.5)))
}

/// Instantaneous tone index (fractional) per sample for a unit sequence.
fn frequency_track(units: &[u8], spu: usize) -> Vec<f64> {
    let n_units = units.len();
    let total = n_units * spu;
    let at = |j: i64| -> f64 {
        let j = j.clamp(0, n_units as i64 - 1);
        units[j as usize] as f64
    };
    (0..total)
        .map(|n| {
            let t = (n as f64 + 0.5) / spu as f64;
            let j0 = t.floor() as i64;
            (j0 - 2..=j0 + 1)
                .map(|j| at(j) * pulse(t - j as f64 - 0.5))
                .sum()
        })
        .collect()
}

/// Synthesise a burst of `tones` in `lane` at sample rate `fs`.
/// `freq_offset_hz` shifts every tone, to emulate a mistuned transmitter.
pub fn synthesize(
    tones: &[u8],
    lane: usize,
    profile: Profile,
    fs: u32,
    amplitude: f32,
    freq_offset_hz: f64,
) -> Result<Vec<f32>, ModemError> {
    check_lane(lane)?;
    let spu = samples_per_unit(fs)?;
    let r = profile.units_per_symbol();
    let units: Vec<u8> = tones
        .iter()
        .flat_map(|&t| std::iter::repeat_n(t, r))
        .collect();
    let track = frequency_track(&units, spu);

    let f0 = tone_hz(lane, 0) + freq_offset_hz;
    let mut phase = 0.0f64;
    let n = track.len();
    let mut out = Vec::with_capacity(n);
    for (i, idx) in track.iter().enumerate() {
        let f = f0 + BIN_HZ * idx;
        phase += 2.0 * PI * f / fs as f64;
        if phase > 2.0 * PI {
            phase -= 2.0 * PI;
        }
        let ramp = if i < spu {
            0.5 * (1.0 - (PI * i as f64 / spu as f64).cos())
        } else if i >= n - spu {
            0.5 * (1.0 - (PI * (n - 1 - i) as f64 / spu as f64).cos())
        } else {
            1.0
        };
        out.push((amplitude as f64 * ramp * phase.sin()) as f32);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn erf_and_pulse_sanity() {
        assert!((erf(0.0)).abs() < 1e-6);
        assert!((erf(3.0) - 1.0).abs() < 1e-4);
        assert!((pulse(0.0) - 1.0).abs() < 1e-6);
        assert!(pulse(1.0).abs() < 1e-6);
        // Partition of unity across integer-spaced pulses.
        for k in 0..10 {
            let t = k as f64 * 0.1;
            let s: f64 = (-3..=3).map(|j| pulse(t - j as f64)).sum();
            assert!((s - 1.0).abs() < 1e-6, "t={t} s={s}");
        }
    }

    #[test]
    fn slow_profile_holds_tone_flat_between_transitions() {
        let spu = 384;
        let units: Vec<u8> = [3u8; 5].iter().chain([9u8; 5].iter()).copied().collect();
        let track = frequency_track(&units, spu);
        // Middle of the first symbol: exactly tone 3.
        assert!((track[2 * spu + spu / 2] - 3.0).abs() < 1e-6);
        // Middle of second: exactly tone 9.
        assert!((track[7 * spu + spu / 2] - 9.0).abs() < 1e-6);
        // Transition at unit boundary 5 is halfway.
        assert!((track[5 * spu] - 6.0).abs() < 0.05);
    }

    #[test]
    fn single_tone_has_expected_frequency_and_constant_envelope() {
        let fs = 12_000;
        let tones = [7u8; 20];
        let x = synthesize(&tones, 1, Profile::Fast, fs, 0.5, 0.0).unwrap();
        assert_eq!(x.len(), 20 * 384);
        // Count zero crossings in the flat middle to estimate frequency.
        let mid = &x[5 * 384..15 * 384];
        let crossings = mid
            .windows(2)
            .filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0))
            .count();
        let f_est = crossings as f64 / 2.0 / (mid.len() as f64 / fs as f64);
        let f_exp = tone_hz(1, 7);
        assert!((f_est - f_exp).abs() < 2.0, "f_est {f_est} vs {f_exp}");
        let peak = mid.iter().fold(0f32, |m, &v| m.max(v.abs()));
        assert!((peak - 0.5).abs() < 0.01);
    }
}
