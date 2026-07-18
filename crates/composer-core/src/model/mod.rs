//! Musical vocabulary and the song domain model.

pub mod editor;
pub mod song;
pub mod taal;
pub mod tempo;

pub use editor::SongEditor;
pub use song::{Clip, Pattern, PatternNote, Song, Track};
pub use taal::{builtin_taals, taal_by_name, Taal, TaalStroke};
pub use tempo::{TempoChange, TempoMap};

use serde::{Deserialize, Serialize};

/// Musical time resolution. 480 divides evenly by 2/3/4/5/6/8/16 — enough for
/// straight, triplet, and swung subdivisions.
pub const TICKS_PER_BEAT: u32 = 480;
pub type Ticks = u32;

/// Convert a (possibly fractional) MIDI note number to frequency
/// (A4 = 69 = 440 Hz, 12-TET).
pub fn midi_to_hz(midi: f32) -> f32 {
    440.0 * 2.0_f32.powf((midi - 69.0) / 12.0)
}

/// Percussion stroke vocabulary, shared across drums. Each instrument maps the
/// strokes it supports to its own sound (and may treat some as synonyms).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stroke {
    /// Deep center hit (damphu "dha", madal left head "din").
    Bass,
    /// Ringing open hit (madal right head "ta").
    Open,
    /// Edge/rim hit ("tin"/"ka").
    Rim,
    /// Sharp closed slap.
    Slap,
    /// Damped/muted hit.
    Mute,
}

impl Stroke {
    /// General-MIDI-flavored percussion key, for MIDI interchange and for
    /// mapping sampled drum kits.
    pub fn to_gm_key(self) -> u8 {
        match self {
            Stroke::Bass => 36, // kick
            Stroke::Rim => 37,  // side stick
            Stroke::Slap => 39, // hand clap
            Stroke::Mute => 42, // closed hi-hat
            Stroke::Open => 47, // low-mid tom
        }
    }

    pub fn from_gm_key(key: u8) -> Option<Self> {
        match key {
            36 => Some(Stroke::Bass),
            37 => Some(Stroke::Rim),
            39 => Some(Stroke::Slap),
            42 => Some(Stroke::Mute),
            47 => Some(Stroke::Open),
            _ => None,
        }
    }
}

/// What sets a voice sounding: a pitch for melodic instruments, a stroke for
/// percussion. Fractional MIDI allows microtonal inflections.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Trigger {
    Pitch(f32),
    Stroke(Stroke),
}

/// Melodic ornamentation — the expressive pitch movement that makes sarangi,
/// bansuri, and shehnai lines sound like folk playing instead of a keyboard.
/// Instruments that physically can't bend (harmonium's fixed reeds) ignore it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum Ornament {
    #[default]
    Plain,
    /// Slide into the note from `from_semitones` away (negative = from below),
    /// arriving over `secs`.
    Meend { from_semitones: f32, secs: f32 },
    /// Rapid oscillation around the written pitch.
    Gamak { depth_semitones: f32, rate_hz: f32 },
}

/// Everything a voice needs to start sounding.
#[derive(Debug, Clone, Copy)]
pub struct NoteParams {
    pub trigger: Trigger,
    /// 0.0..=1.0
    pub velocity: f32,
    pub ornament: Ornament,
}

impl NoteParams {
    pub fn pitch(midi: f32, velocity: f32) -> Self {
        Self {
            trigger: Trigger::Pitch(midi),
            velocity,
            ornament: Ornament::Plain,
        }
    }

    pub fn stroke(stroke: Stroke, velocity: f32) -> Self {
        Self {
            trigger: Trigger::Stroke(stroke),
            velocity,
            ornament: Ornament::Plain,
        }
    }

    pub fn with_ornament(mut self, ornament: Ornament) -> Self {
        self.ornament = ornament;
        self
    }
}

/// A scheduled note in seconds — the engine-facing, tempo-resolved form.
/// Pattern editing uses tick-based [`PatternNote`]; this is what tracks
/// flatten into for rendering.
#[derive(Debug, Clone, Copy)]
pub struct NoteEvent {
    pub start_secs: f32,
    pub duration_secs: f32,
    pub trigger: Trigger,
    /// 0.0..=1.0
    pub velocity: f32,
    pub ornament: Ornament,
}

impl NoteEvent {
    pub fn pitch(start_secs: f32, duration_secs: f32, midi: f32, velocity: f32) -> Self {
        Self {
            start_secs,
            duration_secs,
            trigger: Trigger::Pitch(midi),
            velocity,
            ornament: Ornament::Plain,
        }
    }

    pub fn stroke(start_secs: f32, duration_secs: f32, stroke: Stroke, velocity: f32) -> Self {
        Self {
            start_secs,
            duration_secs,
            trigger: Trigger::Stroke(stroke),
            velocity,
            ornament: Ornament::Plain,
        }
    }

    pub fn with_ornament(mut self, ornament: Ornament) -> Self {
        self.ornament = ornament;
        self
    }

    pub fn params(&self) -> NoteParams {
        NoteParams {
            trigger: self.trigger,
            velocity: self.velocity,
            ornament: self.ornament,
        }
    }
}
