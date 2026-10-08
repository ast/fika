//! Ultra-sparse (dv = 2) non-binary LDPC codes over GF(64).
//!
//! With column weight 2 the Tanner graph is the line graph of a `dc`-regular
//! *check graph* G on `m` vertices with `n` edges: every coded symbol is an
//! edge joining its two checks. Construction:
//!
//! 1. Progressive edge growth on G: each new edge joins the least-filled
//!    vertex to the vertex farthest from it in the current graph, which
//!    pushes the girth up (girth 6 in G = girth 12 in the Tanner graph is
//!    reachable on 64 vertices of degree 4).
//! 2. Cycle-aware labels: a cycle in G whose product of coefficient ratios
//!    around the cycle equals 1 is a low-weight codeword. Enumerate cycles
//!    of length ≤ 8 and redraw labels until none has product 1.
//! 3. Gaussian elimination over GF(64) for an information set and a
//!    systematic generator.
//!
//! Everything is driven by `SplitMix64(seed)`, so the code is a pure
//! function of (n, k, seed).

use crate::gf64::Gf64;
use crate::rng::SplitMix64;

#[derive(Clone, Debug)]
pub struct Edge {
    pub check: usize,
    pub var: usize,
    pub coef: u8,
}

pub struct NbCode {
    pub n: usize,
    pub k: usize,
    pub m: usize,
    pub dc: usize,
    pub seed: u64,
    pub edges: Vec<Edge>,
    /// Edge indices per check node.
    pub check_edges: Vec<Vec<usize>>,
    /// Edge indices per variable node (always two).
    pub var_edges: Vec<Vec<usize>>,
    pub info_positions: Vec<usize>,
    pub parity_positions: Vec<usize>,
    /// parity[j] = Σ_i info[i] · generator[i][j]
    generator: Vec<Vec<u8>>,
    pub gf: Gf64,
}

impl NbCode {
    /// Rate-1/2, n = 128: the fika long-frame code.
    pub fn long() -> Self {
        Self::new(128, 64, 0x6f69_6b61)
    }

    /// Rate-1/3, n = 192, same k: the "crowd" code.
    pub fn crowd() -> Self {
        Self::new(192, 64, 0x6372_6f77)
    }

    pub fn new(n: usize, k: usize, seed: u64) -> Self {
        let m = n - k;
        assert!(
            m > 0 && (2 * n).is_multiple_of(m),
            "2n/m must be an integer check degree"
        );
        let dc = 2 * n / m;
        let gf = Gf64::new();
        let mut rng = SplitMix64::new(seed);
        // PEG is greedy; the last edges can close short cycles. Keep the
        // best of many attempts, stopping at girth 6 (the maximum on 64
        // vertices of degree 4).
        let mut best: Option<(Vec<(usize, usize)>, usize)> = None;
        for _ in 0..400 {
            if let Some(g) = peg_graph(m, dc, &mut rng) {
                let gi = girth(m, &g);
                if best.as_ref().is_none_or(|(_, bg)| gi > *bg) {
                    best = Some((g, gi));
                }
                if gi >= 6 {
                    break;
                }
            }
        }
        let (pairs, _girth) = best.expect("PEG graph");
        let mut edges = Vec::with_capacity(2 * n);
        let mut check_edges = vec![Vec::new(); m];
        let mut var_edges = vec![Vec::new(); n];
        for (v, &(a, b)) in pairs.iter().enumerate() {
            for c in [a, b] {
                let ei = edges.len();
                edges.push(Edge {
                    check: c,
                    var: v,
                    coef: 1,
                });
                check_edges[c].push(ei);
                var_edges[v].push(ei);
            }
        }
        let mut code = Self {
            n,
            k,
            m,
            dc,
            seed,
            edges,
            check_edges,
            var_edges,
            info_positions: Vec::new(),
            parity_positions: Vec::new(),
            generator: Vec::new(),
            gf,
        };
        code.assign_labels(&pairs, &mut rng);
        code.build_generator();
        code
    }

