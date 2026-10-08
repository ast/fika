//! End-to-end: build a burst, render audio, pad with silence or noise,
//! detect and decode.

use fika_modem::{Burst, FrameKind, Profile, Receiver, Transmitter};
use rand::SeedableRng;
use rand_distr::{Distribution, Normal};

fn blocks(kind: FrameKind, n: usize, seed: u8) -> Vec<Vec<u8>> {
    (0..n)
        .map(|b| {
            (0..kind.info_bytes())
                .map(|i| {
                    (i as u8)
                        .wrapping_mul(31)
                        .wrapping_add(seed ^ (b as u8 * 17))
                })
                .collect()
        })
        .collect()
}

/// Render a burst into a buffer with `lead` seconds of silence before it.
fn render(
    burst: &Burst,
    lane: usize,
    profile: Profile,
    lead_s: f64,
    offset_hz: f64,
) -> (Vec<f32>, usize) {
    let fs = 12_000;
    let tx = Transmitter::new(fs);
    let audio = tx.render(burst, lane, profile, offset_hz).unwrap();
    let lead = (lead_s * fs as f64) as usize;
    let mut buf = vec![0f32; lead];
    buf.extend_from_slice(&audio);
    buf.extend(std::iter::repeat_n(0f32, fs as usize));
    (buf, lead)
}

fn add_noise(buf: &mut [f32], snr_db: f64, amplitude: f64, seed: u64) {
    // SNR referenced to 2500 Hz: noise power in 2500 Hz = signal power / snr.
    let fs = 12_000.0;
    let sig_p = amplitude * amplitude / 2.0;
    let snr = 10f64.powf(snr_db / 10.0);
    let noise_p_2500 = sig_p / snr;
    let sigma = (noise_p_2500 * (fs / 2.0) / 2500.0).sqrt();
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    let dist = Normal::new(0.0, sigma).unwrap();
    for x in buf.iter_mut() {
        *x += dist.sample(&mut rng) as f32;
    }
}

#[test]
fn clean_loopback_all_lanes_profiles_kinds() {
    for profile in Profile::ALL {
        for kind in FrameKind::ALL {
            for lane in 0..4 {
                let n = if kind == FrameKind::Long { 2 } else { 1 };
                let burst =
                    Burst::new(kind, (lane as u8 * 5 + 3) % 16, blocks(kind, n, lane as u8))
                        .unwrap();
                let (buf, lead) = render(&burst, lane, profile, 0.37, 0.0);
                let mut rx = Receiver::new();
                let dets = rx.detect(&buf);
                let det = dets
                    .iter()
                    .filter(|d| d.lane == lane && d.profile == profile && d.kind == kind)
                    .min_by(|a, b| {
                        (a.start_sample - lead as f64)
                            .abs()
                            .total_cmp(&(b.start_sample - lead as f64).abs())
                    })
                    .unwrap_or_else(|| {
                        panic!("no detection for {profile:?} {kind:?} lane {lane}: {dets:?}")
                    });
                assert_eq!(det.phase, burst.phase, "{profile:?} {kind:?} lane {lane}");
                let sps = profile.samples_per_symbol(12_000).unwrap() as f64;
                assert!(
                    (det.start_sample - lead as f64).abs() < sps / 32.0,
                    "{profile:?} {kind:?} lane {lane}: start {} vs {lead}",
                    det.start_sample
                );
                assert!(
                    det.freq_offset_hz.abs() < 3.0,
                    "offset {}",
                    det.freq_offset_hz
                );
                let decoded = rx.decode_blocks(&buf, det, n);
                for (i, d) in decoded.iter().enumerate() {
                    assert_eq!(
                        d.bytes.as_deref(),
                        Some(&burst.blocks[i][..]),
                        "{profile:?} {kind:?} lane {lane} block {i}"
                    );
                }
            }
        }
    }
}

#[test]
fn loopback_with_frequency_offset_and_noise() {
    let kind = FrameKind::Long;
    let burst = Burst::new(kind, 11, blocks(kind, 3, 9)).unwrap();
    for (profile, snr_db) in [(Profile::Fast, -6.0), (Profile::Slow, -13.0)] {
        let (mut buf, lead) = render(&burst, 2, profile, 1.1, 17.0);
        add_noise(&mut buf, snr_db, 0.5, 42);
        let mut rx = Receiver::new();
        let dets = rx.detect(&buf);
        let det = dets
            .iter()
            .filter(|d| d.lane == 2 && d.profile == profile && d.kind == kind)
            .min_by(|a, b| {
                (a.start_sample - lead as f64)
                    .abs()
                    .total_cmp(&(b.start_sample - lead as f64).abs())
            })
            .unwrap_or_else(|| panic!("no detection at {snr_db} dB {profile:?}: {dets:?}"));
        assert_eq!(det.phase, 11);
        let sps = profile.samples_per_symbol(12_000).unwrap() as f64;
        assert!(
            (det.start_sample - lead as f64).abs() < sps / 16.0,
            "{profile:?}: start {} vs {lead}",
            det.start_sample
        );
        assert!(
            (det.freq_offset_hz - 17.0).abs() < 4.0,
            "offset {}",
            det.freq_offset_hz
        );
        eprintln!("{profile:?} snr est {:.1} dB (set {snr_db})", det.snr_db());
        let decoded = rx.decode_blocks(&buf, det, 3);
        for (i, d) in decoded.iter().enumerate() {
            assert_eq!(
                d.bytes.as_deref(),
                Some(&burst.blocks[i][..]),
                "block {i} iters {}",
                d.iterations
            );
        }
    }
}
