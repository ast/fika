//! Stage-0 gates: single-user AWGN sensitivity and multi-user decoding with
//! asynchronous equal-power interferers, in the abstract 64-bin model.
//! Run in release: `cargo test --release -p fika-nb --test gates -- --ignored --nocapture`.

use std::f64::consts::PI;

use rand::{Rng, SeedableRng, rngs::StdRng};
use rand_distr::{Distribution, Normal};

use fika_nb::gf64::Q;
use fika_nb::{Decoder, LikelihoodParams, NbCode, symbol_likelihoods};

const N_FFT: f64 = 320.0;

/// Complex partial-window DFT of a unit tone at integer bin offset `delta`
/// present during the window fraction [a, b) (0 ≤ a < b ≤ 1): the
/// rectangular-window leakage of an interferer whose symbol boundary falls
/// inside our window.
fn partial(delta: i64, a: f64, b: f64) -> (f64, f64) {
    if delta == 0 {
        return (b - a, 0.0);
    }
    // (1/N) Σ_{n=aN}^{bN-1} e^{j2πδn/N} = (e^{j2πδb} − e^{j2πδa}) / (N(e^{j2πδ/N} − 1))
    let w = 2.0 * PI * delta as f64;
    let (nr, ni) = ((w * b).cos() - (w * a).cos(), (w * b).sin() - (w * a).sin());
    let (dr, di) = ((w / N_FFT).cos() - 1.0, (w / N_FFT).sin());
    let den = N_FFT * (dr * dr + di * di);
    ((nr * dr + ni * di) / den, (ni * dr - nr * di) / den)
}

struct Scenario {
    users: usize,
    /// Es/N0 of the wanted user, dB.
    es_n0_db: f64,
    /// Interferer level relative to the wanted user, dB.
    interferer_db: f64,
    crowd: bool,
}

/// One block: returns true if the wanted user's info decodes.
fn trial(code: &NbCode, sc: &Scenario, seed: u64) -> bool {
    let mut rng = StdRng::seed_from_u64(seed);
    let normal = Normal::new(0.0f64, (0.5f64).sqrt()).unwrap(); // E|n|² = 1
    let gamma = 10f64.powf(sc.es_n0_db / 10.0);
    let a = gamma.sqrt();
    let ai = a * 10f64.powf(sc.interferer_db / 20.0);
    let info: Vec<u8> = (0..code.k).map(|_| rng.random_range(0..64)).collect();
    let cw = code.encode(&info);
    // Interferers: random tones, boundaries at a random fraction of our window.
    let offsets: Vec<f64> = (1..sc.users).map(|_| rng.random_range(0.0..1.0)).collect();
    let mut prev: Vec<u8> = (1..sc.users).map(|_| rng.random_range(0..64)).collect();
    let mut energies: Vec<[f32; Q]> = Vec::with_capacity(code.n);
    for &x in &cw {
        let mut re = [0f64; Q];
        let mut im = [0f64; Q];
        for t in 0..Q {
            re[t] = normal.sample(&mut rng);
            im[t] = normal.sample(&mut rng);
        }
        let ph: f64 = rng.random_range(0.0..2.0 * PI);
        re[x as usize] += a * ph.cos();
        im[x as usize] += a * ph.sin();
        for (u, &off) in offsets.iter().enumerate() {
            let next: u8 = rng.random_range(0..64);
            let ph: f64 = rng.random_range(0.0..2.0 * PI);
            let (c, s) = (ph.cos(), ph.sin());
            for t in 0..Q {
                let d1 = t as i64 - prev[u] as i64;
                let d2 = t as i64 - next as i64;
                let (r1, i1) = partial(d1, 0.0, off);
                let (r2, i2) = partial(d2, off, 1.0);
                let (pr, pi) = (r1 + r2, i1 + i2);
                re[t] += ai * (pr * c - pi * s);
                im[t] += ai * (pr * s + pi * c);
            }
            prev[u] = next;
        }
        let mut e = [0f32; Q];
        for t in 0..Q {
            e[t] = (re[t] * re[t] + im[t] * im[t]) as f32;
        }
        energies.push(e);
    }
    // Receiver-side per-bin noise normalisation (75th percentile / ln 4).
    let mut col = Vec::with_capacity(code.n);
    let mut noise = [1f32; Q];
    for t in 0..Q {
        col.clear();
        col.extend(energies.iter().map(|e| e[t]));
        col.sort_by(|a, b| a.total_cmp(b));
        noise[t] = (col[col.len() * 3 / 4] / 4f32.ln()).max(1e-6);
    }
    for e in energies.iter_mut() {
        for t in 0..Q {
            e[t] /= noise[t];
        }
    }
    let q = fika_nb::likelihood::estimate_q(&energies, 4.0);
    let params = LikelihoodParams {
        gamma: gamma as f32,
        q,
    };
    let lik: Vec<[f32; Q]> = energies
        .iter()
        .map(|e| symbol_likelihoods(e, params))
        .collect();
    let mut dec = Decoder::new(code);
    matches!(dec.decode(&lik), (Some(out), _) if code.info_of(&out) == info)
}

