//! Watterson HF channel: the analytic input is delayed per path and
//! multiplied by an independent complex Gaussian tap whose Doppler
//! spectrum is Gaussian with the configured spread. Average output power
//! equals input power.

use num_complex::{Complex32, Complex64};
use rand::Rng;
use rand_distr::{Distribution, Normal};
use rustfft::FftPlanner;

use crate::spec::ChannelSpec;

/// Analytic signal via FFT (Hilbert transform). Zero-padded to a power of two.
fn analytic(x: &[f32]) -> Vec<Complex32> {
    let n = x.len().next_power_of_two().max(2);
    let mut buf: Vec<Complex32> = x.iter().map(|&v| Complex32::new(v, 0.0)).collect();
    buf.resize(n, Complex32::new(0.0, 0.0));
    let mut planner = FftPlanner::<f32>::new();
    planner.plan_fft_forward(n).process(&mut buf);
    // Keep DC and Nyquist, double positive frequencies, zero negative.
    for (i, b) in buf.iter_mut().enumerate() {
        if i == 0 || i == n / 2 {
        } else if i < n / 2 {
            *b *= 2.0;
        } else {
            *b = Complex32::new(0.0, 0.0);
        }
    }
    planner.plan_fft_inverse(n).process(&mut buf);
    let scale = 1.0 / n as f32;
    buf.truncate(x.len());
    buf.iter_mut().for_each(|b| *b *= scale);
    buf
}

/// Complex Gaussian tap process with Gaussian Doppler spectrum of 2σ =
/// `spread_hz`, unit mean power, `len` samples at `fs`.
fn tap_process<R: Rng>(len: usize, fs: f64, spread_hz: f64, rng: &mut R) -> Vec<Complex64> {
    if spread_hz <= 0.0 {
        // Static path with a random phase.
        let ph = rng.random_range(0.0..std::f64::consts::TAU);
        return vec![Complex64::from_polar(1.0, ph); len];
    }
    let fs_low = (spread_hz * 50.0).max(20.0);
    let sigma_f = spread_hz / 2.0;
    // h(t) ∝ exp(-4π²σ²t²): amplitude filter whose power spectrum is
    // exp(-f²/(2σ²)).
    let sigma_t = 1.0 / (2.0 * std::f64::consts::SQRT_2 * std::f64::consts::PI * sigma_f);
    let half = (4.0 * sigma_t * fs_low).ceil() as usize;
    let taps: Vec<f64> = (0..=2 * half)
        .map(|i| {
            let t = (i as f64 - half as f64) / fs_low;
            (-t * t / (2.0 * sigma_t * sigma_t)).exp()
        })
        .collect();
    let norm = taps.iter().map(|v| v * v).sum::<f64>().sqrt();
    let n_low = (len as f64 * fs_low / fs).ceil() as usize + 2;
    let dist = Normal::new(0.0, std::f64::consts::FRAC_1_SQRT_2).unwrap();
    let white: Vec<Complex64> = (0..n_low + taps.len())
        .map(|_| Complex64::new(dist.sample(rng), dist.sample(rng)))
        .collect();
    let low: Vec<Complex64> = (0..n_low)
        .map(|k| {
            let mut acc = Complex64::new(0.0, 0.0);
            for (j, &h) in taps.iter().enumerate() {
                acc += white[k + j] * h;
            }
            acc / norm
        })
        .collect();
    // Linear interpolation up to fs.
    (0..len)
        .map(|n| {
            let pos = n as f64 * fs_low / fs;
            let i = pos.floor() as usize;
            let f = pos - i as f64;
            let a = low[i.min(n_low - 1)];
            let b = low[(i + 1).min(n_low - 1)];
            a * (1.0 - f) + b * f
        })
        .collect()
}

/// Pass `x` through the channel: multipath fading, frequency shift and
/// drift, then interferers, impulsive noise and the rig passband.
/// Background noise is not added here (see `add_awgn`). Interferer and
/// impulse levels are relative to `reference_amplitude`, the wanted
/// signal's peak; pass 0.5 for the default transmitter.
pub fn apply_with_reference<R: Rng>(
    spec: &ChannelSpec,
    x: &[f32],
    fs: f64,
    reference_amplitude: f64,
    rng: &mut R,
) -> Vec<f32> {
    let mut y = propagate(spec, x, fs, rng);
    for i in &spec.interferers {
        let r = crate::interferer::render(i, y.len(), fs, reference_amplitude, rng);
        for (a, b) in y.iter_mut().zip(r.iter()) {
            *a += b;
        }
    }
    if let Some(imp) = &spec.impulsive {
        crate::impulsive::add(&mut y, imp, fs, reference_amplitude, rng);
    }
    if spec.bandpass {
        y = crate::filter::ssb_passband(&y, fs);
    }
    y
}

