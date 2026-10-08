//! Sample-clock error: resample by (1 + ppm·1e-6) with linear interpolation.

pub fn resample_ppm(input: &[f32], ppm: f64) -> Vec<f32> {
    if ppm == 0.0 || input.len() < 2 {
        return input.to_vec();
    }
    let ratio = 1.0 + ppm * 1e-6;
    let n_out = ((input.len() - 1) as f64 / ratio).floor() as usize;
    (0..n_out)
        .map(|k| {
            let pos = k as f64 * ratio;
            let i = pos.floor() as usize;
            let frac = (pos - i as f64) as f32;
            input[i] * (1.0 - frac) + input[i + 1] * frac
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_scales_with_ppm() {
        let x = vec![0f32; 1_000_001];
        assert_eq!(resample_ppm(&x, 100.0).len(), 999_900);
        assert_eq!(resample_ppm(&x, 0.0).len(), 1_000_001);
    }
}