    /// Random nonzero labels, then cancel every short cycle whose
    /// coefficient-ratio product is 1 by redrawing one of its edges.
    fn assign_labels(&mut self, pairs: &[(usize, usize)], rng: &mut SplitMix64) {
        for e in self.edges.iter_mut() {
            e.coef = 1 + rng.below(63) as u8;
        }
        let cycles = enumerate_cycles(self.m, pairs, 8);
        for _round in 0..10_000 {
            let mut fixed = true;
            for cyc in &cycles {
                if self.cycle_product_is_one(cyc) {
                    fixed = false;
                    // Redraw both labels of the first variable on the cycle.
                    let v = cyc[0];
                    for &ei in &self.var_edges[v].clone() {
                        self.edges[ei].coef = 1 + rng.below(63) as u8;
                    }
                }
            }
            if fixed {
                return;
            }
        }
        panic!("could not cancel short cycles");
    }

    /// For a cycle given as variables v_0..v_{L-1} (consecutive variables
    /// share a check), compute Π h(c_i, v_i)/h(c_i, v_{i+1}) where c_i is
    /// the check shared by v_i and v_{i+1}; the cycle supports a weight-L
    /// codeword iff the product is 1.
    fn cycle_product_is_one(&self, cyc: &[usize]) -> bool {
        let gf = &self.gf;
        let l = cyc.len();
        let mut prod = 1u8;
        for i in 0..l {
            let v = cyc[i];
            let w = cyc[(i + 1) % l];
            // Shared check between v and w.
            let ev = &self.var_edges[v];
            let ew = &self.var_edges[w];
            let shared = ev
                .iter()
                .find_map(|&a| {
                    ew.iter()
                        .find(|&&b| self.edges[a].check == self.edges[b].check)
                        .map(|&b| (a, b))
                })
                .expect("consecutive cycle variables share a check");
            let (ea, eb) = shared;
            prod = gf.mul(prod, gf.div(self.edges[ea].coef, self.edges[eb].coef));
        }
        prod == 1
    }

    fn dense_h(&self) -> Vec<Vec<u8>> {
        let mut h = vec![vec![0u8; self.n]; self.m];
        for e in &self.edges {
            h[e.check][e.var] = e.coef;
        }
        h
    }

    fn build_generator(&mut self) {
        let gf = &self.gf;
        let mut h = self.dense_h();
        let (m, n) = (self.m, self.n);
        let mut pivot_cols = Vec::with_capacity(m);
        let mut used = vec![false; n];
        let mut row = 0;
        for c in (0..n).rev() {
            if row >= m {
                break;
            }
            let Some(r) = (row..m).find(|&r| h[r][c] != 0) else {
                continue;
            };
            h.swap(row, r);
            let inv = gf.inv(h[row][c]);
            for v in h[row].iter_mut() {
                *v = gf.mul(*v, inv);
            }
            for r2 in 0..m {
                if r2 != row && h[r2][c] != 0 {
                    let f = h[r2][c];
                    let pivot_row = h[row].clone();
                    for (hv, &pv) in h[r2].iter_mut().zip(pivot_row.iter()) {
                        *hv ^= gf.mul(f, pv);
                    }
                }
            }
            pivot_cols.push(c);
            used[c] = true;
            row += 1;
        }
        assert_eq!(row, m, "parity-check matrix is rank deficient");
        let info: Vec<usize> = (0..n).filter(|&c| !used[c]).collect();
        assert_eq!(info.len(), self.k);
        let mut generator = vec![vec![0u8; m]; self.k];
        for i in 0..m {
            for (ii, &c) in info.iter().enumerate() {
                generator[ii][i] = h[i][c];
            }
        }
        self.info_positions = info;
        self.parity_positions = pivot_cols;
        self.generator = generator;
    }

    pub fn encode(&self, info: &[u8]) -> Vec<u8> {
        assert_eq!(info.len(), self.k);
        let gf = &self.gf;
        let mut cw = vec![0u8; self.n];
        for (ii, &pos) in self.info_positions.iter().enumerate() {
            cw[pos] = info[ii];
        }
        for (j, &pos) in self.parity_positions.iter().enumerate() {
            let mut acc = 0u8;
            for (ii, &x) in info.iter().enumerate() {
                if x != 0 {
                    acc ^= gf.mul(self.generator[ii][j], x);
                }
            }
            cw[pos] = acc;
        }
        debug_assert!(self.check(&cw));
        cw
    }

