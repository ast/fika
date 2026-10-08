use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ModemError {
    #[error("lane {0} out of range 0..4")]
    InvalidLane(usize),
    #[error("sample rate {0} Hz is not a multiple of the 31.25 Hz bin grid")]
    InvalidSampleRate(u32),
    #[error("block {index} has {got} info bytes, expected {expected}")]
    BlockSize {
        index: usize,
        got: usize,
        expected: usize,
    },
    #[error("burst must have between 1 and 8 blocks, got {0}")]
    BlockCount(usize),
}
