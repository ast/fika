//! LDPC encode and soft decode on CCSDS TC codes via `labrador-ldpc`
//! (SPEC §8). LLR convention: positive means the bit is more likely 0.

use labrador_ldpc::LDPCCode;

use crate::frame_kind::FrameKind;
use crate::symbols::bytes_to_bits;

pub struct Ldpc {
    code: LDPCCode,
    working: Vec<f32>,
    working_u8: Vec<u8>,
    output: Vec<u8>,
}

impl Ldpc {
    pub fn new(kind: FrameKind) -> Self {
        let code = kind.code();
        Self {
            code,
            working: vec![0.0; code.decode_ms_working_len()],
            working_u8: vec![0u8; code.decode_ms_working_u8_len()],
            output: vec![0u8; code.output_len()],
        }
    }

    pub fn info_bytes(&self) -> usize {
        self.code.k() / 8
    }

    pub fn coded_bits(&self) -> usize {
        self.code.n()
    }

    /// Systematic encode: returns `n` coded bits (0/1), info bits first.
    pub fn encode_bits(&self, info: &[u8]) -> Vec<u8> {
        assert_eq!(info.len(), self.info_bytes());
        let mut codeword = vec![0u8; self.code.n() / 8];
        self.code.copy_encode(info, &mut codeword);
        bytes_to_bits(&codeword)
    }

    /// Min-sum decode. Returns the info bytes if all parity checks are
    /// satisfied within `max_iters`, plus the iteration count.
    pub fn decode(&mut self, llrs: &[f32], max_iters: usize) -> (Option<Vec<u8>>, usize) {
        assert_eq!(llrs.len(), self.code.n());
        let (ok, iters) = self.code.decode_ms(
            llrs,
            &mut self.output,
            &mut self.working,
            &mut self.working_u8,
            max_iters,
        );
        let bytes = ok.then(|| self.output[..self.info_bytes()].to_vec());
        (bytes, iters)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hard_llrs(bits: &[u8]) -> Vec<f32> {
        bits.iter()
            .map(|&b| if b == 0 { 1.0 } else { -1.0 })
            .collect()
    }

    #[test]
    fn roundtrip_clean_and_with_erasures() {
        for kind in FrameKind::ALL {
            let mut ldpc = Ldpc::new(kind);
            let info: Vec<u8> = (0..ldpc.info_bytes() as u8)
                .map(|i| i.wrapping_mul(37) ^ 0x5A)
                .collect();
            let bits = ldpc.encode_bits(&info);
            assert_eq!(bits.len(), kind.coded_bits());
            assert_eq!(&bits[..8], &bytes_to_bits(&info[..1])[..]);

            let llrs = hard_llrs(&bits);
            let (out, _) = ldpc.decode(&llrs, 20);
            assert_eq!(out.unwrap(), info);

            // 20 % random erasures plus 2 % half-confidence flips.
            let mut seed = 7u64;
            let mut rnd = || {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                (seed >> 33) as u32 % 100
            };
            let mut noisy = llrs.clone();
            for l in noisy.iter_mut() {
                let r = rnd();
                if r < 20 {
                    *l = 0.0;
                } else if r < 22 {
                    *l = -*l * 0.5;
                }
            }
            let (out, iters) = ldpc.decode(&noisy, 50);
            assert_eq!(
                out.expect("decode with erasures"),
                info,
                "{kind:?} after {iters} iters"
            );
        }
    }

    #[test]
    #[ignore = "prints decoder tolerance table"]
    fn probe_erasure_tolerance() {
        for kind in FrameKind::ALL {
            let mut ldpc = Ldpc::new(kind);
            let info: Vec<u8> = (0..ldpc.info_bytes() as u8)
                .map(|i| i.wrapping_mul(37) ^ 0x5A)
                .collect();
            let bits = ldpc.encode_bits(&info);
            for &(erase_pct, flip_pct) in &[(10, 0), (25, 0), (40, 0), (0, 5), (25, 3), (33, 3)] {
                let mut seed = 12345u64;
                let mut rnd = || {
                    seed = seed
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407);
                    (seed >> 33) as u32 % 100
                };
                let mut ok = 0;
                let trials = 20;
                for _ in 0..trials {
                    let mut llrs = hard_llrs(&bits);
                    for l in llrs.iter_mut() {
                        let r = rnd();
                        if r < erase_pct {
                            *l = 0.0;
                        } else if r < erase_pct + flip_pct {
                            *l = -*l;
                        }
                    }
                    let (out, _) = ldpc.decode(&llrs, 100);
                    if out.as_deref() == Some(&info[..]) {
                        ok += 1;
                    }
                }
                eprintln!("{kind:?} erase {erase_pct}% flip {flip_pct}%: {ok}/{trials}");
            }
        }
    }
}