    pub fn info_of(&self, cw: &[u8]) -> Vec<u8> {
        self.info_positions.iter().map(|&p| cw[p]).collect()
    }

    pub fn check(&self, cw: &[u8]) -> bool {
        self.check_edges.iter().all(|edges| {
            edges.iter().fold(0u8, |acc, &ei| {
                let e = &self.edges[ei];
                acc ^ self.gf.mul(e.coef, cw[e.var])
            }) == 0
        })
    }

    /// Girth of the check graph (shortest cycle), for diagnostics.
    pub fn check_graph_girth(&self) -> usize {
        let pairs: Vec<(usize, usize)> = self
            .var_edges
            .iter()
            .map(|es| (self.edges[es[0]].check, self.edges[es[1]].check))
            .collect();
        girth(self.m, &pairs)
    }
}

/// Adjacency lists from edge pairs.
fn adjacency(m: usize, pairs: &[(usize, usize)]) -> Vec<Vec<usize>> {
    let mut adj = vec![Vec::new(); m];
    for &(a, b) in pairs {
        adj[a].push(b);
        adj[b].push(a);
    }
    adj
}

/// BFS distances from `src`, usize::MAX when unreachable.
fn bfs(adj: &[Vec<usize>], src: usize) -> Vec<usize> {
    let mut dist = vec![usize::MAX; adj.len()];
    let mut queue = std::collections::VecDeque::new();
    dist[src] = 0;
    queue.push_back(src);
    while let Some(u) = queue.pop_front() {
        for &w in &adj[u] {
            if dist[w] == usize::MAX {
                dist[w] = dist[u] + 1;
                queue.push_back(w);
            }
        }
    }
    dist
}

/// PEG: build a simple `dc`-regular graph on `m` vertices, edge by edge,
/// joining the least-filled vertex to the farthest eligible vertex.
fn peg_graph(m: usize, dc: usize, rng: &mut SplitMix64) -> Option<Vec<(usize, usize)>> {
    let n = m * dc / 2;
    let mut pairs: Vec<(usize, usize)> = Vec::with_capacity(n);
    let mut degree = vec![0usize; m];
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); m];
    for _ in 0..n {
        // Least-filled vertex, random tie-break.
        let min_deg = *degree.iter().min().unwrap();
        let us: Vec<usize> = (0..m).filter(|&v| degree[v] == min_deg).collect();
        let u = us[rng.below(us.len())];
        let dist = bfs(&adj, u);
        let mut best: Vec<usize> = Vec::new();
        let mut best_key = (0usize, usize::MAX);
        for v in 0..m {
            if v == u || degree[v] >= dc || adj[u].contains(&v) {
                continue;
            }
            // Prefer unreachable (infinite distance), then farthest, then least filled.
            let d = if dist[v] == usize::MAX {
                usize::MAX
            } else {
                dist[v]
            };
            let key = (d, usize::MAX - degree[v]);
            if key > best_key {
                best_key = key;
                best = vec![v];
            } else if key == best_key {
                best.push(v);
            }
        }
        if best.is_empty() {
            return None;
        }
        let v = best[rng.below(best.len())];
        pairs.push((u, v));
        degree[u] += 1;
        degree[v] += 1;
        adj[u].push(v);
        adj[v].push(u);
    }
    if degree.iter().any(|&d| d != dc) {
        return None;
    }
    Some(pairs)
}

fn girth(m: usize, pairs: &[(usize, usize)]) -> usize {
    let adj = adjacency(m, pairs);
    let mut g = usize::MAX;
    for s in 0..m {
        // BFS tracking parents to find the shortest cycle through s.
        let mut dist = vec![usize::MAX; m];
        let mut parent = vec![usize::MAX; m];
        let mut q = std::collections::VecDeque::new();
        dist[s] = 0;
        q.push_back(s);
        while let Some(u) = q.pop_front() {
            for &w in &adj[u] {
                if dist[w] == usize::MAX {
                    dist[w] = dist[u] + 1;
                    parent[w] = u;
                    q.push_back(w);
                } else if parent[u] != w {
                    g = g.min(dist[u] + dist[w] + 1);
                }
            }
        }
    }
    g
}

