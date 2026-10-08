//! Preamble detection (SPEC §6.2–§6.4): 2-D correlation of the SYNC Costas
//! sequence over the energy matrix, peak refinement, then PHASE readout.

use crate::energy::EnergyMatrix;
use crate::frame_kind::FrameKind;
use crate::params::{LANES, PHASE_SYMBOLS, SYNC_SYMBOLS, TONES};
use crate::preamble::phase_tone;
use crate::profile::Profile;

#[derive(Clone, Debug)]
pub struct SyncConfig {
    /// Detection threshold on the 16-symbol SYNC sum (noise mean 16, σ 4).
    pub threshold: f32,
    /// Required ratio between best and second-best PHASE candidate.
    pub phase_ratio: f32,
    /// Frequency search range in tone bins each side of nominal.
    pub search_bins: usize,
    /// Required ratio between the refined SYNC peak and the larger of the
    /// sums one symbol earlier and later.
    pub peak_ratio: f32,
    /// Minimum number of the 16 SYNC bins at or above a quarter of their mean.
    pub min_support: usize,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            threshold: 40.0,
            phase_ratio: 1.5,
            search_bins: 2,
            peak_ratio: 2.0,
            min_support: 9,
        }
    }
}

/// A detected burst start.
#[derive(Clone, Debug, PartialEq)]
pub struct Detection {
    pub lane: usize,
    pub profile: Profile,
    pub kind: FrameKind,
    pub phase: u8,
    /// Sample index of the first preamble symbol (fractional).
    pub start_sample: f64,
    /// Transmitter frequency offset relative to nominal, Hz.
    pub freq_offset_hz: f64,
    /// SYNC correlation sum (noise mean 16).
    pub score: f32,
    /// Second-best over best PHASE candidate ratio, for diagnostics.
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
    lane: usize,
    kind: FrameKind,
    frame: usize,
    sub: i64,
    score: f32,
}

