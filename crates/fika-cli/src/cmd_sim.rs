use anyhow::{Result, bail};
use clap::Args;
use rand::{Rng, SeedableRng, rngs::StdRng};

use fika_channel::awgn::tone_power;
use fika_channel::{ChannelSpec, add_awgn, add_carrier, apply, resample_ppm};
use fika_modem::{Profile, Receiver, Transmitter};

use crate::cmd_tx::{TxArgs, build};
use crate::decode::{Decoded, decode_burst};

#[derive(Args)]
pub struct SimArgs {
    #[arg(long, default_value = "fast")]
    pub profile: Profile,
    /// awgn, good, moderate, poor, flat.
    #[arg(long, default_value = "awgn")]
    pub channel: String,
    /// Single SNR in dB (2500 Hz reference).
    #[arg(long, allow_hyphen_values = true)]
    pub snr: Option<f64>,
    /// Sweep "start:end:step" in dB.
    #[arg(long, allow_hyphen_values = true)]
    pub sweep: Option<String>,
    #[arg(long, default_value_t = 20)]
    pub trials: usize,
    #[arg(long, default_value_t = 1)]
    pub seed: u64,
    #[arg(
        long,
        default_value = "Hej allihopa, hör ni mig? Kör 10 W i en dipol här. 73 de SM6WJM"
    )]
    pub text: String,
    #[arg(long, default_value = "SM6WJM")]
    pub from: String,
    #[arg(long, default_value = "@fika")]
    pub to: String,
    #[arg(long, default_value_t = 1)]
    pub lane: usize,
    /// Transmitter frequency error, Hz.
    #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
    pub offset_hz: f64,
    /// Receiver sample clock error, ppm.
    #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
    pub ppm: f64,
    /// Add a steady carrier at this audio frequency.
    #[arg(long)]
    pub carrier_hz: Option<f64>,
    /// Carrier level relative to the signal, dB.
    #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
    pub carrier_db: f64,
    #[arg(long, default_value_t = 40.0)]
    pub threshold: f32,
    /// Print a breakdown of false detections by profile and kind.
    #[arg(long, short)]
    pub verbose: bool,
    #[command(flatten)]
    pub detector: DetectorArgs,
}

/// Detector tuning knobs shared by `rx`, `sim` and `multi`.
#[derive(Args, Clone)]
pub struct DetectorArgs {
    /// Required ratio of the refined SYNC peak to its neighbours one symbol away.
    #[arg(long, default_value_t = 2.0)]
    pub peak_ratio: f32,
    /// Minimum SYNC bins (of 16) at or above a quarter of their mean.
    #[arg(long, default_value_t = 9)]
    pub min_support: usize,
    /// Required ratio between best and second-best PHASE candidate.
    #[arg(long, default_value_t = 1.5)]
    pub phase_ratio: f32,
}

impl DetectorArgs {
    pub fn apply(&self, rx: &mut Receiver, threshold: f32) {
        rx.cfg.threshold = threshold;
        rx.cfg.peak_ratio = self.peak_ratio;
        rx.cfg.min_support = self.min_support;
        rx.cfg.phase_ratio = self.phase_ratio;
    }
}

struct Tally {
    detected: usize,
    decoded: usize,
    blocks_ok: usize,
    blocks_total: usize,
    snr_est_sum: f64,
    false_dets: usize,
    false_by: std::collections::BTreeMap<String, usize>,
}

fn parse_sweep(s: &str) -> Result<Vec<f64>> {
    let parts: Vec<f64> = s
        .split(':')
        .map(|p| p.parse::<f64>())
        .collect::<Result<_, _>>()?;
    let [start, end, step] = parts[..] else {
        bail!("sweep must be start:end:step");
    };
    if step <= 0.0 {
        bail!("sweep step must be positive");
    }
    let mut v = Vec::new();
    let mut x = start;
    while x <= end + 1e-9 {
        v.push(x);
        x += step;
    }
    Ok(v)
}

