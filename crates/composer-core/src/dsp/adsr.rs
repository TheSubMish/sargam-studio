/// Linear attack-decay-sustain-release envelope for sustaining instruments
/// (harmonium, bansuri, shehnai). Plucked/struck instruments use [`super::ExpDecay`].
///
/// Attack always starts from the *current* level, so fast retriggers don't click.
pub struct Adsr {
    sample_rate: f32,
    attack_secs: f32,
    decay_secs: f32,
    sustain_level: f32,
    release_secs: f32,
    stage: Stage,
    level: f32,
    release_step: f32,
}

#[derive(PartialEq, Clone, Copy)]
enum Stage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

impl Adsr {
    pub fn new(sample_rate: f32, attack: f32, decay: f32, sustain: f32, release: f32) -> Self {
        Self {
            sample_rate,
            attack_secs: attack.max(0.001),
            decay_secs: decay.max(0.001),
            sustain_level: sustain.clamp(0.0, 1.0),
            release_secs: release.max(0.001),
            stage: Stage::Idle,
            level: 0.0,
            release_step: 0.0,
        }
    }

    pub fn note_on(&mut self) {
        self.stage = Stage::Attack;
    }

    pub fn note_off(&mut self) {
        self.release_step = self.level / (self.release_secs * self.sample_rate);
        self.stage = Stage::Release;
    }

    /// Jump straight to silence (voice stealing / transport stop).
    pub fn kill(&mut self) {
        self.stage = Stage::Idle;
        self.level = 0.0;
    }

    #[inline]
    pub fn tick(&mut self) -> f32 {
        match self.stage {
            Stage::Idle => {}
            Stage::Attack => {
                self.level += 1.0 / (self.attack_secs * self.sample_rate);
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                self.level -= (1.0 - self.sustain_level) / (self.decay_secs * self.sample_rate);
                if self.level <= self.sustain_level {
                    self.level = self.sustain_level;
                    self.stage = Stage::Sustain;
                }
            }
            Stage::Sustain => {}
            Stage::Release => {
                self.level -= self.release_step;
                if self.level <= 0.0 {
                    self.level = 0.0;
                    self.stage = Stage::Idle;
                }
            }
        }
        self.level
    }

    pub fn is_finished(&self) -> bool {
        self.stage == Stage::Idle && self.level <= 0.0
    }
}