/// Run the detector over a computed energy matrix.
///
/// 1. Coarse: clipped SYNC sums over every frame, lane, kind and frequency
///    offset; keep those above the threshold.
/// 2. Fine: for each coarse candidate, re-find the peak on unclipped
///    energies within ±1 symbol and the full frequency range, and require
///    it to stand out against the sums one symbol earlier and later. A
///    strong or clean signal saturates the clipped sums for any sequence;
///    only a real preamble has a sharp unclipped peak.
/// 3. Non-maximum suppression per lane by unclipped peak, then PHASE.
pub fn detect(m: &EnergyMatrix, cfg: &SyncConfig) -> Vec<Detection> {
    let fps = EnergyMatrix::FRAMES_PER_SYMBOL as i64;
    let max_sub = (cfg.search_bins * m.sub) as i64;
    let span_frames = (SYNC_SYMBOLS as i64) * fps;
    if (m.frames as i64) < span_frames {
        return Vec::new();
    }
    let sync_sum = |lane: usize, seq: &[u8; 16], frame: i64, sub: i64| -> f32 {
        let base = m.tone_bin(lane, 0) as i64 + sub;
        (0..SYNC_SYMBOLS as i64)
            .map(|n| {
                m.get(
                    frame + n * fps,
                    base + seq[n as usize] as i64 * m.sub as i64,
                )
            })
            .sum()
    };
    let fine_sum = |lane: usize, seq: &[u8; 16], frame: i64, sub: i64| -> f32 {
        let base = m.tone_bin(lane, 0) as i64 + sub;
        (0..SYNC_SYMBOLS as i64)
            .map(|n| {
                m.get_unclipped(
                    frame + n * fps,
                    base + seq[n as usize] as i64 * m.sub as i64,
                )
            })
            .sum()
    };

    // Coarse pass.
    let mut cands: Vec<Candidate> = Vec::new();
    for lane in 0..LANES {
        for kind in FrameKind::ALL {
            let seq = kind.sync_sequence();
            let last_frame = m.frames as i64 - span_frames;
            for frame in 0..=last_frame {
                let mut best = (0.0f32, 0i64);
                for sub in -max_sub..=max_sub {
                    let z = sync_sum(lane, seq, frame, sub);
                    if z > best.0 {
                        best = (z, sub);
                    }
                }
                if best.0 >= cfg.threshold {
                    cands.push(Candidate {
                        lane,
                        kind,
                        frame: frame as usize,
                        sub: best.1,
                        score: best.0,
                    });
                }
            }
        }
    }

    // Fine pass: refine on unclipped energies and test peak sharpness.
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
        let seq = c.kind.sync_sequence();
        let mut best = (f32::MIN, c.frame as i64, c.sub);
        for frame in c.frame as i64 - fps..=c.frame as i64 + fps {
            for sub in -max_sub..=max_sub {
                let z = fine_sum(c.lane, seq, frame, sub);
                if z > best.0 {
                    best = (z, frame, sub);
                }
            }
        }
        let (peak, f, sub) = best;
        let neighbour =
            fine_sum(c.lane, seq, f - fps, sub).max(fine_sum(c.lane, seq, f + fps, sub));
        if peak < cfg.peak_ratio * neighbour.max(SYNC_SYMBOLS as f32) {
            continue;
        }
        // Support: a real preamble lights most of its 16 bins; a chance
        // match of data symbols lights only a handful, however strong.
        let mean_bin = peak / SYNC_SYMBOLS as f32;
        let base = m.tone_bin(c.lane, 0) as i64 + sub;
        let support = (0..SYNC_SYMBOLS as i64)
            .filter(|&n| {
                m.get_unclipped(f + n * fps, base + seq[n as usize] as i64 * m.sub as i64)
                    >= 0.25 * mean_bin
            })
            .count();
        if support < cfg.min_support {
            continue;
        }
        let dt = parabolic([
            fine_sum(c.lane, seq, f - 1, sub),
            peak,
            fine_sum(c.lane, seq, f + 1, sub),
        ]);
        let df = parabolic([
            fine_sum(c.lane, seq, f, sub - 1),
            peak,
            fine_sum(c.lane, seq, f, sub + 1),
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

    // Greedy non-maximum suppression: one detection per lane per preamble span.
    let mut accepted: Vec<Fine> = Vec::new();
    for x in fine {
        let clash = accepted
            .iter()
            .any(|a| a.c.lane == x.c.lane && (a.frame - x.frame).abs() < span_frames);
        if !clash {
            accepted.push(x);
        }
    }

    let mut out = Vec::new();
    for x in accepted {
        let (c, peak, f, sub) = (x.c, x.peak, x.frame, x.sub);
        let start_sample = (f as f64 + x.dt) * m.hop as f64;
        let freq_offset_hz = m.subbin_hz(sub as f64 + x.df);

        // PHASE readout at the refined peak, on unclipped energies with an
        // adaptive clip at twice the mean SYNC peak: a true-phase bin passes
        // unclipped while an interferer cannot dominate a wrong phase.
        let base = m.tone_bin(c.lane, 0) as i64 + sub;
        let phase_start = f + SYNC_SYMBOLS as i64 * fps;
        let clip = 2.0 * peak / SYNC_SYMBOLS as f32;
        let mut scores = [0f32; TONES];
        for (phase, s) in scores.iter_mut().enumerate() {
            *s = (0..PHASE_SYMBOLS as i64)
                .map(|n| {
                    m.get_unclipped(
                        phase_start + n * fps,
                        base + phase_tone(n as usize, phase as u8) as i64 * m.sub as i64,
                    )
                    .min(clip)
                })
                .sum()
        }
        let (best_phase, best) = scores
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, &s)| (i, s))
            .unwrap();
        let second = scores
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != best_phase)
            .map(|(_, &s)| s)
            .fold(0f32, f32::max);
        let phase_margin = best / second.max(1e-6);
        if phase_margin < cfg.phase_ratio {
            continue;
        }
        // Empirical +1.4 dB: grid misalignment and the Gaussian transitions
        // take energy out of the measured bin.
        let es_n0 = (peak / SYNC_SYMBOLS as f32 - 1.0).max(0.0) * 1.38;
        out.push(Detection {
            lane: c.lane,
            profile: m.profile,
            kind: c.kind,
            phase: best_phase as u8,
            start_sample,
            freq_offset_hz,
            score: c.score,
            phase_margin,
            es_n0,
        });
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
