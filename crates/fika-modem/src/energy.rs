//! Time × frequency energy matrix and per-bin baseline normalisation
//! (SPEC §10 steps 2–5).

use num_complex::Complex32;
use rustfft::{Fft, FftPlanner};
use std::sync::Arc;

use crate::params::{BIN_HZ, RX_SAMPLE_RATE, tone_hz};
use crate::profile::Profile;

/// Energy matrix for one profile. Row `f` covers samples
/// `[f*hop, f*hop + window)`; columns are FFT bins of `bin_hz` each.
pub struct EnergyMatrix {
    pub profile: Profile,
    pub fs: u32,
    pub window: usize,
    pub nfft: usize,
    pub hop: usize,
    pub nbins: usize,
    pub frames: usize,
    /// Sub-bins per 31.25 Hz tone.
    pub sub: usize,
    raw: Vec<f32>,
    /// Normalised energies: unit mean under noise, unclipped.
    norm: Vec<f32>,
    fft: Arc<dyn Fft<f32>>,
}

impl EnergyMatrix {
    /// Frames per symbol period (hop = window / 4).
    pub const FRAMES_PER_SYMBOL: usize = 4;

    pub fn geometry(profile: Profile, fs: u32) -> (usize, usize, usize, usize) {
        let window = profile.samples_per_symbol(fs).expect("rx sample rate");
        // Zero-padding factor: 4 for fast (7.8125 Hz bins), 2 for slow
        // (3.125 Hz bins), SPEC §10.
        let pad = match profile {
            Profile::Fast => 4,
            Profile::Slow => 2,
        };
        let nfft = window * pad;
        let hop = window / Self::FRAMES_PER_SYMBOL;
        let sub = (nfft as f64 * BIN_HZ / fs as f64).round() as usize;
        (window, nfft, hop, sub)
    }

    pub fn new(profile: Profile) -> Self {
        let fs = RX_SAMPLE_RATE;
        let (window, nfft, hop, sub) = Self::geometry(profile, fs);
        let fft = FftPlanner::<f32>::new().plan_fft_forward(nfft);
        Self {
            profile,
            fs,
            window,
            nfft,
            hop,
            nbins: nfft / 2 + 1,
            frames: 0,
            sub,
            raw: Vec::new(),
            norm: Vec::new(),
            fft,
        }
    }

    pub fn bin_hz(&self) -> f64 {
        self.fs as f64 / self.nfft as f64
    }

    /// Column index of `tone` in `lane` at zero offset.
    pub fn tone_bin(&self, lane: usize, tone: usize) -> usize {
        let hz = tone_hz(lane, tone);
        let b = hz / self.bin_hz();
        debug_assert!((b - b.round()).abs() < 1e-6);
        b.round() as usize
    }

    /// Compute the matrix for a whole buffer (offline use).
    pub fn compute(&mut self, samples: &[f32]) {
        self.frames = if samples.len() >= self.window {
            (samples.len() - self.window) / self.hop + 1
        } else {
            0
        };
        self.raw = vec![0.0; self.frames * self.nbins];
        let mut buf = vec![Complex32::new(0.0, 0.0); self.nfft];
        let mut scratch = vec![Complex32::new(0.0, 0.0); self.fft.get_inplace_scratch_len()];
        for f in 0..self.frames {
            let start = f * self.hop;
            for (i, b) in buf.iter_mut().enumerate() {
                *b = if i < self.window {
                    Complex32::new(samples[start + i], 0.0)
                } else {
                    Complex32::new(0.0, 0.0)
                };
            }
            self.fft.process_with_scratch(&mut buf, &mut scratch);
            let row = &mut self.raw[f * self.nbins..(f + 1) * self.nbins];
            for (i, b) in buf[..self.nbins].iter().enumerate() {
                row[i] = b.norm_sqr();
            }
        }
        self.normalise();
    }

