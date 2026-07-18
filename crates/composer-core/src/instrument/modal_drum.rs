//! Shared voice for all struck membrane drums. Individual drums (damphu,
//! madal, later dhime) supply only a [`VoicingFn`]: stroke + velocity → mode
//! table + strike character.

use crate::dsp::{ExpDecay, ModalBank, Mode, OnePoleLowPass, WhiteNoise};
use crate::model::{NoteParams, Stroke, Trigger};

use super::Voice;

pub const MAX_MODES: usize = 8;

/// How one stroke sounds: which modes ring and what the hand impact is like.
#[derive(Clone, Copy)]
pub struct Voicing {
    pub modes: [Mode; MAX_MODES],
    pub mode_count: usize,
    /// Cutoff of the strike-noise filter (hand "thud" vs "tick").
    pub strike_cutoff_hz: f32,
    /// Length of the strike-noise burst.
    pub strike_secs: f32,
}

pub type VoicingFn = fn(Stroke, f32) -> Voicing;

/// Build a Voicing from a fundamental and mode ratios. `brightness_tilt` > 0
/// favors upper modes, < 0 favors the fundamental; higher modes always decay
/// faster than the fundamental.
pub fn membrane_voicing(
    fundamental_hz: f32,
    ratios: &[f32],
    ring_secs: f32,
    brightness_tilt: f32,
    strike_cutoff_hz: f32,
) -> Voicing {
    let mut modes = [Mode {
        freq_hz: 0.0,
        decay_secs: 0.0,
        amplitude: 0.0,
    }; MAX_MODES];
    let count = ratios.len().min(MAX_MODES);
    for (i, (mode, ratio)) in modes.iter_mut().zip(ratios).enumerate().take(count) {
        mode.freq_hz = fundamental_hz * ratio;
        mode.decay_secs = ring_secs / (1.0 + i as f32 * 0.8);
        mode.amplitude = (1.0 + brightness_tilt * i as f32).max(0.05) / (1.0 + i as f32);
    }
    Voicing {
        modes,
        mode_count: count,
        strike_cutoff_hz,
        strike_secs: 0.006,
    }
}

pub struct ModalDrumVoice {
    bank: ModalBank,
    noise: WhiteNoise,
    strike_env: ExpDecay,
    strike_filter: OnePoleLowPass,
    voicing: VoicingFn,
    silent_samples: u32,
    active: bool,
}

impl ModalDrumVoice {
    pub fn new(sample_rate: f32, noise_seed: u32, voicing: VoicingFn) -> Self {
        Self {
            bank: ModalBank::new(sample_rate),
            noise: WhiteNoise::new(noise_seed),
            strike_env: ExpDecay::new(sample_rate),
            strike_filter: OnePoleLowPass::new(sample_rate, 800.0),
            voicing,
            silent_samples: 0,
            active: false,
        }
    }
}

impl Voice for ModalDrumVoice {
    fn note_on(&mut self, note: NoteParams) {
        // Pitched input (e.g. auditioning from a piano roll) plays as a center hit.
        let stroke = match note.trigger {
            Trigger::Stroke(s) => s,
            Trigger::Pitch(_) => Stroke::Bass,
        };
        let v = (self.voicing)(stroke, note.velocity);
        self.bank.set_modes(&v.modes[..v.mode_count]);
        self.bank.reset();

        // Hard hits carry more of the hand's impact noise into the skin.
        self.strike_filter
            .set_cutoff(v.strike_cutoff_hz * (0.6 + 0.8 * note.velocity));
        self.strike_filter.reset();
        self.strike_env.trigger(note.velocity, v.strike_secs);

        self.silent_samples = 0;
        self.active = true;
    }

    fn note_off(&mut self) {
        // Drums ring freely; the stroke itself decided its decay.
    }

    fn kill(&mut self) {
        self.bank.reset();
        self.strike_env.trigger(0.0, 0.001);
        self.active = false;
    }

    #[inline]
    fn tick(&mut self) -> f32 {
        if !self.active {
            return 0.0;
        }
        let strike = self
            .strike_filter
            .tick(self.noise.tick() * self.strike_env.tick());
        let out = self.bank.tick(strike * 6.0);

        if out.abs() < 1e-5 && self.strike_env.is_silent() {
            self.silent_samples += 1;
            if self.silent_samples > 2_000 {
                self.active = false;
            }
        } else {
            self.silent_samples = 0;
        }
        out
    }

    fn is_active(&self) -> bool {
        self.active
    }
}
