//! Turns instruments + note events into audio.
//!
//! Two paths sharing the same voice logic:
//! - offline ([`render_track`], [`render_song`]) — exports and tests;
//! - real-time ([`realtime::EngineCore`] driven by [`player::Player`]) — live
//!   playback behind lock-free command/event queues.

pub mod player;
pub mod realtime;
mod render;

pub use player::{Player, PlayerError};
pub use realtime::{compile, compile_with, Command, CompiledSong, EngineCore, EngineEvent};
pub use render::{
    render_song, render_song_with, render_stems, render_track, write_wav, write_wav_with_depth,
    BitDepth, RenderError,
};
