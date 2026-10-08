//! Channel regression suite: decode rate per scenario must stay above a
//! floor. Monte-Carlo with few trials, so floors sit well below the
//! typical result. Run with `cargo test --release -p fika-cli --test
//! channels -- --ignored` (or `just channels`); each scenario takes a few
//! seconds in release.

use rand::{Rng, SeedableRng, rngs::StdRng};

use fika_channel::awgn::tone_power;
use fika_channel::{ChannelSpec, Interferer, add_awgn, apply, resample_ppm};
use fika_modem::{Burst, FrameKind, Profile, Receiver, Transmitter};
use fika_proto::{Destination, Message, MessageAssembler, callsign, group};

const TEXT: &str = "Hej allihopa, hör ni mig? Kör 10 W i en dipol här. 73 de SM6WJM";

struct Scenario {
    name: &'static str,
    profile: Profile,
    snr_db: f64,
    spec: ChannelSpec,
    ppm: f64,
    trials: usize,
    /// Minimum fraction of messages decoded.
    min_rate: f64,
}

fn run(sc: &Scenario) -> f64 {
    let fs = fika_modem::params::RX_SAMPLE_RATE;
    let tx = Transmitter::new(fs);
    let lane = 1;
    let mut ok = 0;
    for trial in 0..sc.trials {
        let mut rng = StdRng::seed_from_u64(1000 + trial as u64);
        let msg = Message {
            sender: callsign::pack("SM6WJM"),
            dest: Destination::Group(group::group_id("fika")),
            msg_id: rng.random(),
            ack_req: false,
            text: TEXT.into(),
        };
        let burst = Burst::new(
            FrameKind::Long,
            rng.random_range(0..16),
            msg.to_blocks().unwrap(),
        )
        .unwrap();
        let audio = tx.render(&burst, lane, sc.profile, 0.0).unwrap();
        let lead = rng.random_range(fs as usize / 2..fs as usize * 2);
        let mut buf = vec![0f32; lead];
        buf.extend_from_slice(&audio);
        buf.extend(std::iter::repeat_n(0f32, fs as usize));
        let mut buf = apply(&sc.spec, &buf, fs as f64, &mut rng);
        add_awgn(
            &mut buf,
            sc.snr_db,
            tone_power(tx.amplitude as f64),
            fs as f64,
            &mut rng,
        );
        let buf = resample_ppm(&buf, sc.ppm);

        let mut rx = Receiver::new();
        let dets = rx.detect(&buf);
        let Some(det) = dets
            .iter()
            .filter(|d| d.lane == lane && d.profile == sc.profile && d.kind == FrameKind::Long)
            .min_by(|a, b| {
                (a.start_sample - lead as f64)
                    .abs()
                    .total_cmp(&(b.start_sample - lead as f64).abs())
            })
        else {
            continue;
        };
        let mut asm = MessageAssembler::new();
        let n = burst.blocks.len();
        for k in 0..n {
            if let Some(bytes) = rx.decode_block(&buf, det, k).bytes {
                let _ = asm.push(k, &bytes);
            }
        }
        if asm.is_complete() && asm.finish().map(|m| m.text == TEXT).unwrap_or(false) {
            ok += 1;
        }
    }
    ok as f64 / sc.trials as f64
}

