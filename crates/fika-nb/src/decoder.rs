//! Sum-product decoding over GF(64) in the probability domain.
//!
//! Variable-to-check messages are probability vectors over the 64 field
//! elements. A check node constrains Σ coef_i · x_i = 0; after permuting
//! each incoming vector by its edge coefficient the constraint is a plain
//! XOR over GF(2)^6, and the Walsh–Hadamard transform turns XOR-convolution
//! into pointwise products. Check nodes run in f64: the product's DC term
//! is 1, so small probabilities are differences of nearly equal numbers.

use crate::code::NbCode;
use crate::gf64::Q;

pub struct Decoder<'a> {
    code: &'a NbCode,
    v2c: Vec<[f32; Q]>,
    c2v: Vec<[f32; Q]>,
    perms: Vec<[u8; Q]>,
    scratch: Vec<[f64; Q]>,
    pub max_iters: usize,
}

/// In-place Walsh–Hadamard transform of length 64 (unnormalised).
fn fwht(a: &mut [f64; Q]) {
    let mut h = 1;
    while h < Q {
        let mut i = 0;
        while i < Q {
            for j in i..i + h {
                let x = a[j];
                let y = a[j + h];
                a[j] = x + y;
                a[j + h] = x - y;
            }
            i += 2 * h;
        }
        h *= 2;
    }
}

fn normalise32(a: &mut [f32; Q]) {
    let s: f32 = a.iter().sum();
    if s > 0.0 && s.is_finite() {
        for v in a.iter_mut() {
            *v /= s;
        }
    } else {
        a.fill(1.0 / Q as f32);
    }
}

impl<'a> Decoder<'a> {
    pub fn new(code: &'a NbCode) -> Self {
        let e = code.edges.len();
        let gf = &code.gf;
        // perm[e][y] = coef^{-1} · y, so permuted[y] = msg[perm[y]] is P(coef·x = y).
        let perms = code
            .edges
            .iter()
            .map(|edge| {
                let inv = gf.inv(edge.coef);
                let mut p = [0u8; Q];
                for (y, slot) in p.iter_mut().enumerate() {
                    *slot = gf.mul(inv, y as u8);
                }
                p
            })
            .collect();
        Self {
            code,
            v2c: vec![[0.0; Q]; e],
            c2v: vec![[1.0 / Q as f32; Q]; e],
            perms,
            scratch: vec![[0.0; Q]; code.dc],
            max_iters: 50,
        }
    }

    /// Decode from per-symbol likelihood vectors (non-negative, any scale).
    /// Returns the codeword if all checks are satisfied within
    /// `max_iters`, plus the iteration count.
    pub fn decode(&mut self, likelihoods: &[[f32; Q]]) -> (Option<Vec<u8>>, usize) {
        let code = self.code;
        assert_eq!(likelihoods.len(), code.n);
        let mut prior: Vec<[f32; Q]> = likelihoods.to_vec();
        for p in prior.iter_mut() {
            for v in p.iter_mut() {
                *v = v.max(1e-9);
            }
            normalise32(p);
        }
        for (ei, e) in code.edges.iter().enumerate() {
            self.v2c[ei] = prior[e.var];
        }
        let mut hard = vec![0u8; code.n];
        let mut prod = [0f64; Q];
        for iter in 1..=self.max_iters {
            // Check nodes.
            for edges in &code.check_edges {
                for (slot, &ei) in edges.iter().enumerate() {
                    let perm = &self.perms[ei];
                    let msg = &self.v2c[ei];
                    let t = &mut self.scratch[slot];
                    for y in 0..Q {
                        t[y] = msg[perm[y] as usize] as f64;
                    }
                    fwht(t);
                }
                for (slot, &ei) in edges.iter().enumerate() {
                    prod.fill(1.0);
                    for (o, t) in self.scratch[..edges.len()].iter().enumerate() {
                        if o != slot {
                            for y in 0..Q {
                                prod[y] *= t[y];
                            }
                        }
                    }
                    fwht(&mut prod);
                    let perm = &self.perms[ei];
                    let out = &mut self.c2v[ei];
                    let mut max = 0f64;
                    for y in 0..Q {
                        let v = prod[y].max(0.0);
                        out[perm[y] as usize] = v as f32;
                        max = max.max(v);
                    }
                    let floor = (max * 1e-10) as f32;
                    for v in out.iter_mut() {
                        *v = v.max(floor);
                    }
                    normalise32(out);
                }
            }
            // Variable nodes.
            for (v, edges) in code.var_edges.iter().enumerate() {
                let mut total = prior[v];
                for &ei in edges {
                    for (t, c) in total.iter_mut().zip(self.c2v[ei].iter()) {
                        *t *= c;
                    }
                }
                let (best, _) = total
                    .iter()
                    .enumerate()
                    .max_by(|a, b| a.1.total_cmp(b.1))
                    .unwrap();
                hard[v] = best as u8;
                for &ei in edges {
                    let mut msg = prior[v];
                    for &ej in edges {
                        if ej != ei {
                            for (mv, c) in msg.iter_mut().zip(self.c2v[ej].iter()) {
                                *mv *= c;
                            }
                        }
                    }
                    normalise32(&mut msg);
                    self.v2c[ei] = msg;
                }
            }
            if code.check(&hard) {
                return (Some(hard), iter);
            }
        }
        (None, self.max_iters)
    }
}
