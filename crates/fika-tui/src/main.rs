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
    let terminal = ratatui::init();
    let result = app::App::new(station).run(terminal);
    ratatui::restore();
    result
}
