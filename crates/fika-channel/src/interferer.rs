//! Synthetic interferers at a level relative to the wanted signal.

use std::f64::consts::PI;

use rand::Rng;

use crate::spec::Interferer;

fn amplitude(level_db: f64, reference_amplitude: f64) -> f64 {
    reference_amplitude * 10f64.powf(level_db / 20.0)
}

/// Render `n` samples of the interferer at `fs`, amplitude relative to
/// `reference_amplitude` (the wanted signal's peak).
pub fn render<R: Rng>(
    i: &Interferer,
    n: usize,
    fs: f64,
    reference_amplitude: f64,
    rng: &mut R,
) -> Vec<f32> {
    match *i {
        Interferer::Carrier { freq_hz, level_db } => {
            let a = amplitude(level_db, reference_amplitude);
            (0..n)
                .map(|k| (a * (2.0 * PI * freq_hz * k as f64 / fs).sin()) as f32)
                .collect()
        }
        Interferer::Cw {
            freq_hz,
            level_db,
            wpm,
        } => {
            let a = amplitude(level_db, reference_amplitude);
            let dit = (1.2 / wpm * fs) as usize;
            let edge = (0.005 * fs) as usize;
            let mut out = vec![0f32; n];
            let mut pos = 0usize;
            while pos < n {
                // Element: dit or dah, then a gap of one or three dits.
                let on = if rng.random_bool(0.5) { dit } else { 3 * dit };
                let off = if rng.random_bool(0.7) { dit } else { 3 * dit };
                for k in 0..on.min(n - pos) {
                    let env = if k < edge {
                        0.5 - 0.5 * (PI * k as f64 / edge as f64).cos()
                    } else if k + edge > on {
                        0.5 - 0.5 * (PI * (on - k) as f64 / edge as f64).cos()
                    } else {
                        1.0
                    };
                    let t = (pos + k) as f64 / fs;
                    out[pos + k] = (a * env * (2.0 * PI * freq_hz * t).sin()) as f32;
                }
                pos += on + off;
            }
            out
        }
        Interferer::Rtty {
            center_hz,
            level_db,
        } => {
            let a = amplitude(level_db, reference_amplitude);
            let baud = 45.45;
            let spb = fs / baud;
            let mut phase = 0.0f64;
            let mut bit = false;
            let mut next = 0.0f64;
            (0..n)
                .map(|k| {
                    if k as f64 >= next {
                        bit = rng.random_bool(0.5);
                        next += spb;
                    }
                    let f = center_hz + if bit { 85.0 } else { -85.0 };
                    phase += 2.0 * PI * f / fs;
                    (a * phase.sin()) as f32
                })
                .collect()
        }
        Interferer::Psk31 { freq_hz, level_db } => {
            let a = amplitude(level_db, reference_amplitude);
            let spb = fs / 31.25;
            let mut sign = 1.0f64;
            let mut next_sign = 1.0f64;
            let mut boundary = 0.0f64;
            (0..n)
                .map(|k| {
                    let t = k as f64;
                    if t >= boundary {
                        sign = next_sign;
                        next_sign = if rng.random_bool(0.5) { -sign } else { sign };
                        boundary += spb;
                    }
                    // Cosine envelope dips to zero at a reversal.
                    let frac = (boundary - t) / spb; // 1 at symbol start → 0 at end
                    let env = if next_sign != sign {
                        (PI * (1.0 - frac) / 2.0).cos()
                    } else {
                        1.0
                    };
                    (a * sign * env * (2.0 * PI * freq_hz * t / fs).sin()) as f32
                })
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn power(x: &[f32]) -> f64 {
        x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len() as f64
    }

    #[test]
    fn levels_are_relative_to_reference() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(1);
        let c = render(
            &Interferer::Carrier {
                freq_hz: 1000.0,
                level_db: 0.0,
            },
            12_000,
            12_000.0,
            0.5,
            &mut rng,
        );
        assert!((power(&c) - 0.125).abs() < 0.01);
        let r = render(
            &Interferer::Rtty {
                center_hz: 1500.0,
                level_db: -6.0,
            },
            12_000,
            12_000.0,
            0.5,
            &mut rng,
        );
        assert!((power(&r) / 0.125 - 0.25).abs() < 0.05);
        let p = render(
            &Interferer::Psk31 {
                freq_hz: 1200.0,
                level_db: 0.0,
            },
            60_000,
            12_000.0,
            0.5,
            &mut rng,
        );
        assert!(power(&p) > 0.06 && power(&p) < 0.125);
        let cw = render(
            &Interferer::Cw {
                freq_hz: 800.0,
                level_db: 0.0,
                wpm: 25.0,
            },
            60_000,
            12_000.0,
            0.5,
            &mut rng,
        );
        assert!(power(&cw) > 0.03 && power(&cw) < 0.1);
    }
}
