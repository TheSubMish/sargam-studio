/// Exponential decay envelope: starts at `level` on trigger and decays toward zero.
///
/// Natural-sounding for struck/plucked instruments (damphu hit, tungna pluck).
/// A full ADSR will join this file when sustaining instruments (harmonium,
/// shehnai) arrive.
pub struct ExpDecay {
    level: f32,
    multiplier: f32,
    sample_rate: f32,
}

impl ExpDecay {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            level: 0.0,
            multiplier: 1.0,
            sample_rate,
        }
    }

    /// Start the envelope at `peak`, reaching -60 dB after `decay_secs`.
    pub fn trigger(&mut self, peak: f32, decay_secs: f32) {
        self.level = peak;
        let samples = (decay_secs * self.sample_rate).max(1.0);
        // 10^(-60/20) = 0.001 → the -60 dB point
        self.multiplier = 0.001_f32.powf(1.0 / samples);
    }

    /// Force a faster decay from the current level (used for note-off / damping).
    pub fn release(&mut self, release_secs: f32) {
        let samples = (release_secs * self.sample_rate).max(1.0);
        self.multiplier = 0.001_f32.powf(1.0 / samples);
    }

    #[inline]
    pub fn tick(&mut self) -> f32 {
        self.level *= self.multiplier;
        self.level
    }

    pub fn is_silent(&self) -> bool {
        self.level < 1e-5
    }
}
