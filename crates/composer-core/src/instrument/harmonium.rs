use crate::dsp::{Adsr, OnePoleLowPass, Saw, Sine};
use crate::model::{midi_to_hz, NoteParams, Trigger};

use super::{Instrument, Voice};

/// Harmonium — the hand-pumped reed organ that anchors bhajan and modern folk.
///
/// Model: two slightly detuned band-limited saws (double-reed banks beating
/// against each other) through a low-pass "cabinet" filter, shaped by an ADSR
/// with the reed's soft speak-time, plus a slow bellows tremolo.
///
/// Ornaments are ignored on purpose: fixed metal reeds physically cannot bend.
pub struct Harmonium {
    /// Detune between the two reed banks, in semitones (~5 cents stock).
    detune_semitones: f32,
}

impl Default for Harmonium {
    fn default() -> Self {
        Self {
            detune_semitones: 0.05,
        }
    }
}

impl Instrument for Harmonium {
    fn name(&self) -> &str {
        "Harmonium"
    }

    fn make_voice(&self, sample_rate: f32) -> Box<dyn Voice> {
        Box::new(HarmoniumVoice {
            reed_a: Saw::new(sample_rate),
            reed_b: Saw::new(sample_rate),
            cabinet: OnePoleLowPass::new(sample_rate, 2_200.0),
            envelope: Adsr::new(sample_rate, 0.035, 0.08, 0.85, 0.12),
            bellows: Sine::new(sample_rate),
            detune_ratio: 2.0_f32.powf(self.detune_semitones / 12.0),
            freq_hz: 220.0,
            gain: 0.0,
        })
    }
}

struct HarmoniumVoice {
    reed_a: Saw,
    reed_b: Saw,
    cabinet: OnePoleLowPass,
    envelope: Adsr,
    bellows: Sine,
    detune_ratio: f32,
    freq_hz: f32,
    gain: f32,
}

impl Voice for HarmoniumVoice {
    fn note_on(&mut self, note: NoteParams) {
        let midi = match note.trigger {
            Trigger::Pitch(m) => m,
            // A stroke on a keyboard makes no sound; keep silent but let the
            // voice pass through its (empty) lifecycle.
            Trigger::Stroke(_) => {
                self.gain = 0.0;
                self.envelope.note_on();
                return;
            }
        };
        self.freq_hz = midi_to_hz(midi);
        self.gain = 0.25 + 0.75 * note.velocity;
        // Louder pumping opens the tone up slightly.
        self.cabinet.set_cutoff(1_600.0 + 1_400.0 * note.velocity);
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
        let reeds =
            self.reed_a.tick(self.freq_hz) + self.reed_b.tick(self.freq_hz * self.detune_ratio);
        // Gentle bellows unsteadiness — a real harmonium never holds a note
        // perfectly flat.
        let tremolo = 1.0 + 0.025 * self.bellows.tick(4.3);
        self.cabinet.tick(reeds * 0.5) * self.envelope.tick() * tremolo * self.gain
    }

    fn is_active(&self) -> bool {
        !self.envelope.is_finished()
    }
}
