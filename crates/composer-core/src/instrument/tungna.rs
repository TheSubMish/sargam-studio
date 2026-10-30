use crate::dsp::{ExpDecay, KarplusStrong, OnePoleLowPass};
use crate::model::{midi_to_hz, NoteParams, Trigger};

use super::ornament::PitchLine;
use super::{Instrument, Voice};

/// Tungna — the Himalayan plucked lute (Tamang/Sherpa communities).
///
/// Model: Karplus-Strong string into a one-pole "body" filter approximating the
/// small carved-wood resonator, with velocity driving both loudness and pluck
/// brightness (harder plucks sound brighter, as on the real instrument).
/// Meend/gamak ornaments bend the string by retuning the delay line mid-note.
pub struct Tungna {
    /// Loop feedback: tungna strings ring shorter than a guitar's.
    feedback: f32,
    /// Body filter cutoff shaping the warm, boxy resonator tone.
    body_cutoff_hz: f32,
}

impl Default for Tungna {
    fn default() -> Self {
        Self {
            feedback: 0.994,
            body_cutoff_hz: 2_200.0,
        }
    }
}

impl Instrument for Tungna {
    fn name(&self) -> &str {
        "Tungna"
    }

    fn make_voice(&self, sample_rate: f32) -> Box<dyn Voice> {
        Box::new(TungnaVoice {
            string: KarplusStrong::new(sample_rate, 60.0),
            body: OnePoleLowPass::new(sample_rate, self.body_cutoff_hz),
            envelope: ExpDecay::new(sample_rate),
            pitch: PitchLine::new(sample_rate),
            feedback: self.feedback,
            active: false,
        })
    }
}

struct TungnaVoice {
    string: KarplusStrong,
    body: OnePoleLowPass,
    envelope: ExpDecay,
    pitch: PitchLine,
    feedback: f32,
    active: bool,
}

impl Voice for TungnaVoice {
    fn note_on(&mut self, note: NoteParams) {
        // A stroke on a string instrument reads as a percussive slap on the
        // strings: map it to a dark, heavily damped low pluck.
        let (midi, brightness) = match note.trigger {
            Trigger::Pitch(m) => (m, 1_500.0 + note.velocity * 3_500.0),
            Trigger::Stroke(_) => (45.0, 800.0),
        };
        self.pitch.trigger(midi, note.ornament);
        let start_midi = self.pitch.tick();
        self.string.pluck(
            midi_to_hz(start_midi),
            note.velocity,
            brightness,
            self.feedback,
        );
        self.envelope.trigger(1.0, 2.5);
        self.body.reset();
        self.active = true;
    }

    fn note_off(&mut self) {
        self.string.damp();
        self.envelope.release(0.15);
    }

    fn kill(&mut self) {
        self.envelope.release(0.001);
        self.active = false;
    }

    #[inline]
    fn tick(&mut self) -> f32 {
        if !self.active {
            return 0.0;
        }
        if self.pitch.is_moving() {
            self.string.set_freq(midi_to_hz(self.pitch.tick()));
        }
        let sample = self.body.tick(self.string.tick()) * self.envelope.tick();
        if self.envelope.is_silent() {
            self.active = false;
        }
        sample
    }

    fn is_active(&self) -> bool {
        self.active
    }
}
