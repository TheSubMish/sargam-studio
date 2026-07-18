use std::f32::consts::TAU;

use crate::model::Ornament;

/// Turns a note's [`Ornament`] into a per-sample pitch trajectory (in MIDI
/// units, so instruments convert to Hz however they like). Shared by every
/// melodic voice that can bend.
pub struct PitchLine {
    sample_rate: f32,
    base_midi: f32,
    ornament: Ornament,
    t_secs: f32,
}

impl PitchLine {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            base_midi: 69.0,
            ornament: Ornament::Plain,
            t_secs: 0.0,
        }
    }

    pub fn trigger(&mut self, base_midi: f32, ornament: Ornament) {
        self.base_midi = base_midi;
        self.ornament = ornament;
        self.t_secs = 0.0;
    }

    /// True if the pitch actually moves (lets voices skip per-sample retuning
    /// for plain notes).
    pub fn is_moving(&self) -> bool {
        !matches!(self.ornament, Ornament::Plain)
    }

    /// Current pitch in MIDI units; advances one sample.
    #[inline]
    pub fn tick(&mut self) -> f32 {
        let t = self.t_secs;
        self.t_secs += 1.0 / self.sample_rate;
        match self.ornament {
            Ornament::Plain => self.base_midi,
            Ornament::Meend {
                from_semitones,
                secs,
            } => {
                if t >= secs {
                    self.base_midi
                } else {
                    self.base_midi + from_semitones * (1.0 - t / secs)
                }
            }
            Ornament::Gamak {
                depth_semitones,
                rate_hz,
            } => self.base_midi + depth_semitones * (TAU * rate_hz * t).sin(),
        }
    }
}
