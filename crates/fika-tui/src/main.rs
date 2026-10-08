//! fika terminal UI: chat over HF with a live heard list and waterfall.

mod app;
mod line_edit;
mod ui;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;

use fika_station::{Config, Station};

#[derive(Parser)]
#[command(name = "fika-tui", version, about = "fika: HF group chat")]
struct Cli {
    /// Configuration file (TOML).
    #[arg(short, long, default_value = "fika.toml")]
    config: PathBuf,
    /// List audio devices and exit.
    #[arg(long)]
    list_audio: bool,
    /// Print an example configuration and exit.
    #[arg(long)]
    example_config: bool,
    /// Headless self-test: send this text once, wait for it to come back
    /// (software loopback or acoustically), print the result and exit.
    #[arg(long, value_name = "TEXT")]
    selftest: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.list_audio {
        for line in fika_station::audio::list_devices() {
            println!("{line}");
        }
        return Ok(());
    }
    if cli.example_config {
        print!("{}", Config::example());
        return Ok(());
    }
    let cfg = Config::load(&cli.config).with_context(|| {
        format!(
            "loading {} (try --example-config > fika.toml)",
            cli.config.display()
        )
    })?;
    let station = Station::start(cfg)?;
    if let Some(text) = cli.selftest {
        return selftest(station, &text);
    }
    let terminal = ratatui::init();
    let result = app::App::new(station).run(terminal);
    ratatui::restore();
    result
}

fn selftest(mut station: Station, text: &str) -> Result<()> {
    use fika_station::StationEvent;
    use std::time::{Duration, Instant};

    println!(
        "selftest: in {} / out {} @ {} Hz, lane {} {}",
        station.audio_names.0,
        station.audio_names.1,
        station.cfg.audio.sample_rate,
        station.lane,
        station.profile
    );
    // Let the receiver settle its noise baseline before transmitting.
    let settle = Instant::now();
    while settle.elapsed() < Duration::from_secs(3) {
        station.poll();
        std::thread::sleep(Duration::from_millis(50));
    }
    println!(
        "selftest: input level {:+.0} dBFS, sending {text:?}",
        station.level_db
    );
    station.send_text(text)?;
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut airtime = 0.0;
    while Instant::now() < deadline {
        for ev in station.poll() {
            match ev {
                StationEvent::TxStarted { airtime_s, .. } => {
                    airtime = airtime_s;
                    println!("selftest: transmitting, {airtime_s:.1} s");
                }
                StationEvent::TxFinished => println!("selftest: transmission done, listening"),
                StationEvent::Detected { det, .. } => println!(
                    "selftest: burst detected lane {} {} {:+.1} dB offset {:+.1} Hz",
                    det.lane,
                    det.profile,
                    det.snr_db(),
                    det.freq_offset_hz
                ),
                StationEvent::Message {
                    message,
                    det,
                    blocks_ok,
                    total,
                    ..
                } => {
                    let ok = message.text == text;
                    println!(
                        "selftest: decoded {:?} at {:+.1} dB, {blocks_ok}/{total} blocks: {}",
                        message.text,
                        det.snr_db(),
                        if ok { "PASS" } else { "text differs" }
                    );
                    if ok {
                        return Ok(());
                    }
                }
                StationEvent::BurstFailed { det } => {
                    println!("selftest: burst on lane {} failed to decode", det.lane)
                }
                StationEvent::Log(s) => println!("selftest: {s}"),
                _ => {}
            }
        }
        if airtime > 0.0
            && station.tx_busy.is_none()
            && Instant::now()
                > deadline - Duration::from_secs(40) + Duration::from_secs_f64(airtime)
        {
            // Give the receiver a few seconds after the burst, then give up.
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    anyhow::bail!(
        "selftest: nothing decoded within the time limit (input level {:+.0} dBFS)",
        station.level_db
    )
}
