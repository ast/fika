//! fika command line: build bursts, decode recordings, run simulations.

mod cmd_multi;
mod cmd_rx;
mod cmd_sim;
mod cmd_tx;
mod decode;
mod wav;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "fika",
    version,
    about = "HF group chat mode: encode, decode and simulate"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Encode a message into a WAV file.
    Tx(cmd_tx::TxArgs),
    /// Decode every burst in a WAV file.
    Rx(cmd_rx::RxArgs),
    /// Monte-Carlo decode rate over a simulated channel.
    Sim(cmd_sim::SimArgs),
    /// Several stations transmitting at once in one passband.
    Multi(cmd_multi::MultiArgs),
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Tx(a) => cmd_tx::run(a),
        Command::Rx(a) => cmd_rx::run(a),
        Command::Sim(a) => cmd_sim::run(a),
        Command::Multi(a) => cmd_multi::run(a),
    }
}
