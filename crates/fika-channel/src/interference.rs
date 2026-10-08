//! Deterministic interferers.

use std::f64::consts::PI;

/// Add a steady carrier of `amplitude` at `freq_hz`.
pub fn add_carrier(samples: &mut [f32], freq_hz: f64, amplitude: f64, fs: f64) {
    for (i, x) in samples.iter_mut().enumerate() {
        *x += (amplitude * (2.0 * PI * freq_hz * i as f64 / fs).sin()) as f32;
    }
}

/// Mix `other` into `samples` starting at `offset` samples, scaled.
pub fn add_signal(samples: &mut [f32], other: &[f32], offset: usize, scale: f32) {
    for (i, &v) in other.iter().enumerate() {
        if let Some(x) = samples.get_mut(offset + i) {
            *x += v * scale;
        }
    }
}
