//! Integer-factor decimation with a windowed-sinc low-pass, for bringing
//! 48 kHz sound-card audio down to the 12 kHz receiver rate.

use std::f64::consts::PI;

/// Blackman-windowed sinc low-pass with cutoff at `cutoff / fs` of the
/// input rate, `taps` odd.
pub fn lowpass_taps(taps: usize, cutoff_norm: f64) -> Vec<f32> {
    let m = (taps - 1) as f64;
    let mut h: Vec<f64> = (0..taps)
        .map(|i| {
            let n = i as f64 - m / 2.0;
            let sinc = if n == 0.0 {
                2.0 * cutoff_norm
            } else {
                (2.0 * PI * cutoff_norm * n).sin() / (PI * n)
            };
            let w = 0.42 - 0.5 * (2.0 * PI * i as f64 / m).cos()
                + 0.08 * (4.0 * PI * i as f64 / m).cos();
            sinc * w
        })
        .collect();
    let sum: f64 = h.iter().sum();
    for v in h.iter_mut() {
        *v /= sum;
    }
    h.into_iter().map(|v| v as f32).collect()
}

/// Decimate by `factor`. Output sample `k` is the filtered input at
/// `k * factor`, delayed by half the filter length.
pub fn decimate(input: &[f32], factor: usize) -> Vec<f32> {
    if factor == 1 {
        return input.to_vec();
    }
    let taps = 32 * factor + 1;
    let h = lowpass_taps(taps, 0.45 / factor as f64);
    let half = taps / 2;
    let n_out = input.len() / factor;
    let mut out = Vec::with_capacity(n_out);
    for k in 0..n_out {
        let center = k * factor;
        let mut acc = 0f32;
        for (j, &c) in h.iter().enumerate() {
            let idx = center as i64 + j as i64 - half as i64;
            if idx >= 0 && (idx as usize) < input.len() {
                acc += c * input[idx as usize];
            }
        }
        out.push(acc);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimate_keeps_in_band_tone_and_kills_alias() {
        let fs = 48_000.0f64;
        let n = 48_000;
        let tone: Vec<f32> = (0..n)
            .map(|i| (2.0 * PI * 1000.0 * i as f64 / fs).sin() as f32)
            .collect();
        let y = decimate(&tone, 4);
        assert_eq!(y.len(), n / 4);
        let rms = (y[2000..10000].iter().map(|v| v * v).sum::<f32>() / 8000.0).sqrt();
        assert!((rms - 0.707).abs() < 0.02, "rms {rms}");
        let alias: Vec<f32> = (0..n)
            .map(|i| (2.0 * PI * 11_000.0 * i as f64 / fs).sin() as f32)
            .collect();
        let y = decimate(&alias, 4);
        let rms = (y[2000..10000].iter().map(|v| v * v).sum::<f32>() / 8000.0).sqrt();
        assert!(rms < 0.01, "alias rms {rms}");
    }
}

/// Stateful integer-factor decimator for streaming use. Same filter as
/// [`decimate`], with history carried between calls.
pub struct StreamDecimator {
    factor: usize,
    taps: Vec<f32>,
    history: Vec<f32>,
    phase: usize,
}

impl StreamDecimator {
    pub fn new(factor: usize) -> Self {
        let taps = if factor > 1 {
            lowpass_taps(32 * factor + 1, 0.45 / factor as f64)
        } else {
            vec![1.0]
        };
        Self {
            factor,
            history: vec![0.0; taps.len()],
            taps,
            phase: 0,
        }
    }

    /// Push input samples, append decimated output to `out`.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if self.factor == 1 {
            out.extend_from_slice(input);
            return;
        }
        let n = self.taps.len();
        for &x in input {
            // History is a shift register with the newest sample last.
            self.history.copy_within(1..n, 0);
            self.history[n - 1] = x;
            if self.phase == 0 {
                let mut acc = 0f32;
                for (h, c) in self.history.iter().zip(self.taps.iter().rev()) {
                    acc += h * c;
                }
                out.push(acc);
            }
            self.phase = (self.phase + 1) % self.factor;
        }
    }
}

#[cfg(test)]
mod stream_tests {
    use super::*;

    #[test]
    fn stream_matches_block_decimation_rate_and_level() {
        let fs = 48_000.0f64;
        let x: Vec<f32> = (0..48_000)
            .map(|i| (2.0 * PI * 1000.0 * i as f64 / fs).sin() as f32)
            .collect();
        let mut d = StreamDecimator::new(4);
        let mut y = Vec::new();
        for chunk in x.chunks(1000) {
            d.process(chunk, &mut y);
        }
        assert_eq!(y.len(), 12_000);
        let rms = (y[2000..10000].iter().map(|v| v * v).sum::<f32>() / 8000.0).sqrt();
        assert!((rms - 0.707).abs() < 0.02, "rms {rms}");
    }
}
