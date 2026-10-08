//! Symbol likelihoods from 64 tone energies, interference-aware.
//!
//! Per tone, with the energy already normalised to unit noise mean, the
//! non-coherent evidence that a tone of symbol SNR γ is present is
//! ℓ_t = ln I0(2·sqrt(γ·e_t)) − γ. Other senders put signal-level peaks on
//! other tones; modelling each other tone as carrying a peak with
//! probability q gives the symbol metric
//! L_t = ℓ_t − ln(1 − q + q·e^{ℓ_t}), which saturates at ln(1/q): two
//! equal peaks split the posterior, and a much stronger peak gets no more
//! credit than our own. γ is capped because the exact metric becomes
//! hypersensitive to leakage at high SNR.

use crate::gf64::Q;

#[derive(Clone, Copy, Debug)]
pub struct LikelihoodParams {
    /// Symbol SNR estimate (linear), capped internally at `GAMMA_CAP`.
    pub gamma: f32,
    /// Probability that an unrelated tone carries an interferer peak.
    pub q: f32,
}

/// 15 dB.
pub const GAMMA_CAP: f32 = 31.6;

/// ln I0(z) for z ≥ 0.
pub fn ln_i0(z: f64) -> f64 {
    if z < 3.75 {
        let t = (z / 3.75).powi(2);
        let i0 = 1.0
            + t * (3.5156229
                + t * (3.0899424
                    + t * (1.2067492 + t * (0.2659732 + t * (0.0360768 + t * 0.0045813)))));
        i0.ln()
    } else {
        let t = 3.75 / z;
        let s = 0.39894228
            + t * (0.01328592
                + t * (0.00225319
                    + t * (-0.00157565
                        + t * (0.00916281
                            + t * (-0.02057706
                                + t * (0.02635537 + t * (-0.01647633 + t * 0.00392377)))))));
        z + (s / z.sqrt()).ln()
    }
}

/// Probabilities (sum 1) over the 64 symbol values from normalised energies.
pub fn symbol_likelihoods(e: &[f32; Q], p: LikelihoodParams) -> [f32; Q] {
    let gamma = p.gamma.clamp(0.1, GAMMA_CAP) as f64;
    let q = p.q.clamp(1.0 / Q as f32, 0.5) as f64;
    let mut l = [0f64; Q];
    let mut max = f64::MIN;
    for t in 0..Q {
        let ell = ln_i0(2.0 * (gamma * e[t].max(0.0) as f64).sqrt()) - gamma;
        let lt = if ell > 0.0 {
            -(q + (1.0 - q) * (-ell).exp()).ln()
        } else {
            ell - (1.0 - q + q * ell.exp()).ln()
        };
        l[t] = lt;
        max = max.max(lt);
    }
    let mut out = [0f32; Q];
    let mut sum = 0f64;
    for t in 0..Q {
        let v = (l[t] - max).exp();
        out[t] = v as f32;
        sum += v;
    }
    for v in out.iter_mut() {
        *v /= sum as f32;
    }
    out
}

/// Interferer occupancy estimate for a block: cells above `hot` times the
/// noise, minus the one wanted peak per symbol, over all cells.
pub fn estimate_q(energies: &[[f32; Q]], hot: f32) -> f32 {
    let n = energies.len().max(1);
    let hot_cells: usize = energies
        .iter()
        .map(|e| e.iter().filter(|&&v| v > hot).count())
        .sum();
    let extra = hot_cells.saturating_sub(n) as f32;
    (extra / (Q * n) as f32).max(1.0 / Q as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ln_i0_matches_known_values() {
        assert!((ln_i0(0.0)).abs() < 1e-6);
        assert!((ln_i0(1.0) - 0.235914).abs() < 1e-4);
        assert!((ln_i0(10.0) - 7.9430).abs() < 2e-3);
    }

    #[test]
    fn two_equal_peaks_split_the_posterior() {
        let mut e = [1f32; Q];
        e[3] = 12.0;
        e[40] = 12.0;
        let p = symbol_likelihoods(
            &e,
            LikelihoodParams {
                gamma: 10.0,
                q: 0.05,
            },
        );
        assert!((p[3] - p[40]).abs() < 1e-3);
        assert!(p[3] > 0.45 && p[3] < 0.5, "{}", p[3]);
    }

    #[test]
    fn stronger_peak_does_not_win_outright() {
        let mut e = [1f32; Q];
        e[3] = 12.0; // ours
        e[40] = 120.0; // +10 dB interferer
        let p = symbol_likelihoods(
            &e,
            LikelihoodParams {
                gamma: 10.0,
                q: 0.05,
            },
        );
        assert!(p[3] > 0.3, "ours {}", p[3]);
    }
}
