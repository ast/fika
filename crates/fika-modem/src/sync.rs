//! Preamble detection: 2-D correlation of the SYNC Costas sequence over the
//! energy matrix (clipped coarse pass), unclipped refinement with peak,
//! median and support tests, local non-maximum suppression, then PHASE
//! readout that may yield several phases when bursts were keyed together.

use crate::energy::EnergyMatrix;
use crate::frame_kind::FrameKind;
use crate::params::{PHASE_SYMBOLS, PHASES, SYNC_SYMBOLS};
use crate::preamble::phase_tone;
use crate::profile::Profile;

#[derive(Clone, Debug)]
pub struct SyncConfig {
    /// Coarse detection threshold on the clipped 16-symbol SYNC sum (noise mean 16, σ 4).
    pub threshold: f32,
    /// Required ratio of the refined SYNC peak to its neighbours one symbol away.
    pub peak_ratio: f32,
    /// Minimum SYNC bins (of 16) at or above a quarter of their mean.
    pub min_support: usize,
    /// Minimum median of the 16 unclipped SYNC cells (noise median ≈ 0.7).
    pub min_median: f32,
    /// A PHASE candidate must reach this fraction of the best one.
    pub phase_fraction: f32,
    /// Frequency search range in tone bins each side of nominal.
    pub search_bins: usize,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            threshold: 40.0,
            peak_ratio: 2.0,
            min_support: 11,
            min_median: 2.5,
            phase_fraction: 0.6,
            search_bins: 2,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Detection {
    pub profile: Profile,
    pub kind: FrameKind,
    pub phase: u8,
    /// Sample index of the first preamble symbol (fractional).
    pub start_sample: f64,
    /// Transmitter frequency offset relative to nominal, Hz.
    pub freq_offset_hz: f64,
    /// Coarse SYNC sum (clipped).
    pub score: f32,
    /// Best over second-best PHASE score.
    pub phase_margin: f32,
    /// Estimated Es/N0 (linear) from the SYNC peaks.
    pub es_n0: f32,
}

impl Detection {
    /// SNR in the 2500 Hz reference bandwidth, dB.
    pub fn snr_db(&self) -> f32 {
        let rs = self.profile.baud() as f32;
        10.0 * (self.es_n0.max(1e-3) * rs / 2500.0).log10()
    }
}

struct Candidate {
    kind: FrameKind,
    frame: usize,
    sub: i64,
    score: f32,
}

/// Run the detector over a computed energy matrix.
pub fn detect(m: &EnergyMatrix, cfg: &SyncConfig) -> Vec<Detection> {
    let fps = EnergyMatrix::FRAMES_PER_SYMBOL as i64;
    let max_sub = (cfg.search_bins * m.sub) as i64;
    let span_frames = (SYNC_SYMBOLS as i64) * fps;
    if (m.frames as i64) < span_frames {
        return Vec::new();
    }
    let base0 = m.tone_bin(0) as i64;
    let cell = |kind: FrameKind, n: usize, sub: i64| -> i64 {
        base0 + sub + kind.sync_tone(n) as i64 * m.sub as i64
    };
    let sync_sum = |kind: FrameKind, frame: i64, sub: i64| -> f32 {
        (0..SYNC_SYMBOLS)
            .map(|n| m.get(frame + n as i64 * fps, cell(kind, n, sub)))
            .sum()
    };
    let fine_sum = |kind: FrameKind, frame: i64, sub: i64| -> f32 {
        (0..SYNC_SYMBOLS)
            .map(|n| m.get_unclipped(frame + n as i64 * fps, cell(kind, n, sub)))
            .sum()
    };

    // Coarse pass.
    let mut cands: Vec<Candidate> = Vec::new();
    for kind in FrameKind::ALL {
        let last_frame = m.frames as i64 - span_frames;
        for frame in 0..=last_frame {
            let mut best = (0.0f32, 0i64);
            for sub in -max_sub..=max_sub {
                let z = sync_sum(kind, frame, sub);
                if z > best.0 {
                    best = (z, sub);
                }
            }
            if best.0 >= cfg.threshold {
                cands.push(Candidate {
                    kind,
                    frame: frame as usize,
                    sub: best.1,
                    score: best.0,
                });
            }
        }
    }

    // Fine pass on unclipped energies.
    struct Fine {
        c: Candidate,
        peak: f32,
        frame: i64,
        sub: i64,
        dt: f64,
        df: f64,
    }
    let mut fine: Vec<Fine> = Vec::new();
    for c in cands {
        let mut best = (f32::MIN, c.frame as i64, c.sub);
        for frame in c.frame as i64 - fps..=c.frame as i64 + fps {
            for sub in -max_sub..=max_sub {
                let z = fine_sum(c.kind, frame, sub);
                if z > best.0 {
                    best = (z, frame, sub);
                }
            }
        }
        let (peak, f, sub) = best;
        let neighbour = fine_sum(c.kind, f - fps, sub).max(fine_sum(c.kind, f + fps, sub));
        if peak < cfg.peak_ratio * neighbour.max(SYNC_SYMBOLS as f32) {
            continue;
        }
        let mut cells: Vec<f32> = (0..SYNC_SYMBOLS)
            .map(|n| m.get_unclipped(f + n as i64 * fps, cell(c.kind, n, sub)))
            .collect();
        let mean_bin = peak / SYNC_SYMBOLS as f32;
        let support = cells.iter().filter(|&&v| v >= 0.25 * mean_bin).count();
        if support < cfg.min_support {
            continue;
        }
        cells.sort_by(|a, b| a.total_cmp(b));
        let median = 0.5 * (cells[7] + cells[8]);
        if median < cfg.min_median {
            continue;
        }
        let dt = parabolic([
            fine_sum(c.kind, f - 1, sub),
            peak,
            fine_sum(c.kind, f + 1, sub),
        ]);
        let df = parabolic([
            fine_sum(c.kind, f, sub - 1),
            peak,
            fine_sum(c.kind, f, sub + 1),
        ]);
        fine.push(Fine {
            c,
            peak,
            frame: f,
            sub,
            dt,
            df,
        });
    }
    fine.sort_by(|a, b| b.peak.total_cmp(&a.peak));

    // Local non-maximum suppression: same kind within ±1 symbol and ±1 tone.
    let mut accepted: Vec<Fine> = Vec::new();
    for x in fine {
        let clash = accepted.iter().any(|a| {
            a.c.kind == x.c.kind
                && (a.frame - x.frame).abs() <= fps
                && (a.sub - x.sub).abs() <= m.sub as i64
        });
        if !clash {
            accepted.push(x);
        }
    }

    let mut out = Vec::new();
    for x in accepted {
        let (c, peak, f, sub) = (x.c, x.peak, x.frame, x.sub);
        let start_sample = (f as f64 + x.dt) * m.hop as f64;
        let freq_offset_hz = m.subbin_hz(sub as f64 + x.df);
        let gamma = (peak / SYNC_SYMBOLS as f32 - 1.0).max(0.0);

        // PHASE readout on unclipped energies with an adaptive clip at
        // twice the mean SYNC peak. Every phase above an absolute bar and a
        // fraction of the best is reported: two bursts keyed within a
        // symbol share one SYNC peak and show two PHASE winners.
        let phase_start = f + SYNC_SYMBOLS as i64 * fps;
        let clip = 2.0 * peak / SYNC_SYMBOLS as f32;
        let mut scores = [0f32; PHASES];
        for (phase, s) in scores.iter_mut().enumerate() {
            *s = (0..PHASE_SYMBOLS)
                .map(|n| {
                    let b = base0 + sub + phase_tone(n, phase as u8) as i64 * m.sub as i64;
                    m.get_unclipped(phase_start + n as i64 * fps, b).min(clip)
                })
                .sum()
        }
        let mut order: Vec<usize> = (0..PHASES).collect();
        order.sort_by(|a, b| scores[*b].total_cmp(&scores[*a]));
        let best = scores[order[0]];
        let second = scores[order[1]];
        let bar = PHASE_SYMBOLS as f32 * (1.0 + 0.5 * gamma);
        for &phase in &order {
            let s = scores[phase];
            if s < bar || s < cfg.phase_fraction * best {
                break;
            }
            out.push(Detection {
                profile: m.profile,
                kind: c.kind,
                phase: phase as u8,
                start_sample,
                freq_offset_hz,
                score: c.score,
                phase_margin: best / second.max(1e-6),
                // Empirical +1.4 dB: grid misalignment and the Gaussian
                // transitions take energy out of the measured bin.
                es_n0: gamma * 1.38,
            });
        }
    }
    out.sort_by(|a, b| a.start_sample.total_cmp(&b.start_sample));
    out
}

/// Peak offset in [-0.5, 0.5] from three samples around a maximum.
fn parabolic(z: [f32; 3]) -> f64 {
    let denom = z[0] - 2.0 * z[1] + z[2];
    if denom.abs() < 1e-9 {
        return 0.0;
    }
    (0.5 * (z[0] - z[2]) / denom).clamp(-0.5, 0.5) as f64
}
