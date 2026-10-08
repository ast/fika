//! Offline receiver facade: samples in, detections and decoded blocks out.

use crate::demod::{BlockDecode, Demodulator};
use crate::energy::EnergyMatrix;
use crate::params::RX_SAMPLE_RATE;
use crate::profile::Profile;
use crate::sync::{Detection, SyncConfig, detect};

pub struct Receiver {
    pub cfg: SyncConfig,
    demod: Demodulator,
}

impl Default for Receiver {
    fn default() -> Self {
        Self::new()
    }
}

impl Receiver {
    pub fn new() -> Self {
        Self {
            cfg: SyncConfig::default(),
            demod: Demodulator::new(RX_SAMPLE_RATE),
        }
    }

    pub fn fs(&self) -> u32 {
        RX_SAMPLE_RATE
    }

    /// Find every burst start in `samples` (12 kHz), both profiles, with
    /// timing refined in the sample domain.
    pub fn detect(&self, samples: &[f32]) -> Vec<Detection> {
        let mut out = Vec::new();
        for profile in Profile::ALL {
            let mut m = EnergyMatrix::new(profile);
            m.compute(samples);
            for mut det in detect(&m, &self.cfg) {
                self.demod.refine_timing(samples, &mut det);
                out.push(det);
            }
        }
        out.sort_by(|a, b| a.start_sample.total_cmp(&b.start_sample));
        out
    }

    pub fn decode_block(&mut self, samples: &[f32], det: &Detection, index: usize) -> BlockDecode {
        self.demod.decode_block(samples, det, index)
    }

    pub fn decode_blocks(
        &mut self,
        samples: &[f32],
        det: &Detection,
        n: usize,
    ) -> Vec<BlockDecode> {
        (0..n).map(|i| self.decode_block(samples, det, i)).collect()
    }
}
