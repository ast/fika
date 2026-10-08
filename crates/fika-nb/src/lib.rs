//! Non-binary LDPC over GF(2^6) for 64-tone MFSK.
//!
//! The decoder takes, per coded symbol, a probability vector over the 64
//! field elements built from the 64 tone energies, so an overlapping
//! sender costs about one bit of six per symbol instead of half the
//! bit-level likelihoods. Sum-product with Hadamard-transformed check
//! nodes. Codes are built deterministically from a seed with an in-crate
//! generator, so every build of fika agrees on the code.

pub mod code;
pub mod decoder;
pub mod gf64;
pub mod likelihood;
pub mod rng;

pub use code::NbCode;
pub use decoder::Decoder;
pub use likelihood::{LikelihoodParams, symbol_likelihoods};
