//! Channel description and the CCIR 520 / ITU-R F.1487 presets.

#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    pub delay_ms: f64,
    pub gain_db: f64,
    /// Doppler spread, 2σ of the Gaussian Doppler spectrum, Hz. 0 = static.
    pub spread_hz: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChannelSpec {
    pub name: &'static str,
    /// Empty means a non-fading, single-path channel.
    pub paths: Vec<Path>,
    /// Extra frequency shift applied to the whole signal, Hz.
    pub freq_shift_hz: f64,
}

impl ChannelSpec {
    pub fn awgn() -> Self {
        Self {
            name: "awgn",
            paths: Vec::new(),
            freq_shift_hz: 0.0,
        }
    }

    fn two_path(name: &'static str, delay_ms: f64, spread_hz: f64) -> Self {
        Self {
            name,
            paths: vec![
                Path {
                    delay_ms: 0.0,
                    gain_db: 0.0,
                    spread_hz,
                },
                Path {
                    delay_ms,
                    gain_db: 0.0,
                    spread_hz,
                },
            ],
            freq_shift_hz: 0.0,
        }
    }

    /// CCIR good: 0.5 ms, 0.1 Hz.
    pub fn ccir_good() -> Self {
        Self::two_path("good", 0.5, 0.1)
    }

    /// CCIR moderate: 1 ms, 0.5 Hz.
    pub fn ccir_moderate() -> Self {
        Self::two_path("moderate", 1.0, 0.5)
    }

    /// CCIR poor: 2 ms, 1 Hz.
    pub fn ccir_poor() -> Self {
        Self::two_path("poor", 2.0, 1.0)
    }

    /// Single-path flat Rayleigh fading with the given spread.
    pub fn flat(spread_hz: f64) -> Self {
        Self {
            name: "flat",
            paths: vec![Path {
                delay_ms: 0.0,
                gain_db: 0.0,
                spread_hz,
            }],
            freq_shift_hz: 0.0,
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "awgn" => Some(Self::awgn()),
            "good" => Some(Self::ccir_good()),
            "moderate" | "mod" => Some(Self::ccir_moderate()),
            "poor" => Some(Self::ccir_poor()),
            "flat" => Some(Self::flat(0.5)),
            _ => None,
        }
    }

    pub const NAMES: [&'static str; 5] = ["awgn", "good", "moderate", "poor", "flat"];
}
