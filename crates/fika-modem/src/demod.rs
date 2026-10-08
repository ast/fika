//! Block demodulation: per-symbol tone energies at the detected time and
//! frequency, per-bin noise normalisation, interference-aware symbol
//! likelihoods, hop removal, GF(64) LDPC decoding.

use std::f64::consts::PI;

use num_complex::Complex64;

use fika_nb::gf64::Q;
use fika_nb::{Decoder, LikelihoodParams, symbol_likelihoods};

use crate::hop;
use crate::params::{BIN_HZ, PILOT_SYMBOLS, PREAMBLE_SYMBOLS, SYNC_SYMBOLS, TONES, tone_hz};
use crate::symbols::symbols_to_bytes;
use crate::sync::Detection;
use crate::tx::code;

/// Result of decoding one block.
#[derive(Clone, Debug)]
pub struct BlockDecode {
    pub index: usize,
    pub bytes: Option<Vec<u8>>,
    pub iterations: usize,
    /// Mean peak-to-noise ratio over the block's symbols (linear).
    pub es_n0: f32,
    /// Interferer occupancy estimate used for the likelihoods.
    pub q: f32,
}

pub struct Demodulator {
    fs: u32,
    pub max_iters: usize,
}

impl Demodulator {
    pub fn new(fs: u32) -> Self {
        Self { fs, max_iters: 50 }
    }

    pub fn fs(&self) -> u32 {
        self.fs
    }

    /// Energies of the 64 tones over `sps` samples starting at `start`,
    /// with tone 0 at `f0` Hz. Samples outside the buffer count as zero.
    pub fn tone_energies(&self, samples: &[f32], start: i64, sps: usize, f0: f64) -> [f32; TONES] {
        let mut acc = [Complex64::new(0.0, 0.0); TONES];
        let mut ph = [Complex64::new(1.0, 0.0); TONES];
        let step: Vec<Complex64> = (0..TONES)
            .map(|k| {
                Complex64::from_polar(1.0, -2.0 * PI * (f0 + BIN_HZ * k as f64) / self.fs as f64)
            })
            .collect();
        let lo = (start.max(0) as usize).min(samples.len());
        let hi = ((start + sps as i64).max(0) as usize).min(samples.len());
        // Advance the phasors to `lo` if the start was clipped.
        if lo as i64 > start {
            let skip = (lo as i64 - start) as f64;
            for (k, p) in ph.iter_mut().enumerate() {
                *p = Complex64::from_polar(
                    1.0,
                    -2.0 * PI * (f0 + BIN_HZ * k as f64) * skip / self.fs as f64,
                );
            }
        }
        for &x in &samples[lo..hi.max(lo)] {
            let x = x as f64;
            for k in 0..TONES {
                acc[k] += ph[k] * x;
                ph[k] *= step[k];
            }
        }
        let mut e = [0f32; TONES];
        for k in 0..TONES {
            e[k] = acc[k].norm_sqr() as f32;
        }
        e
    }

    /// Refine `det.start_sample` in the sample domain: scan ±1/8 symbol in
    /// steps of 1/64 symbol for the maximum summed energy of the 16 SYNC
    /// tones.
    pub fn refine_timing(&self, samples: &[f32], det: &mut Detection) {
        let sps = det
            .profile
            .samples_per_symbol(self.fs)
            .expect("sample rate");
        let f0 = tone_hz(0) + det.freq_offset_hz;
        let step = (sps / 64).max(1) as i64;
        let half = (sps / 8) as i64;
        let mut best = (f32::MIN, 0i64);
        let mut d = -half;
        while d <= half {
            let start = det.start_sample.round() as i64 + d;
            let e: f32 = (0..SYNC_SYMBOLS)
                .map(|n| {
                    let s = start + (n * sps) as i64;
                    self.tone_energies(samples, s, sps, f0)[det.kind.sync_tone(n) as usize]
                })
                .sum();
            if e > best.0 {
                best = (e, d);
            }
            d += step;
        }
        det.start_sample += best.1 as f64;
    }

    /// Decode block `index` of the burst described by `det`.
    pub fn decode_block(&mut self, samples: &[f32], det: &Detection, index: usize) -> BlockDecode {
        let kind = det.kind;
        let sps = det
            .profile
            .samples_per_symbol(self.fs)
            .expect("sample rate");
        let f0 = tone_hz(0) + det.freq_offset_hz;
        let block_start_symbol = PREAMBLE_SYMBOLS + index * kind.block_symbols();
        let m0 = index * kind.block_symbols();
        let n_data = kind.data_symbols();

        // Tone-indexed energies per data symbol.
        let mut e_tone_all: Vec<[f32; TONES]> = Vec::with_capacity(n_data);
        for s in 0..n_data {
            let sym = block_start_symbol + PILOT_SYMBOLS + s;
            let start = (det.start_sample + sym as f64 * sps as f64).round() as i64;
            e_tone_all.push(self.tone_energies(samples, start, sps, f0));
        }
        // Per-tone-bin noise level: the hop puts the wanted signal on any one
        // bin only one symbol in 64, so the 75th percentile of a bin over the
        // block measures its noise plus whatever interferer sits on it.
        let mut col: Vec<f32> = Vec::with_capacity(n_data);
        for k in 0..TONES {
            col.clear();
            col.extend(e_tone_all.iter().map(|e| e[k]));
            let idx = (col.len() * 3 / 4).min(col.len() - 1);
            let (_, p75, _) = col.select_nth_unstable_by(idx, |a, b| a.total_cmp(b));
            let noise = (*p75 / 4f32.ln()).max(1e-12);
            for e in e_tone_all.iter_mut() {
                e[k] /= noise;
            }
        }
        // Es/N0 and interferer occupancy from the normalised energies.
        let peak_mean = e_tone_all
            .iter()
            .map(|e| e.iter().copied().fold(0f32, f32::max))
            .sum::<f32>()
            / n_data as f32;
        let es_n0 = (peak_mean - 1.0).max(0.0);
        let q = fika_nb::likelihood::estimate_q(&e_tone_all, 4.0);
        let params = LikelihoodParams { gamma: es_n0, q };

        // Likelihoods, hop removed.
        let lik: Vec<[f32; Q]> = e_tone_all
            .iter()
            .enumerate()
            .map(|(s, e)| {
                let m = m0 + PILOT_SYMBOLS + s;
                let p = symbol_likelihoods(e, params);
                let mut out = [0f32; Q];
                for (tone, &v) in p.iter().enumerate() {
                    out[hop::unmap(tone as u8, m, det.phase) as usize] = v;
                }
                out
            })
            .collect();
        let mut dec = Decoder::new(code());
        dec.max_iters = self.max_iters;
        let (cw, iterations) = dec.decode(&lik);
        let bytes = cw.map(|cw| symbols_to_bytes(&code().info_of(&cw)));
        BlockDecode {
            index,
            bytes,
            iterations,
            es_n0,
            q,
        }
    }
}