/// `apply_with_reference` with the default transmitter level of 0.5.
pub fn apply<R: Rng>(spec: &ChannelSpec, x: &[f32], fs: f64, rng: &mut R) -> Vec<f32> {
    apply_with_reference(spec, x, fs, 0.5, rng)
}

/// Multipath and frequency shift/drift only.
fn propagate<R: Rng>(spec: &ChannelSpec, x: &[f32], fs: f64, rng: &mut R) -> Vec<f32> {
    if spec.paths.is_empty() && spec.freq_shift_hz == 0.0 && spec.drift_hz_per_s == 0.0 {
        return x.to_vec();
    }
    let xa = analytic(x);
    let n = x.len();
    let mut y = vec![Complex64::new(0.0, 0.0); n];
    if spec.paths.is_empty() {
        for (i, v) in xa.iter().enumerate() {
            y[i] = Complex64::new(v.re as f64, v.im as f64);
        }
    } else {
        let total_gain: f64 = spec
            .paths
            .iter()
            .map(|p| 10f64.powf(p.gain_db / 10.0))
            .sum();
        for p in &spec.paths {
            let delay = (p.delay_ms * 1e-3 * fs).round() as usize;
            let amp = (10f64.powf(p.gain_db / 10.0) / total_gain).sqrt();
            let g = tap_process(n, fs, p.spread_hz, rng);
            for i in delay..n {
                let v = xa[i - delay];
                y[i] += g[i] * Complex64::new(v.re as f64, v.im as f64) * amp;
            }
        }
    }
    let w = 2.0 * std::f64::consts::PI * spec.freq_shift_hz / fs;
    y.iter()
        .enumerate()
        .map(|(i, v)| {
            let r = if w != 0.0 {
                v * Complex64::from_polar(1.0, w * i as f64)
            } else {
                *v
            };
            r.re as f32
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn tone(n: usize, f: f64, fs: f64) -> Vec<f32> {
        (0..n)
            .map(|i| (2.0 * std::f64::consts::PI * f * i as f64 / fs).sin() as f32)
            .collect()
    }

    fn power(x: &[f32]) -> f64 {
        x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len() as f64
    }

    #[test]
    fn analytic_signal_of_sine_has_constant_envelope() {
        let x = tone(4096, 1000.0, 12_000.0);
        let a = analytic(&x);
        for v in &a[200..3800] {
            assert!((v.norm() - 1.0).abs() < 0.02, "{}", v.norm());
        }
    }

    #[test]
    fn average_power_is_preserved_under_fading() {
        let fs = 12_000.0;
        let x = tone(12_000 * 60, 1500.0, fs);
        let mut rng = rand::rngs::StdRng::seed_from_u64(3);
        let y = apply(&ChannelSpec::ccir_moderate(), &x, fs, &mut rng);
        let ratio = power(&y) / power(&x);
        assert!((ratio - 1.0).abs() < 0.25, "power ratio {ratio}");
    }

    #[test]
    fn tap_process_has_unit_power_and_fades_slowly() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(5);
        let g = tap_process(12_000 * 120, 12_000.0, 0.5, &mut rng);
        let p = g.iter().map(|v| v.norm_sqr()).sum::<f64>() / g.len() as f64;
        assert!((p - 1.0).abs() < 0.2, "tap power {p}");
        // Adjacent samples nearly identical.
        assert!((g[1000] - g[1001]).norm() < 1e-2);
    }

    #[test]
    fn frequency_shift_moves_tone() {
        let fs = 12_000.0;
        let x = tone(24_000, 1000.0, fs);
        let mut rng = rand::rngs::StdRng::seed_from_u64(1);
        let spec = ChannelSpec::awgn().with_shift(100.0);
        let y = apply(&spec, &x, fs, &mut rng);
        let mid = &y[4000..20000];
        let crossings = mid
            .windows(2)
            .filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0))
            .count();
        let f = crossings as f64 / 2.0 / (mid.len() as f64 / fs);
        assert!((f - 1100.0).abs() < 3.0, "f {f}");
    }
}
