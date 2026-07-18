use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{Ornament, TempoMap, Ticks, Trigger};

fn default_master_gain() -> f32 {
    1.0
}

/// A complete composition: the unit that gets saved/loaded as a project file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Song {
    pub title: String,
    pub tempo: TempoMap,
    /// Live-monitoring output fader, 1.0 = unity. Only affects real-time
    /// playback (`engine::realtime`) — offline exports (`render_song`) are
    /// peak-normalized afterward, which makes a uniform pre-normalize gain a
    /// no-op, so this is deliberately not applied there.
    #[serde(default = "default_master_gain")]
    pub master_gain: f32,
    pub tracks: Vec<Track>,
}

impl Song {
    pub fn new(title: impl Into<String>, bpm: f32) -> Self {
        Self {
            title: title.into(),
            tempo: TempoMap::constant(bpm),
            master_gain: 1.0,
            tracks: Vec::new(),
        }
    }

    pub fn ticks_to_secs(&self, ticks: Ticks) -> f32 {
        self.tempo.ticks_to_secs(ticks)
    }

    /// Length of the composition, in seconds — the last clip's end, converted
    /// through the tempo map. 0.0 for an empty song. No ring-out tail (unlike
    /// the render engine's padded buffers) — this is "how long is the piece",
    /// not "how many samples to allocate".
    pub fn duration_secs(&self) -> f32 {
        let mut last_tick = 0;
        for track in &self.tracks {
            for clip in &track.clips {
                if let Some(pattern) = track.patterns.get(clip.pattern) {
                    last_tick = last_tick.max(clip.start + pattern.length);
                }
            }
        }
        self.ticks_to_secs(last_tick)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("song serialization cannot fail")
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        std::fs::write(path, self.to_json())
    }

    pub fn load(path: &Path) -> std::io::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        Self::from_json(&json).map_err(std::io::Error::other)
    }
}

/// One instrument lane: its patterns plus where they sit on the timeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub name: String,
    /// Instrument catalog name (see `instrument::by_name`).
    pub instrument: String,
    /// Linear gain, 1.0 = unity. Mute/solo are implemented by the UI zeroing
    /// this (with its own value remembered client-side to restore later)
    /// rather than as separate persisted flags.
    pub gain: f32,
    /// -1.0 (left) ..= 1.0 (right).
    pub pan: f32,
    /// Insert-effect wet mix, 0.0..=1.0 — reverb send (`dsp::PlateReverb`).
    #[serde(default)]
    pub reverb: f32,
    /// Insert-effect wet mix, 0.0..=1.0 — echo send (`dsp::EchoDelay`).
    #[serde(default)]
    pub delay: f32,
    pub patterns: Vec<Pattern>,
    pub clips: Vec<Clip>,
}

impl Track {
    pub fn new(name: impl Into<String>, instrument: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            instrument: instrument.into(),
            gain: 1.0,
            pan: 0.0,
            reverb: 0.0,
            delay: 0.0,
            patterns: Vec::new(),
            clips: Vec::new(),
        }
    }

    /// Resolve clips into absolutely-positioned notes, ready for rendering.
    pub fn flatten(&self) -> Vec<PatternNote> {
        let mut notes = Vec::new();
        for clip in &self.clips {
            let Some(pattern) = self.patterns.get(clip.pattern) else {
                continue;
            };
            for note in &pattern.notes {
                let mut absolute = note.clone();
                absolute.start += clip.start;
                notes.push(absolute);
            }
        }
        notes.sort_by_key(|n| n.start);
        notes
    }
}

/// A reusable phrase (a taal cycle, a melody line) placed on the timeline via
/// [`Clip`]s.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pattern {
    pub name: String,
    pub length: Ticks,
    pub notes: Vec<PatternNote>,
}

/// One note inside a pattern, in ticks relative to the pattern start.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatternNote {
    pub start: Ticks,
    pub duration: Ticks,
    pub trigger: Trigger,
    /// 0.0..=1.0
    pub velocity: f32,
    #[serde(default)]
    pub ornament: Ornament,
}

impl PatternNote {
    pub fn new(start: Ticks, duration: Ticks, trigger: Trigger, velocity: f32) -> Self {
        Self {
            start,
            duration,
            trigger,
            velocity,
            ornament: Ornament::Plain,
        }
    }

    pub fn with_ornament(mut self, ornament: Ornament) -> Self {
        self.ornament = ornament;
        self
    }
}

/// An instance of a pattern on the track timeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Clip {
    /// Index into the track's `patterns`.
    pub pattern: usize,
    pub start: Ticks,
}
