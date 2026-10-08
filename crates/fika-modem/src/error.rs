use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ModemError {
    #[error("sample rate {0} Hz is not a multiple of the 37.5 Hz tone grid")]
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
