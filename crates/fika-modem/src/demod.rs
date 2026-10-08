//! Block demodulation (SPEC §8.2): per-symbol tone energies at the detected
//! time and frequency, hop removal, max-log bit LLRs, deinterleave, LDPC.

use std::f64::consts::PI;

use num_complex::Complex64;

use crate::energy::EnergyMatrix;
use crate::frame_kind::FrameKind;
use crate::hop;
use crate::interleave::deinterleave;
use crate::ldpc::Ldpc;
use crate::params::{BIN_HZ, BITS_PER_SYMBOL, PILOT_SYMBOLS, PREAMBLE_SYMBOLS, TONES, tone_hz};
use crate::profile::Profile;
use crate::symbols::value_bit;
use crate::sync::Detection;

/// Result of decoding one block.
#[derive(Clone, Debug)]
pub struct BlockDecode {
    pub index: usize,
    pub bytes: Option<Vec<u8>>,
    pub iterations: usize,
    /// Mean peak-to-noise ratio over the block's symbols (linear).
    pub es_n0: f32,
}

pub struct Demodulator {
    fs: u32,
    ldpc_long: Ldpc,
    ldpc_short: Ldpc,
    pub max_iters: usize,
}

impl Demodulator {
    pub fn new(fs: u32) -> Self {
        Self {
            fs,
            ldpc_long: Ldpc::new(FrameKind::Long),
            ldpc_short: Ldpc::new(FrameKind::Short),
            max_iters: 50,
        }
    }

    /// Energies of the 16 tones over `sps` samples starting at `start`,
    /// with tone 0 at `f0` Hz. Samples outside the buffer count as zero.
    pub fn tone_energies(&self, samples: &[f32], start: i64, sps: usize, f0: f64) -> [f32; TONES] {
        let mut acc = [Complex64::new(0.0, 0.0); TONES];
        let mut ph = [Complex64::new(1.0, 0.0); TONES];
        let step: Vec<Complex64> = (0..TONES)
            .map(|k| {
                Complex64::from_polar(1.0, -2.0 * PI * (f0 + BIN_HZ * k as f64) / self.fs as f64)
            })
            .collect();
        for n in 0..sps {
            let idx = start + n as i64;
            let x = if idx >= 0 && (idx as usize) < samples.len() {
                samples[idx as usize] as f64
            } else {
                0.0
            };
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
    /// steps of 1/64 symbol and keep the offset that maximises the energy
    /// of the 16 SYNC tones. The frame-grid estimate is only good to a few
    /// percent of a symbol, which costs about a decibel.
    pub fn refine_timing(&self, samples: &[f32], det: &mut Detection) {
        let sps = det
            .profile
            .samples_per_symbol(self.fs)
            .expect("sample rate");
        let f0 = tone_hz(det.lane, 0) + det.freq_offset_hz;
        let seq = det.kind.sync_sequence();
        let step = (sps / 64).max(1) as i64;
        let half = (sps / 8) as i64;
        let mut best = (f32::MIN, 0i64);
        let mut d = -half;
        while d <= half {
            let start = det.start_sample.round() as i64 + d;
            let e: f32 = (0..crate::params::SYNC_SYMBOLS)
                .map(|n| {
                    let s = start + (n * sps) as i64;
                    self.tone_energies(samples, s, sps, f0)[seq[n] as usize]
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
        let profile: Profile = det.profile;
        let sps = profile.samples_per_symbol(self.fs).expect("sample rate");
        let f0 = tone_hz(det.lane, 0) + det.freq_offset_hz;
        let block_start_symbol = PREAMBLE_SYMBOLS + index * kind.block_symbols();
        let m0 = index * kind.block_symbols(); // hop counter at block start

        // Energies per symbol, tone-indexed (hop not yet removed).
        let n_data = kind.data_symbols();
        let mut e_tone_all: Vec<[f32; TONES]> = Vec::with_capacity(n_data);
        for s in 0..n_data {
            let sym = block_start_symbol + PILOT_SYMBOLS + s;
            let start = (det.start_sample + sym as f64 * sps as f64).round() as i64;
            e_tone_all.push(self.tone_energies(samples, start, sps, f0));
        }
        // Per-tone-bin noise level: the data hop puts our signal on any one
        // bin only one symbol in sixteen, so the 75th percentile of a bin's
        // energy over the block measures that bin's noise *and* whatever
        // interferer sits on it. Normalising per bin turns a carrier or
        // keyed CW inside the lane into a quiet bin instead of a winner
        // in every symbol's max-log decision.
        let mut noise_tone = [1f32; TONES];
        let mut col: Vec<f32> = Vec::with_capacity(n_data);
        for (k, nt) in noise_tone.iter_mut().enumerate() {
            col.clear();
            col.extend(e_tone_all.iter().map(|e| e[k]));
            let idx = (col.len() * 3 / 4).min(col.len() - 1);
            let (_, p75, _) = col.select_nth_unstable_by(idx, |a, b| a.total_cmp(b));
            // 75th percentile of an exponential is ln 4 times its mean.
            *nt = (*p75 / 4f32.ln()).max(1e-12);
        }
        // Hop removal and SNR estimate on the normalised energies.
        let mut e_all: Vec<[f32; TONES]> = Vec::with_capacity(n_data);
        let mut peak_sum = 0f64;
        for (s, e_tone) in e_tone_all.iter().enumerate() {
            let m = m0 + PILOT_SYMBOLS + s;
            let mut e_val = [0f32; TONES];
            for (tone, &e) in e_tone.iter().enumerate() {
                e_val[hop::unmap(tone as u8, m, det.phase) as usize] = e / noise_tone[tone];
            }
            peak_sum += e_val.iter().copied().fold(0f32, f32::max) as f64;
            e_all.push(e_val);
        }
        let noise = 1f32;
        let es_n0 = (peak_sum as f32 / n_data as f32 - 1.0).max(0.0);

        // Max-log LLRs, positive = bit 0.
        let mut llrs_air = Vec::with_capacity(n_data * BITS_PER_SYMBOL);
        for e_val in &e_all {
            for b in 0..BITS_PER_SYMBOL {
                let mut m0v = f32::NEG_INFINITY;
                let mut m1v = f32::NEG_INFINITY;
                for (d, &e) in e_val.iter().enumerate() {
                    let s = e / noise;
                    if value_bit(d as u8, b) == 0 {
                        m0v = m0v.max(s);
                    } else {
                        m1v = m1v.max(s);
                    }
                }
                llrs_air.push(m0v - m1v);
            }
        }
        let (rows, cols) = kind.interleaver();
        let llrs = deinterleave(&llrs_air, rows, cols);
        let ldpc = match kind {
            FrameKind::Long => &mut self.ldpc_long,
            FrameKind::Short => &mut self.ldpc_short,
        };
        let (bytes, iterations) = ldpc.decode(&llrs, self.max_iters);
        BlockDecode {
            index,
            bytes,
            iterations,
            es_n0,
        }
    }

    /// Helper for callers that already have an energy matrix: nothing yet,
    /// reserved for successive cancellation.
    pub fn fs(&self) -> u32 {
        self.fs
    }
}

#[allow(dead_code)]
fn _unused(_: &EnergyMatrix) {}
