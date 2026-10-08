//! fika physical layer.
//!
//! 16-tone Gaussian-shaped MFSK in a 500 Hz lane with a per-symbol Costas
//! tone permutation, CCSDS LDPC coding, and an asynchronous preamble
//! detector. See `docs/SPEC.md` sections 3 to 8, 10 and 11.

pub mod costas;
pub mod demod;
pub mod energy;
pub mod error;
pub mod frame_kind;
pub mod gfsk;
pub mod hop;
pub mod interleave;
pub mod ldpc;
pub mod params;
pub mod preamble;
pub mod profile;
pub mod resample;
pub mod rx;
pub mod symbols;
pub mod sync;
pub mod tx;

pub use demod::{BlockDecode, Demodulator};
pub use energy::EnergyMatrix;
pub use error::ModemError;
pub use frame_kind::FrameKind;
pub use ldpc::Ldpc;
pub use profile::Profile;
pub use rx::Receiver;
pub use sync::{Detection, SyncConfig};
pub use tx::{Burst, Transmitter};
