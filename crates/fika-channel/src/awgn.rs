//! White Gaussian noise calibrated the way FT8 reports SNR: the ratio of
//! signal power to the noise power in a 2500 Hz bandwidth.

use rand::Rng;
use rand_distr::{Distribution, Normal};

pub const REFERENCE_BW_HZ: f64 = 2500.0;

/// Noise standard deviation per sample for a given SNR and signal power.
pub fn noise_sigma(snr_db: f64, signal_power: f64, fs: f64) -> f64 {
    let snr = 10f64.powf(snr_db / 10.0);
    let noise_in_ref = signal_power / snr;
    (noise_in_ref * (fs / 2.0) / REFERENCE_BW_HZ).sqrt()
}

/// Mean power of a constant-envelope tone of peak `amplitude`.
pub fn tone_power(amplitude: f64) -> f64 {
    amplitude * amplitude / 2.0
}

pub fn add_awgn<R: Rng>(samples: &mut [f32], snr_db: f64, signal_power: f64, fs: f64, rng: &mut R) {
    let sigma = noise_sigma(snr_db, signal_power, fs);
    let dist = Normal::new(0.0, sigma).unwrap();
    for x in samples.iter_mut() {
        *x += dist.sample(rng) as f32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn zero_db_means_equal_power_in_reference_bandwidth() {
        let fs = 12_000.0;
        let p = tone_power(0.5);
        let mut buf = vec![0f32; 120_000];
        let mut rng = rand::rngs::StdRng::seed_from_u64(1);
        add_awgn(&mut buf, 0.0, p, fs, &mut rng);
        let var = buf.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / buf.len() as f64;
        // Total noise power over fs/2 = 6000 Hz should be p * 6000/2500.
        let expected = p * (fs / 2.0) / REFERENCE_BW_HZ;
        assert!((var / expected - 1.0).abs() < 0.03, "{var} vs {expected}");
    }
}
