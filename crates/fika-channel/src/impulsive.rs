//! Impulsive noise: Poisson-timed bursts of white noise (lightning QRN,
//! switching transients).

use rand::Rng;
use rand_distr::{Distribution, Normal};

use crate::spec::Impulsive;

pub fn add<R: Rng>(
    samples: &mut [f32],
    spec: &Impulsive,
    fs: f64,
    reference_amplitude: f64,
    rng: &mut R,
) {
    let sigma = reference_amplitude * 10f64.powf(spec.level_db / 20.0);
    let dist = Normal::new(0.0, sigma).unwrap();
    let len = (spec.duration_ms * 1e-3 * fs).max(1.0) as usize;
    let mean_gap = fs / spec.rate_hz.max(1e-6);
    let mut pos = 0usize;
    loop {
        // Exponential inter-arrival times.
        let u: f64 = rng.random_range(1e-9..1.0);
        pos += (-u.ln() * mean_gap) as usize;
        if pos >= samples.len() {
            break;
        }
        for s in samples.iter_mut().skip(pos).take(len) {
            *s += dist.sample(rng) as f32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn bursts_cover_about_rate_times_duration() {
        let mut x = vec![0f32; 120_000];
        let mut rng = rand::rngs::StdRng::seed_from_u64(2);
        add(
            &mut x,
            &Impulsive {
                rate_hz: 5.0,
                duration_ms: 2.0,
                level_db: 0.0,
            },
            12_000.0,
            0.5,
            &mut rng,
        );
        let hit = x.iter().filter(|v| **v != 0.0).count() as f64 / x.len() as f64;
        // 5/s × 2 ms = 1 % duty.
        assert!(hit > 0.005 && hit < 0.02, "duty {hit}");
    }
}