pub fn run(args: SimArgs) -> Result<()> {
    let spec = ChannelSpec::from_name(&args.channel).ok_or_else(|| {
        anyhow::anyhow!(
            "unknown channel '{}', expected one of {:?}",
            args.channel,
            ChannelSpec::NAMES
        )
    })?;
    let snrs = match (&args.sweep, args.snr) {
        (Some(s), _) => parse_sweep(s)?,
        (None, Some(x)) => vec![x],
        (None, None) => bail!("give --snr or --sweep"),
    };
    let fs = fika_modem::params::RX_SAMPLE_RATE;
    let tx_args = TxArgs {
        from: args.from.clone(),
        to: args.to.clone(),
        text: args.text.clone(),
        lane: args.lane,
        profile: args.profile,
        phase: None,
        ack: false,
        offset_hz: args.offset_hz,
        lead_s: 0.0,
        tail_s: 0.0,
        fs,
        output: Default::default(),
    };
    let (probe, burst) = build(&tx_args, 0, 0)?;
    println!(
        "fika sim: {} profile, {} channel, {} chars -> {} block(s), {:.1} s burst, {} trials per point",
        args.profile,
        spec.name,
        probe.text.chars().count(),
        burst.blocks.len(),
        burst.airtime_s(args.profile),
        args.trials
    );
    println!(
        "{:>7} {:>9} {:>9} {:>9} {:>9} {:>9}",
        "snr_dB", "detect%", "decode%", "blocks%", "snr_est", "false/tr"
    );

    let tx = Transmitter::new(fs);
    let signal_power = tone_power(tx.amplitude as f64);
    for &snr_db in &snrs {
        let mut tally = Tally {
            detected: 0,
            decoded: 0,
            blocks_ok: 0,
            blocks_total: 0,
            snr_est_sum: 0.0,
            false_dets: 0,
            false_by: Default::default(),
        };
        for trial in 0..args.trials {
            let mut rng = StdRng::seed_from_u64(
                args.seed
                    .wrapping_mul(1_000_003)
                    .wrapping_add(trial as u64 * 7919 + (snr_db * 100.0) as u64),
            );
            let phase = rng.random_range(0..16u8);
            let msg_id: u16 = rng.random();
            let (expected, burst) = build(&tx_args, msg_id, phase)?;
            let audio = tx.render(&burst, args.lane, args.profile, args.offset_hz)?;
            let lead = rng.random_range(fs as usize / 2..fs as usize * 2);
            let mut buf = vec![0f32; lead];
            buf.extend_from_slice(&audio);
            buf.extend(std::iter::repeat_n(0f32, fs as usize));
            let mut buf = apply(&spec, &buf, fs as f64, &mut rng);
            if let Some(f) = args.carrier_hz {
                let a = tx.amplitude as f64 * 10f64.powf(args.carrier_db / 20.0);
                add_carrier(&mut buf, f, a, fs as f64);
            }
            add_awgn(&mut buf, snr_db, signal_power, fs as f64, &mut rng);
            let buf = resample_ppm(&buf, args.ppm);

            let mut rx = Receiver::new();
            args.detector.apply(&mut rx, args.threshold);
            let dets = rx.detect(&buf);
            let is_hit = |d: &&fika_modem::Detection| {
                d.lane == args.lane
                    && d.profile == args.profile
                    && d.kind == burst.kind
                    && (d.start_sample - lead as f64).abs() < 2.0 * fs as f64 * 0.032
            };
            let hit = dets.iter().filter(is_hit).min_by(|a, b| {
                (a.start_sample - lead as f64)
                    .abs()
                    .total_cmp(&(b.start_sample - lead as f64).abs())
            });
            for d in dets.iter().filter(|d| !is_hit(d)) {
                tally.false_dets += 1;
                *tally
                    .false_by
                    .entry(format!("{} {} lane{}", d.profile, d.kind.name(), d.lane))
                    .or_default() += 1;
            }
            tally.blocks_total += burst.blocks.len();
            let Some(det) = hit else { continue };
            tally.detected += 1;
            tally.snr_est_sum += det.snr_db() as f64;
            if let Decoded::Message {
                blocks_ok, message, ..
            } = decode_burst(&mut rx, &buf, det)
            {
                tally.blocks_ok += blocks_ok;
                if let Some(m) = message
                    && m.msg_id == msg_id
                    && m.text == expected.text
                    && m.sender == expected.sender
                    && m.dest == expected.dest
                {
                    tally.decoded += 1;
                }
            }
        }
        let n = args.trials as f64;
        println!(
            "{:>7.1} {:>8.0}% {:>8.0}% {:>8.0}% {:>+9.1} {:>9.2}",
            snr_db,
            100.0 * tally.detected as f64 / n,
            100.0 * tally.decoded as f64 / n,
            100.0 * tally.blocks_ok as f64 / tally.blocks_total.max(1) as f64,
            if tally.detected > 0 {
                tally.snr_est_sum / tally.detected as f64
            } else {
                f64::NAN
            },
            tally.false_dets as f64 / n
        );
        if args.verbose && !tally.false_by.is_empty() {
            println!("        false detections: {:?}", tally.false_by);
        }
    }
    Ok(())
}
