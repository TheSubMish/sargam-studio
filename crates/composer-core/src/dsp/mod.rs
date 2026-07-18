//! Reusable DSP building blocks.
//!
//! Everything here is instrument-agnostic and allocation-free in its `tick`/process
//! path, so blocks can be composed freely inside real-time voices. New algorithms
//! (waveguides, band-pass banks, reverb, ...) get their own file in this module and
//! become available to every instrument.

pub mod adsr;
pub mod allpass;
pub mod delay_fx;
pub mod delay_line;
pub mod envelope;
pub mod filter;
pub mod karplus_strong;
pub mod modal;
pub mod noise;
pub mod oscillator;
pub mod reverb;

pub use adsr::Adsr;
pub use allpass::AllpassFilter;
pub use delay_fx::EchoDelay;
pub use delay_line::DelayLine;
pub use envelope::ExpDecay;
pub use filter::OnePoleLowPass;
pub use karplus_strong::KarplusStrong;
pub use modal::{ModalBank, Mode};
pub use noise::WhiteNoise;
pub use oscillator::{Phase, Saw, Sine};
pub use reverb::PlateReverb;
