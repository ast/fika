use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

use fika_modem::Receiver;
use fika_proto::callsign;

use crate::decode::{Decoded, decode_burst, describe_dest};

#[derive(Args)]
pub struct RxArgs {
    pub input: PathBuf,
    /// SYNC detection threshold (noise mean 16).
    #[arg(long, default_value_t = 40.0)]
    pub threshold: f32,
    #[arg(long, short)]
    pub verbose: bool,
    #[command(flatten)]
    pub detector: crate::cmd_sim::DetectorArgs,
}

pub fn run(args: RxArgs) -> Result<()> {
    let samples = crate::wav::read_for_rx(&args.input)?;
    let mut rx = Receiver::new();
    args.detector.apply(&mut rx, args.threshold);
    let fs = rx.fs() as f64;
    let dets = rx.detect(&samples);
    println!(
        "{:.1} s of audio, {} burst(s) detected",
        samples.len() as f64 / fs,
        dets.len()
    );
    for det in &dets {
        println!(
            "-- t={:7.2}s lane {} {} {} phase {:2} offset {:+6.1} Hz snr {:+5.1} dB score {:.0}",
            det.start_sample / fs,
            det.lane,
            det.profile,
            det.kind.name(),
            det.phase,
            det.freq_offset_hz,
            det.snr_db(),
            det.score
        );
        match decode_burst(&mut rx, &samples, det) {
            Decoded::Message {
                blocks_ok,
                total,
                message: Some(m),
            } => {
                println!(
                    "   {} -> {}: {:?}  [{}/{} blocks, id {:04X}{}]",
                    callsign::unpack(m.sender)
                        .map(|c| c.to_string())
                        .unwrap_or("?".into()),
                    describe_dest(m.dest),
                    m.text,
                    blocks_ok,
                    total,
                    m.msg_id,
                    if m.ack_req { ", ack requested" } else { "" }
                );
            }
            Decoded::Message {
                blocks_ok,
                total,
                message: None,
            } => {
                println!("   message incomplete: {blocks_ok}/{total} blocks");
            }
            Decoded::Ack(a) => println!(
                "   ACK from {} to {} for {:04X}, snr {:+} dB",
                callsign::unpack(a.sender)
                    .map(|c| c.to_string())
                    .unwrap_or("?".into()),
                callsign::unpack(a.dest)
                    .map(|c| c.to_string())
                    .unwrap_or("?".into()),
                a.msg_id,
                a.snr_db
            ),
            Decoded::Beacon(b) => println!(
                "   beacon from {} grid {:?} tags {:03X} {:03X} status {}",
                callsign::unpack(b.sender)
                    .map(|c| c.to_string())
                    .unwrap_or("?".into()),
                b.grid,
                b.group_tags[0],
                b.group_tags[1],
                b.status
            ),
            Decoded::Failed => println!("   block 0 failed"),
        }
    }
    Ok(())
}
