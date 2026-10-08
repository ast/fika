//! Channel description: multipath (Watterson), frequency shift and drift,
//! interferers, impulsive noise and the rig's passband. Presets follow
//! CCIR 520 / ITU-R F.1487.

#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    pub delay_ms: f64,
    pub gain_db: f64,
    /// Doppler spread, 2σ of the Gaussian Doppler spectrum, Hz. 0 = static.
    pub spread_hz: f64,
}

/// Something else on the band, at a level relative to the wanted signal.
#[derive(Clone, Debug, PartialEq)]
pub enum Interferer {
    /// Steady carrier.
    Carrier { freq_hz: f64, level_db: f64 },
    /// Keyed CW with random Morse-like timing.
    Cw {
        freq_hz: f64,
        level_db: f64,
        wpm: f64,
    },
    /// 45.45 Bd FSK with 170 Hz shift around `center_hz`.
    Rtty { center_hz: f64, level_db: f64 },
    /// 31.25 Bd BPSK with cosine-shaped reversals (PSK31-like).
    Psk31 { freq_hz: f64, level_db: f64 },
}

/// Poisson bursts of wideband noise, like lightning static.
#[derive(Clone, Debug, PartialEq)]
pub struct Impulsive {
    /// Bursts per second.
    pub rate_hz: f64,
    /// Burst length, ms.
    pub duration_ms: f64,
    /// Burst RMS level relative to the wanted signal, dB.
    pub level_db: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChannelSpec {
    pub name: String,
    /// Empty means a non-fading, single-path channel.
    pub paths: Vec<Path>,
    /// Frequency shift applied to the whole signal, Hz.
    pub freq_shift_hz: f64,
    /// Linear drift added to the shift, Hz per second.
    pub drift_hz_per_s: f64,
    pub interferers: Vec<Interferer>,
    pub impulsive: Option<Impulsive>,
    /// Apply a 300–2700 Hz SSB receiver passband.
    pub bandpass: bool,
}

impl Default for ChannelSpec {
    fn default() -> Self {
        Self::awgn()
    }
}

impl ChannelSpec {
    pub fn awgn() -> Self {
        Self {
            name: "awgn".into(),
            paths: Vec::new(),
            freq_shift_hz: 0.0,
            drift_hz_per_s: 0.0,
            interferers: Vec::new(),
            impulsive: None,
            bandpass: false,
        }
    }

    fn two_path(name: &str, delay_ms: f64, spread_hz: f64) -> Self {
        Self {
            name: name.into(),
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
            ..Self::awgn()
        }
    }

    /// CCIR good = ITU-R F.1487 mid-latitude quiet: 0.5 ms, 0.1 Hz.
    pub fn ccir_good() -> Self {
        Self::two_path("good", 0.5, 0.1)
    }

    /// CCIR moderate = mid-latitude moderate: 1 ms, 0.5 Hz.
    pub fn ccir_moderate() -> Self {
        Self::two_path("moderate", 1.0, 0.5)
    }

    /// CCIR poor = mid-latitude disturbed: 2 ms, 1 Hz.
    pub fn ccir_poor() -> Self {
        Self::two_path("poor", 2.0, 1.0)
    }

    /// Single-path flat Rayleigh fading with the given spread.
    pub fn flat(spread_hz: f64) -> Self {
        Self {
            name: "flat".into(),
            paths: vec![Path {
                delay_ms: 0.0,
                gain_db: 0.0,
                spread_hz,
            }],
            ..Self::awgn()
        }
    }

    /// ITU-R F.1487 table of two-path channels: (name, delay ms, spread Hz).
    pub const ITU: [(&'static str, f64, f64); 10] = [
        ("low-quiet", 0.5, 0.5),
        ("low-moderate", 2.0, 1.5),
        ("low-disturbed", 6.0, 10.0),
        ("mid-quiet", 0.5, 0.1),
        ("mid-moderate", 1.0, 0.5),
        ("mid-disturbed", 2.0, 1.0),
        ("mid-nvis", 7.0, 1.0),
        ("high-quiet", 1.0, 0.5),
        ("high-moderate", 3.0, 10.0),
        ("high-disturbed", 7.0, 30.0),
    ];

    pub fn itu(name: &str) -> Option<Self> {
        Self::ITU
            .iter()
            .find(|(n, _, _)| *n == name)
            .map(|&(n, d, s)| Self::two_path(n, d, s))
    }

    pub fn from_name(name: &str) -> Option<Self> {
        let n = name.to_ascii_lowercase();
        match n.as_str() {
            "awgn" => Some(Self::awgn()),
            "good" => Some(Self::ccir_good()),
            "moderate" | "mod" => Some(Self::ccir_moderate()),
            "poor" => Some(Self::ccir_poor()),
            "flat" => Some(Self::flat(0.5)),
            _ => Self::itu(&n),
        }
    }

    pub fn names() -> Vec<&'static str> {
        let mut v = vec!["awgn", "good", "moderate", "poor", "flat"];
        v.extend(Self::ITU.iter().map(|(n, _, _)| *n));
        v
    }

    pub fn with_interferer(mut self, i: Interferer) -> Self {
        self.interferers.push(i);
        self
    }

    pub fn with_impulsive(mut self, rate_hz: f64, duration_ms: f64, level_db: f64) -> Self {
        self.impulsive = Some(Impulsive {
            rate_hz,
            duration_ms,
            level_db,
        });
        self
    }

    pub fn with_drift(mut self, hz_per_s: f64) -> Self {
        self.drift_hz_per_s = hz_per_s;
        self
    }

    pub fn with_bandpass(mut self) -> Self {
        self.bandpass = true;
        self
    }

    pub fn with_shift(mut self, hz: f64) -> Self {
        self.freq_shift_hz = hz;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_resolve() {
        assert_eq!(
            ChannelSpec::from_name("mid-moderate").unwrap().paths,
            ChannelSpec::ccir_moderate().paths
        );
        assert!(ChannelSpec::from_name("high-disturbed").is_some());
        assert!(ChannelSpec::from_name("nope").is_none());
        assert_eq!(ChannelSpec::names().len(), 15);
    }
}