fn scenarios() -> Vec<Scenario> {
    use Profile::{Fast, Slow};
    let s = |name, profile, snr_db, spec, ppm, trials, min_rate| Scenario {
        name,
        profile,
        snr_db,
        spec,
        ppm,
        trials,
        min_rate,
    };
    vec![
        // Reference points on AWGN, about 1 dB above the measured 50 % threshold.
        s(
            "fast awgn -10 dB",
            Fast,
            -10.0,
            ChannelSpec::awgn(),
            0.0,
            12,
            0.85,
        ),
        s(
            "slow awgn -17 dB",
            Slow,
            -17.0,
            ChannelSpec::awgn(),
            0.0,
            6,
            0.8,
        ),
        // Rig and operator imperfections.
        s(
            "fast offset +30 Hz, 100 ppm",
            Fast,
            -9.0,
            ChannelSpec::awgn().with_shift(30.0),
            100.0,
            10,
            0.8,
        ),
        s(
            "fast drift 1 Hz/s",
            Fast,
            -8.0,
            ChannelSpec::awgn().with_drift(1.0),
            0.0,
            10,
            0.8,
        ),
        s(
            "fast SSB passband",
            Fast,
            -9.0,
            ChannelSpec::awgn().with_bandpass(),
            0.0,
            10,
            0.8,
        ),
        // Fading, ITU-R F.1487 mid-latitude.
        // Slow selective fading: the lane can sit in a notch for a whole
        // burst, so even at 0 dB some messages are simply gone.
        s(
            "fast mid-quiet 0 dB",
            Fast,
            0.0,
            ChannelSpec::itu("mid-quiet").unwrap(),
            0.0,
            12,
            0.6,
        ),
        s(
            "fast mid-moderate -2 dB",
            Fast,
            -2.0,
            ChannelSpec::itu("mid-moderate").unwrap(),
            0.0,
            12,
            0.6,
        ),
        s(
            "fast mid-disturbed 0 dB",
            Fast,
            0.0,
            ChannelSpec::itu("mid-disturbed").unwrap(),
            0.0,
            12,
            0.5,
        ),
        s(
            "slow mid-moderate -10 dB",
            Slow,
            -10.0,
            ChannelSpec::itu("mid-moderate").unwrap(),
            0.0,
            6,
            0.5,
        ),
        // Interference inside the lane: the hopping must ride over it.
        s(
            "fast carrier in lane, 0 dB",
            Fast,
            -6.0,
            ChannelSpec::awgn().with_interferer(Interferer::Carrier {
                freq_hz: 1234.0,
                level_db: 0.0,
            }),
            0.0,
            10,
            0.8,
        ),
        // A carrier halfway between two tones leaks into both through the
        // rectangular symbol window; +6 dB is fine, +10 dB is not (see
        // DESIGN.md open issues: coherent carrier cancellation).
        s(
            "fast carrier between tones, +6 dB",
            Fast,
            -6.0,
            ChannelSpec::awgn().with_interferer(Interferer::Carrier {
                freq_hz: 1234.0,
                level_db: 6.0,
            }),
            0.0,
            10,
            0.7,
        ),
        s(
            "fast CW in lane, +6 dB",
            Fast,
            -6.0,
            ChannelSpec::awgn().with_interferer(Interferer::Cw {
                freq_hz: 1100.0,
                level_db: 6.0,
                wpm: 25.0,
            }),
            0.0,
            10,
            0.7,
        ),
        s(
            "fast RTTY in lane, -3 dB",
            Fast,
            -6.0,
            ChannelSpec::awgn().with_interferer(Interferer::Rtty {
                center_hz: 1300.0,
                level_db: -3.0,
            }),
            0.0,
            10,
            0.6,
        ),
        s(
            "fast PSK31 next lane, +10 dB",
            Fast,
            -8.0,
            ChannelSpec::awgn().with_interferer(Interferer::Psk31 {
                freq_hz: 1800.0,
                level_db: 10.0,
            }),
            0.0,
            10,
            0.8,
        ),
        // Lightning static.
        s(
            "fast QRN 5/s +20 dB",
            Fast,
            -8.0,
            ChannelSpec::awgn().with_impulsive(5.0, 2.0, 20.0),
            0.0,
            10,
            0.7,
        ),
    ]
}

#[test]
#[ignore = "Monte-Carlo, several seconds per scenario; run with --ignored in release"]
fn decode_rates_stay_above_floors() {
    let mut failures = Vec::new();
    println!("{:<34} {:>6} {:>6}", "scenario", "rate", "floor");
    for sc in scenarios() {
        let rate = run(&sc);
        println!(
            "{:<34} {:>5.0}% {:>5.0}%{}",
            sc.name,
            rate * 100.0,
            sc.min_rate * 100.0,
            if rate < sc.min_rate { "  FAIL" } else { "" }
        );
        if rate < sc.min_rate {
            failures.push(format!(
                "{}: {:.0}% < {:.0}%",
                sc.name,
                rate * 100.0,
                sc.min_rate * 100.0
            ));
        }
    }
    assert!(failures.is_empty(), "below floor: {failures:?}");
}
