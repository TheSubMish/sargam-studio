//! composer-core — the platform-independent heart of the Nepali folk music composer.
//!
//! Layering (each layer only depends on the ones above it):
//!
//! - [`dsp`]        — reusable signal-processing primitives (delay lines, filters,
//!   envelopes, noise, Karplus-Strong). No notion of "instrument".
//! - [`instrument`] — each Nepali folk instrument as a small module composing `dsp`
//!   blocks behind the common [`instrument::Instrument`] /
//!   [`instrument::Voice`] traits.
//! - [`model`]      — musical vocabulary: pitches, note events (taal/song model grows here).
//! - [`engine`]     — turns instruments + note events into audio (offline render now,
//!   real-time cpal playback in Phase 2).

pub mod dsp;
pub mod engine;
pub mod generate;
pub mod instrument;
pub mod io;
pub mod model;
