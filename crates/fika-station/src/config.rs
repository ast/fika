//! TOML configuration (`fika-tui -c fika.toml`).

use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub station: StationCfg,
    pub audio: AudioCfg,
    pub rig: RigCfg,
    pub modem: ModemCfg,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct StationCfg {
    /// Your callsign.
    pub call: String,
    /// Four or six character Maidenhead locator, optional.
    pub grid: Option<String>,
    /// Groups you listen to; the first is the default destination.
    pub groups: Vec<String>,
}

impl Default for StationCfg {
    fn default() -> Self {
        Self {
            call: "N0CALL".into(),
            grid: None,
            groups: vec!["fika".into()],
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct AudioCfg {
    /// Input device: "default", "none", or a case-insensitive substring of the device name.
    pub input: String,
    /// Output device: "default", "none", or a substring.
    pub output: String,
    /// Device sample rate; 12000, 24000, 48000 or 96000.
    pub sample_rate: u32,
    /// Feed transmitted audio back into the receiver at playback pace, so
    /// you can hear and decode your own bursts without a radio.
    pub loopback: bool,
    /// Peak transmit level, 0..1 of full scale.
    pub tx_level: f32,
}

impl Default for AudioCfg {
    fn default() -> Self {
        Self {
            input: "default".into(),
            output: "default".into(),
            sample_rate: 48_000,
            loopback: false,
            tx_level: 0.5,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RigKind {
    /// No rig control: audio only (VOX, or no radio at all).
    #[default]
    None,
    /// hamlib rigctld over TCP.
    Rigctld,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct RigCfg {
    pub kind: RigKind,
    /// rigctld address.
    pub host: String,
    /// Delay between PTT on and the first audio sample, ms.
    pub tx_delay_ms: u64,
    /// Silence after the last sample before PTT off, ms.
    pub tx_tail_ms: u64,
    /// Set the rig to PKTUSB with a 3 kHz filter on start.
    pub set_data_mode: bool,
}

impl Default for RigCfg {
    fn default() -> Self {
        Self {
            kind: RigKind::None,
            host: "127.0.0.1:4532".into(),
            tx_delay_ms: 150,
            tx_tail_ms: 100,
            set_data_mode: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct ModemCfg {
    /// Transmit lane 0..3.
    pub lane: usize,
    /// "fast" or "slow".
    pub profile: String,
    /// SYNC detection threshold.
    pub threshold: f32,
}

impl Default for ModemCfg {
    fn default() -> Self {
        Self {
            lane: 1,
            profile: "fast".into(),
            threshold: 40.0,
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let cfg: Config =
            toml::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(self.modem.lane < 4, "modem.lane must be 0..3");
        anyhow::ensure!(
            matches!(self.audio.sample_rate, 12_000 | 24_000 | 48_000 | 96_000),
            "audio.sample_rate must be 12000, 24000, 48000 or 96000"
        );
        self.modem
            .profile
            .parse::<fika_modem::Profile>()
            .map_err(anyhow::Error::msg)?;
        anyhow::ensure!(
            (0.0..=1.0).contains(&self.audio.tx_level),
            "audio.tx_level must be 0..1"
        );
        Ok(())
    }

    /// A commented example configuration.
    pub fn example() -> &'static str {
        r#"# fika station configuration

[station]
call = "SM6WJM"
grid = "JO57"
groups = ["fika"]          # first entry is the default destination

[audio]
# "default", "none", or a substring of the device name (see fika-tui --list-audio).
# IC-705 / FT-891 over USB show up as "USB Audio CODEC".
input = "default"
output = "default"
sample_rate = 48000
# Without a radio: play bursts on the speakers and decode them yourself.
loopback = true
tx_level = 0.5

[rig]
kind = "none"              # "none" (VOX / no radio) or "rigctld"
host = "127.0.0.1:4532"    # e.g. rigctld -m 3085 -r /dev/ic-705a -s 115200
tx_delay_ms = 150
tx_tail_ms = 100
set_data_mode = false

[modem]
lane = 1                   # 0..3
profile = "fast"           # fast or slow
threshold = 40.0
"#
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_parses_and_defaults_fill_in() {
        let cfg: Config = toml::from_str(Config::example()).unwrap();
        cfg.validate().unwrap();
        assert_eq!(cfg.station.call, "SM6WJM");
        let minimal: Config = toml::from_str("[station]\ncall = \"AD8KM\"\n").unwrap();
        assert_eq!(minimal.audio.sample_rate, 48_000);
        assert_eq!(minimal.rig.kind, RigKind::None);
    }
}