fn rate(code: &NbCode, sc: &Scenario, trials: usize, seed: u64) -> f64 {
    (0..trials)
        .filter(|&i| trial(code, sc, seed * 1000 + i as u64))
        .count() as f64
        / trials as f64
}

#[test]
#[ignore = "Monte-Carlo; run in release with --ignored --nocapture"]
fn gate1_single_user_sensitivity() {
    let long = NbCode::long();
    let crowd = NbCode::crowd();
    println!("{:>6} {:>10} {:>10}", "Es/N0", "rate1/2", "rate1/3");
    let mut r12_at_68 = 0.0;
    for es in [4, 5, 6, 7, 8, 9] {
        let a = rate(
            &long,
            &Scenario {
                users: 1,
                es_n0_db: es as f64,
                interferer_db: 0.0,
                crowd: false,
            },
            40,
            10 + es as u64,
        );
        let b = rate(
            &crowd,
            &Scenario {
                users: 1,
                es_n0_db: es as f64,
                interferer_db: 0.0,
                crowd: true,
            },
            40,
            20 + es as u64,
        );
        println!("{:>6} {:>9.0}% {:>9.0}%", es, a * 100.0, b * 100.0);
        if es == 7 {
            r12_at_68 = a;
        }
    }
    assert!(r12_at_68 >= 0.5, "rate 1/2 should reach 50 % by 7 dB");
}

#[test]
#[ignore = "Monte-Carlo; run in release with --ignored --nocapture"]
fn gate2_multi_user() {
    let long = NbCode::long();
    let crowd = NbCode::crowd();
    println!("{:<34} {:>7}", "scenario", "decoded");
    let cases: Vec<(&str, &NbCode, Scenario)> = vec![
        (
            "1/2: 2 users equal, 10 dB",
            &long,
            Scenario {
                users: 2,
                es_n0_db: 10.0,
                interferer_db: 0.0,
                crowd: false,
            },
        ),
        (
            "1/2: 3 users equal, 12 dB",
            &long,
            Scenario {
                users: 3,
                es_n0_db: 12.0,
                interferer_db: 0.0,
                crowd: false,
            },
        ),
        (
            "1/2: 4 users equal, 14 dB",
            &long,
            Scenario {
                users: 4,
                es_n0_db: 14.0,
                interferer_db: 0.0,
                crowd: false,
            },
        ),
        (
            "1/2: 5 users equal, 16 dB",
            &long,
            Scenario {
                users: 5,
                es_n0_db: 16.0,
                interferer_db: 0.0,
                crowd: false,
            },
        ),
        (
            "1/2: near-far +10 dB, 11 dB",
            &long,
            Scenario {
                users: 2,
                es_n0_db: 11.0,
                interferer_db: 10.0,
                crowd: false,
            },
        ),
        (
            "1/2: near-far +15 dB, 12 dB",
            &long,
            Scenario {
                users: 2,
                es_n0_db: 12.0,
                interferer_db: 15.0,
                crowd: false,
            },
        ),
        (
            "1/3: 4 users equal, 12 dB",
            &crowd,
            Scenario {
                users: 4,
                es_n0_db: 12.0,
                interferer_db: 0.0,
                crowd: true,
            },
        ),
        (
            "1/3: 6 users equal, 14 dB",
            &crowd,
            Scenario {
                users: 6,
                es_n0_db: 14.0,
                interferer_db: 0.0,
                crowd: true,
            },
        ),
        (
            "1/3: 8 users equal, 16 dB",
            &crowd,
            Scenario {
                users: 8,
                es_n0_db: 16.0,
                interferer_db: 0.0,
                crowd: true,
            },
        ),
    ];
    let mut results = Vec::new();
    for (i, (name, code, sc)) in cases.iter().enumerate() {
        let r = rate(code, sc, 20, 100 + i as u64);
        println!("{:<34} {:>6.0}%", name, r * 100.0);
        results.push(r);
    }
    assert!(
        results[0] >= 0.9,
        "2 users at 10 dB: {:.0}%",
        results[0] * 100.0
    );
    assert!(
        results[1] >= 0.9,
        "3 users at 12 dB: {:.0}%",
        results[1] * 100.0
    );
    assert!(
        results[2] >= 0.8,
        "4 users at 14 dB: {:.0}%",
        results[2] * 100.0
    );
}
