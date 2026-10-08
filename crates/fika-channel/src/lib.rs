//! HF channel models for simulation: AWGN calibrated to the 2500 Hz
//! reference bandwidth, Watterson multipath fading with CCIR presets,
//! sample-clock error and narrowband interferers.

pub mod awgn;
pub mod clock;
pub mod filter;
pub mod impulsive;
pub mod interference;
pub mod interferer;
pub mod spec;
pub mod watterson;

pub use awgn::{add_awgn, noise_sigma};
pub use clock::resample_ppm;
pub use interference::add_carrier;
pub use spec::{ChannelSpec, Impulsive, Interferer, Path};
pub use watterson::{apply, apply_with_reference};
