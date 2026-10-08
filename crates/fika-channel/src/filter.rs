//! A 300–2700 Hz SSB receiver passband, windowed-sinc FIR.

use std::f64::consts::PI;

fn lowpass(taps: usize, fc_norm: f64) -> Vec<f64> {
    let m = (taps - 1) as f64;
    (0..taps)
        .map(|i| {
            let n = i as f64 - m / 2.0;
            let sinc = if n == 0.0 {
                2.0 * fc_norm
            } else {
                (2.0 * PI * fc_norm * n).sin() / (PI * n)
            };
            let w = 0.42 - 0.5 * (2.0 * PI * i as f64 / m).cos()
                + 0.08 * (4.0 * PI * i as f64 / m).cos();
            sinc * w
        })
        .collect()
}

/// Bandpass taps: lowpass at `hi` minus lowpass at `lo`.
pub fn bandpass_taps(taps: usize, fs: f64, lo_hz: f64, hi_hz: f64) -> Vec<f32> {
    let a = lowpass(taps, hi_hz / fs);
    let b = lowpass(taps, lo_hz / fs);
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y) as f32)
        .collect()
}

/// Apply the SSB passband (delay compensated).
pub fn ssb_passband(x: &[f32], fs: f64) -> Vec<f32> {
    let taps = 257;
    let h = bandpass_taps(taps, fs, 300.0, 2700.0);
    let half = taps / 2;
    (0..x.len())
        .map(|n| {
            let mut acc = 0f32;
            for (j, &c) in h.iter().enumerate() {
                let idx = n as i64 + j as i64 - half as i64;
                if idx >= 0 && (idx as usize) < x.len() {
                    acc += c * x[idx as usize];
                }
            }
            acc
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms_of_tone(f: f64) -> f32 {
        let fs = 12_000.0;
        let x: Vec<f32> = (0..24_000)
            .map(|i| (2.0 * PI * f * i as f64 / fs).sin() as f32)
            .collect();
        let y = ssb_passband(&x, fs);
        (y[4000..20000].iter().map(|v| v * v).sum::<f32>() / 16000.0).sqrt()
    }

    #[test]
    fn passes_band_rejects_outside() {
        assert!((rms_of_tone(1500.0) - 0.707).abs() < 0.03);
        assert!(rms_of_tone(100.0) < 0.05);
        assert!(rms_of_tone(3500.0) < 0.05);
    }
}
