//! Multi-station scenario: N bursts with random lanes, start times, phases
//! and levels in one passband, through one channel, decoded together.

use anyhow::Result;
use clap::Args;
use rand::{Rng, SeedableRng, rngs::StdRng};

use fika_channel::awgn::tone_power;
use fika_channel::interference::add_signal;
use fika_channel::{ChannelSpec, add_awgn, apply};
use fika_modem::{Burst, FrameKind, Profile, Receiver, Transmitter};
use fika_proto::{Destination, Message, callsign};

use crate::cmd_sim::DetectorArgs;
use crate::decode::{Decoded, decode_burst};

#[derive(Args)]
pub struct MultiArgs {
    /// Number of simultaneous stations.
    #[arg(long, default_value_t = 4)]
    pub stations: usize,
    #[arg(long, default_value = "fast")]
    pub profile: Profile,
    #[arg(long, default_value = "awgn")]
    pub channel: String,
    /// SNR of the weakest station, dB in 2500 Hz.
    #[arg(long, default_value_t = -6.0, allow_hyphen_values = true)]
    pub snr: f64,
    /// Level spread: each station is 0..spread dB above the weakest.
    #[arg(long, default_value_t = 10.0)]
    pub spread_db: f64,
    /// Force all stations into this lane (default: random lanes).
    #[arg(long)]
    pub lane: Option<usize>,
    /// Stagger window: start times are uniform in 0..stagger seconds.
    #[arg(long, default_value_t = 4.0)]
    pub stagger_s: f64,
    #[arg(long, default_value_t = 10)]
    pub trials: usize,
    #[arg(long, default_value_t = 1)]
    pub seed: u64,
    #[arg(long, default_value_t = 40.0)]
    pub threshold: f32,
    #[arg(long, short)]
    pub verbose: bool,
    #[command(flatten)]
    pub detector: DetectorArgs,
}

const CALLS: [&str; 8] = [
    "SM6WJM", "AD8KM", "SA6BSS", "SM7XYZ", "OH2ABC", "LA1DX", "OZ9QRP", "DL3FIK",
];
const TEXTS: [&str; 4] = [
    "Hej, hör ni mig? Kör 10 W här.",
    "QRV on 40 m from the summit, battery power.",
    "Kaffet är klart, kom in när ni kan. 73",
    "Signal report please, testing new antenna.",
];

pub fn run(args: MultiArgs) -> Result<()> {
    let spec = ChannelSpec::from_name(&args.channel)
        .ok_or_else(|| anyhow::anyhow!("unknown channel '{}'", args.channel))?;
    let fs = fika_modem::params::RX_SAMPLE_RATE;
    let tx = Transmitter::new(fs);
    let p_ref = tone_power(tx.amplitude as f64);
    println!(
        "fika multi: {} stations, {} profile, {} channel, weakest {} dB, spread {} dB, lanes {}",
        args.stations,
        args.profile,
        spec.name,
        args.snr,
        args.spread_db,
        args.lane.map(|l| l.to_string()).unwrap_or("random".into())
    );
    let mut total = 0usize;
    let mut decoded = 0usize;
    let mut detected = 0usize;
    for trial in 0..args.trials {
        let mut rng = StdRng::seed_from_u64(args.seed * 7919 + trial as u64);
        struct Station {
            lane: usize,
            start: usize,
            msg: Message,
            burst: Burst,
            level_db: f64,
        }
        // Random lanes are distinct while they can be, so that the default
        // scenario measures lanes rather than same-lane collisions.
        let mut lane_pool: Vec<usize> = (0..4).collect();
        let mut stations = Vec::new();
        for i in 0..args.stations {
            let msg = Message {
                sender: callsign::pack(CALLS[i % CALLS.len()]),
                dest: Destination::Group(fika_proto::group::group_id("fika")),
                msg_id: rng.random(),
                ack_req: false,
                text: TEXTS[rng.random_range(0..TEXTS.len())].to_string(),
            };
            let burst = Burst::new(FrameKind::Long, rng.random_range(0..16), msg.to_blocks()?)?;
            stations.push(Station {
                lane: args.lane.unwrap_or_else(|| {
                    if lane_pool.is_empty() {
                        rng.random_range(0..4)
                    } else {
                        lane_pool.swap_remove(rng.random_range(0..lane_pool.len()))
                    }
                }),
                start: (rng.random_range(0.0..args.stagger_s) * fs as f64) as usize + fs as usize,
                msg,
                burst,
                level_db: rng.random_range(0.0..=args.spread_db),
            });
        }
        let longest = stations
            .iter()
            .map(|s| s.start + (s.burst.airtime_s(args.profile) * fs as f64) as usize)
            .max()
            .unwrap();
        let mut buf = vec![0f32; longest + fs as usize];
        for s in &stations {
            let audio = tx.render(&s.burst, s.lane, args.profile, 0.0)?;
            add_signal(
                &mut buf,
                &audio,
                s.start,
                10f32.powf(s.level_db as f32 / 20.0),
            );
        }
        let mut buf = apply(&spec, &buf, fs as f64, &mut rng);
        add_awgn(&mut buf, args.snr, p_ref, fs as f64, &mut rng);

        let mut rx = Receiver::new();
        args.detector.apply(&mut rx, args.threshold);
        let dets = rx.detect(&buf);
        let mut got: Vec<Message> = Vec::new();
        for det in &dets {
            if let Decoded::Message {
                message: Some(m), ..
            } = decode_burst(&mut rx, &buf, det)
            {
                got.push(m);
            }
        }
        for s in &stations {
            total += 1;
            let was_detected = dets.iter().any(|d| {
                d.lane == s.lane
                    && d.profile == args.profile
                    && (d.start_sample - s.start as f64).abs() < 2.0 * 384.0
            });
            let ok = got.iter().any(|m| {
                m.msg_id == s.msg.msg_id && m.text == s.msg.text && m.sender == s.msg.sender
            });
            detected += was_detected as usize;
            decoded += ok as usize;
            if args.verbose {
                println!(
                    "  trial {trial} {} lane {} t={:.1}s +{:.1} dB: {}{}",
                    callsign::unpack(s.msg.sender).unwrap(),
                    s.lane,
                    s.start as f64 / fs as f64,
                    s.level_db,
                    if was_detected { "detected" } else { "missed" },
                    if ok { ", decoded" } else { "" }
                );
            }
        }
    }
    println!(
        "{} station bursts: {} detected ({:.0}%), {} decoded ({:.0}%)",
        total,
        detected,
        100.0 * detected as f64 / total as f64,
        decoded,
        100.0 * decoded as f64 / total as f64
    );
    Ok(())
}
