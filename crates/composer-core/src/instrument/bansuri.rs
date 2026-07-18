use std::f32::consts::TAU;

use crate::dsp::{Adsr, OnePoleLowPass, Phase, Sine, WhiteNoise};
use crate::model::{midi_to_hz, NoteParams, Trigger};

use super::ornament::PitchLine;
use super::{Instrument, Voice};

/// Bansuri — the bamboo flute.
///
/// Model: a phase-locked stack of the first three harmonics (a flute is nearly
/// sinusoidal, warming with harder blowing), filtered breath noise, and a
/// delayed-onset vibrato. Fully supports meend/gamak ornaments — the essential
/// vocabulary of bansuri playing.
#[derive(Default)]
pub struct Bansuri;

impl Instrument for Bansuri {
    fn name(&self) -> &str {
        "Bansuri"
    }

    fn make_voice(&self, sample_rate: f32) -> Box<dyn Voice> {
        Box::new(BansuriVoice {
            phase: Phase::new(sample_rate),
            pitch: PitchLine::new(sample_rate),
            vibrato: Sine::new(sample_rate),
            breath: WhiteNoise::new(0xBA45),
            breath_filter: OnePoleLowPass::new(sample_rate, 3_000.0),
            envelope: Adsr::new(sample_rate, 0.07, 0.05, 0.9, 0.18),
            sample_rate,
            gain: 0.0,
            harmonic2: 0.3,
            harmonic3: 0.1,
            t_secs: 0.0,
        })
    }
}

struct BansuriVoice {
    phase: Phase,
    pitch: PitchLine,
    vibrato: Sine,
    breath: WhiteNoise,
    breath_filter: OnePoleLowPass,
    envelope: Adsr,
    sample_rate: f32,
    gain: f32,
    harmonic2: f32,
    harmonic3: f32,
    t_secs: f32,
}

impl Voice for BansuriVoice {
    fn note_on(&mut self, note: NoteParams) {
        let midi = match note.trigger {
            Trigger::Pitch(m) => m,
            // Breath attack with no fingered note: a soft airy chiff.
            Trigger::Stroke(_) => {
                self.gain = 0.0;
                self.envelope.note_on();
                return;
            }
        };
        self.pitch.trigger(midi, note.ornament);
        self.gain = 0.3 + 0.7 * note.velocity;
        // Blowing harder brightens the tone (stronger upper harmonics).
        self.harmonic2 = 0.15 + 0.35 * note.velocity;
        self.harmonic3 = 0.04 + 0.16 * note.velocity;
        self.t_secs = 0.0;
        self.envelope.note_on();
    }

    fn note_off(&mut self) {
        self.envelope.note_off();
    }

    fn kill(&mut self) {
        self.envelope.kill();
    }

    #[inline]
    fn tick(&mut self) -> f32 {
        self.t_secs += 1.0 / self.sample_rate;

        // Vibrato eases in after ~0.3 s, as a player would.
        let vibrato_depth = 0.12 * (self.t_secs / 0.3).min(1.0);
        let midi = self.pitch.tick() + vibrato_depth * self.vibrato.tick(5.2);
        let freq = midi_to_hz(midi);

        let p = self.phase.advance(freq) * TAU;
        let tone = p.sin() + self.harmonic2 * (2.0 * p).sin() + self.harmonic3 * (3.0 * p).sin();
        let breath = self.breath_filter.tick(self.breath.tick()) * 0.045;

        (tone + breath) * self.envelope.tick() * self.gain
    }

    fn is_active(&self) -> bool {
        !self.envelope.is_finished()
    }
}
