use crate::error::ModemError;
use crate::params::samples_per_unit;

/// Symbol length profile (SPEC §4). Tones and coding are identical; only
/// the symbol period differs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Profile {
    /// 32 ms symbols, 31.25 Bd.
    Fast,
    /// 160 ms symbols, 6.25 Bd.
    Slow,
}

impl Profile {
    pub const ALL: [Profile; 2] = [Profile::Fast, Profile::Slow];

    /// Symbol period in 32 ms shaping units.
    pub const fn units_per_symbol(self) -> usize {
        match self {
            Profile::Fast => 1,
            Profile::Slow => 5,
        }
    }

    pub fn symbol_s(self) -> f64 {
        0.032 * self.units_per_symbol() as f64
    }

    pub fn baud(self) -> f64 {
        1.0 / self.symbol_s()
    }

    pub fn samples_per_symbol(self, fs: u32) -> Result<usize, ModemError> {
        Ok(samples_per_unit(fs)? * self.units_per_symbol())
    }

    pub fn name(self) -> &'static str {
        match self {
            Profile::Fast => "fast",
            Profile::Slow => "slow",
        }
    }
}

impl std::fmt::Display for Profile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

impl std::str::FromStr for Profile {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "fast" | "f" => Ok(Profile::Fast),
            "slow" | "s" => Ok(Profile::Slow),
            other => Err(format!("unknown profile '{other}', expected fast or slow")),
        }
    }
}