    /// Per-bin baseline: 30th percentile over blocks of about 8 s (fast) or
    /// 40 s (slow), scaled so that noise-only bins have unit mean.
    fn normalise(&mut self) {
        const P: f64 = 0.30;
        // 30th percentile of Exp(1) is -ln(0.7).
        let p_to_mean = 1.0 / (-(1.0f64 - P).ln());
        let block = 1000usize.min(self.frames.max(1));
        self.norm = vec![0.0; self.raw.len()];
        // Floor for digitally silent input: a thousandth of the global mean
        // keeps peaks finite and sidelobes below the clip level.
        let global_mean =
            self.raw.iter().map(|&v| v as f64).sum::<f64>() / self.raw.len().max(1) as f64;
        let floor = (global_mean * 1e-3).max(1e-20) as f32;
        let mut col = Vec::with_capacity(block);
        let mut start = 0;
        while start < self.frames {
            let end = (start + block).min(self.frames);
            for bin in 0..self.nbins {
                col.clear();
                col.extend((start..end).map(|f| self.raw[f * self.nbins + bin]));
                let k = ((col.len() as f64 - 1.0) * P).round() as usize;
                let col_max = col.iter().copied().fold(0f32, f32::max);
                let (_, p30, _) = col.select_nth_unstable_by(k, |a, b| a.total_cmp(b));
                // Per-bin floor 30 dB under the bin's own peak: invisible
                // under noise, but for a clean strong signal it keeps
                // spectral leakage below the clip so sync keeps its shape.
                let base = (*p30 as f64 * p_to_mean)
                    .max(floor as f64)
                    .max(col_max as f64 * 1e-3) as f32;
                for f in start..end {
                    let i = f * self.nbins + bin;
                    self.norm[i] = self.raw[i] / base;
                }
            }
            start = end;
        }
    }

    /// Clip applied to detection sums so one strong bin cannot trigger alone.
    pub const CLIP: f32 = 20.0;

    /// Normalised energy, clipped at `CLIP`.
    #[inline]
    pub fn at(&self, frame: usize, bin: usize) -> f32 {
        self.norm[frame * self.nbins + bin].min(Self::CLIP)
    }

    /// Clipped normalised energy, or 0 outside the matrix.
    #[inline]
    pub fn get(&self, frame: i64, bin: i64) -> f32 {
        self.get_unclipped(frame, bin).min(Self::CLIP)
    }

    /// Unclipped normalised energy, or 0 outside the matrix. Used for peak
    /// refinement and SNR estimation, where strong signals must keep their
    /// shape.
    #[inline]
    pub fn get_unclipped(&self, frame: i64, bin: i64) -> f32 {
        if frame < 0 || bin < 0 || frame as usize >= self.frames || bin as usize >= self.nbins {
            0.0
        } else {
            self.norm[frame as usize * self.nbins + bin as usize]
        }
    }

    pub fn raw_at(&self, frame: usize, bin: usize) -> f32 {
        self.raw[frame * self.nbins + bin]
    }

    /// Frequency offset in Hz for a sub-bin offset.
    pub fn subbin_hz(&self, sub: f64) -> f64 {
        sub * BIN_HZ / self.sub as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_matches_spec() {
        let m = EnergyMatrix::new(Profile::Fast);
        assert_eq!((m.window, m.nfft, m.hop), (384, 1536, 96));
        assert!((m.bin_hz() - 7.8125).abs() < 1e-9);
        assert_eq!(m.tone_bin(0, 0), 56);
        assert_eq!(m.tone_bin(1, 3), 56 + 72 + 12);
        let s = EnergyMatrix::new(Profile::Slow);
        assert_eq!((s.window, s.nfft, s.hop), (1920, 3840, 480));
        assert_eq!(s.tone_bin(0, 0), 140);
    }

    #[test]
    fn tone_shows_up_in_its_bin() {
        let x = crate::gfsk::synthesize(&[5u8; 40], 2, Profile::Fast, 12_000, 0.3, 0.0).unwrap();
        let mut m = EnergyMatrix::new(Profile::Fast);
        m.compute(&x);
        let bin = m.tone_bin(2, 5);
        let mid = m.frames / 2;
        assert!(m.raw_at(mid, bin) > 100.0 * m.raw_at(mid, bin + 8));
    }
}
