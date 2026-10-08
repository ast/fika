use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Args;
use rand::Rng;

use fika_modem::{Burst, FrameKind, Profile, Transmitter};
use fika_proto::{Destination, Message, callsign, group};

#[derive(Args, Clone)]
pub struct TxArgs {
    /// Sender callsign.
    #[arg(long)]
    pub from: String,
    /// Destination: "all", "@group" or a callsign.
    #[arg(long, default_value = "all")]
    pub to: String,
    /// Message text.
    #[arg(long)]
    pub text: String,
    #[arg(long, default_value_t = 1)]
    pub lane: usize,
    #[arg(long, default_value = "fast")]
    pub profile: Profile,
    /// Pattern phase 0..15; random if omitted.
    #[arg(long)]
    pub phase: Option<u8>,
    /// Request an acknowledgement (direct messages only).
    #[arg(long)]
    pub ack: bool,
    /// Simulated transmitter frequency error, Hz.
    #[arg(long, default_value_t = 0.0)]
    pub offset_hz: f64,
    #[arg(long, default_value_t = 1.0)]
    pub lead_s: f64,
    #[arg(long, default_value_t = 1.0)]
    pub tail_s: f64,
    #[arg(long, default_value_t = 12_000)]
    pub fs: u32,
    #[arg(short, long, default_value = "fika.wav")]
    pub output: PathBuf,
}

pub fn parse_dest(s: &str) -> Destination {
    if s.eq_ignore_ascii_case("all") {
        Destination::All
    } else if let Some(g) = s.strip_prefix('@').or_else(|| s.strip_prefix('#')) {
        Destination::Group(group::group_id(g))
    } else {
        Destination::Call(callsign::pack(s))
    }
}

/// Build the message and burst; shared with `sim`.
pub fn build(args: &TxArgs, msg_id: u16, phase: u8) -> Result<(Message, Burst)> {
    let message = Message {
        sender: callsign::pack(&args.from),
        dest: parse_dest(&args.to),
        msg_id,
        ack_req: args.ack,
        text: args.text.clone(),
    };
    let blocks = message.to_blocks().context("encode message")?;
    let burst = Burst::new(FrameKind::Long, phase, blocks)?;
    Ok((message, burst))
}

pub fn run(args: TxArgs) -> Result<()> {
    let mut rng = rand::rng();
    let phase = args.phase.unwrap_or_else(|| rng.random_range(0..16));
    let msg_id: u16 = rng.random();
    let (message, burst) = build(&args, msg_id, phase)?;
    let (payload, raw) = message.payload();
    let tx = Transmitter::new(args.fs);
    let audio = tx.render(&burst, args.lane, args.profile, args.offset_hz)?;
    let lead = (args.lead_s * args.fs as f64) as usize;
    let tail = (args.tail_s * args.fs as f64) as usize;
    let mut out = vec![0f32; lead];
    out.extend_from_slice(&audio);
    out.extend(std::iter::repeat_n(0f32, tail));
    crate::wav::write(&args.output, &out, args.fs)?;
    println!(
        "{} chars -> {} payload bits{} -> {} block(s), {} symbols, {:.1} s on air ({} profile, lane {}, phase {}, msg_id {:04X})",
        message.text.chars().count(),
        payload.len(),
        if raw { " (raw)" } else { "" },
        burst.blocks.len(),
        burst.symbols(),
        burst.airtime_s(args.profile),
        args.profile,
        args.lane,
        phase,
        msg_id
    );
    println!("wrote {}", args.output.display());
    Ok(())
}
