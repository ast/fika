//! fika physical layer, v2.
//!
//! 64-tone Gaussian-shaped MFSK across the whole 300–2700 Hz passband with a
//! per-symbol Costas tone permutation, GF(64) non-binary LDPC decoded at
//! symbol level (so overlapping senders cost about one bit of six per
//! symbol instead of killing each other), and an asynchronous preamble
//! detector. See `docs/SPEC.md`.

pub mod costas;
pub mod demod;
pub mod energy;
pub mod error;
pub mod frame_kind;
pub mod gfsk;
pub mod hop;
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
pub use profile::Profile;
pub use rx::Receiver;
pub use sync::{Detection, SyncConfig};
pub use tx::{Burst, Transmitter};