/// Simple cycles of length 3..=max_len in the check graph, each once, as
/// sequences of *variables* (edge indices into `pairs`).
fn enumerate_cycles(m: usize, pairs: &[(usize, usize)], max_len: usize) -> Vec<Vec<usize>> {
    // adjacency with edge ids
    let mut adj: Vec<Vec<(usize, usize)>> = vec![Vec::new(); m];
    for (e, &(a, b)) in pairs.iter().enumerate() {
        adj[a].push((b, e));
        adj[b].push((a, e));
    }
    let mut cycles = Vec::new();
    // Each cycle is reported from its smallest vertex, and only in the
    // direction where the second vertex is smaller than the last vertex.
    for start in 0..m {
        let mut path_v = vec![start];
        let mut path_e: Vec<usize> = Vec::new();
        fn dfs(
            start: usize,
            adj: &[Vec<(usize, usize)>],
            path_v: &mut Vec<usize>,
            path_e: &mut Vec<usize>,
            max_len: usize,
            cycles: &mut Vec<Vec<usize>>,
        ) {
            let u = *path_v.last().unwrap();
            for &(w, e) in &adj[u] {
                if path_e.last() == Some(&e) {
                    continue;
                }
                if w == start && path_e.len() >= 2 {
                    // Close the cycle; direction rule to avoid duplicates.
                    if path_v[1] < u {
                        let mut c = path_e.clone();
                        c.push(e);
                        cycles.push(c);
                    }
                    continue;
                }
                if w <= start || path_v.contains(&w) || path_e.len() + 1 >= max_len {
                    continue;
                }
                path_v.push(w);
                path_e.push(e);
                dfs(start, adj, path_v, path_e, max_len, cycles);
                path_v.pop();
                path_e.pop();
            }
        }
        dfs(start, &adj, &mut path_v, &mut path_e, max_len, &mut cycles);
    }
    cycles
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_code_is_regular_with_decent_girth() {
        let code = NbCode::long();
        assert_eq!((code.n, code.k, code.m, code.dc), (128, 64, 64, 4));
        assert!(code.check_edges.iter().all(|e| e.len() == 4));
        assert!(code.var_edges.iter().all(|e| e.len() == 2));
        let g = code.check_graph_girth();
        eprintln!("check-graph girth {g}");
        assert!(g >= 5, "girth {g}");
        let info: Vec<u8> = (0..64).map(|i| (i * 37 % 64) as u8).collect();
        let cw = code.encode(&info);
        assert!(code.check(&cw));
        assert_eq!(code.info_of(&cw), info);
        let mut bad = cw.clone();
        bad[3] ^= 1;
        assert!(!code.check(&bad));
    }

    #[test]
    fn crowd_code_builds() {
        let code = NbCode::crowd();
        assert_eq!((code.n, code.k, code.m, code.dc), (192, 64, 128, 3));
        assert!(code.check_graph_girth() >= 5);
        let cw = code.encode(&[7u8; 64]);
        assert!(code.check(&cw));
    }

    #[test]
    fn no_short_cycle_supports_a_codeword() {
        let code = NbCode::long();
        let pairs: Vec<(usize, usize)> = code
            .var_edges
            .iter()
            .map(|es| (code.edges[es[0]].check, code.edges[es[1]].check))
            .collect();
        let cycles = enumerate_cycles(code.m, &pairs, 8);
        eprintln!("{} cycles of length <= 8", cycles.len());
        assert!(cycles.iter().all(|c| !code.cycle_product_is_one(c)));
    }

    #[test]
    fn deterministic() {
        assert_eq!(
            NbCode::long().encode(&[5u8; 64]),
            NbCode::long().encode(&[5u8; 64])
        );
        assert_ne!(
            NbCode::new(128, 64, 1).encode(&[5u8; 64]),
            NbCode::new(128, 64, 2).encode(&[5u8; 64])
        );
    }
}
