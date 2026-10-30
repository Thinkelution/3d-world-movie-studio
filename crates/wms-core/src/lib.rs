//! Shared foundations for 3D World Movie Studio.
//!
//! Everything here must be deterministic: the same inputs give the same
//! outputs on every machine, every run.

pub mod color;
pub mod rng;
pub mod time;

pub use color::Rgb;
pub use rng::DetRng;
pub use time::FrameClock;
