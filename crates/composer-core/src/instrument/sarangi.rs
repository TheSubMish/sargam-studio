use crate::dsp::{Adsr, BandPass, OnePoleLowPass, Saw, Sine, WhiteNoise};
use crate::model::{midi_to_hz, NoteParams, Trigger};

use super::ornament::PitchLine;
use super::{Instrument, Voice};

/// Sarangi — the bowed, hollow-bodied folk fiddle of the Gandharva tradition.
///
/// Model: a band-limited sawtooth (the stick-slip bow excitation) with a slow
/// bowed attack, a touch of bow noise, and a delayed-onset vibrato, passed
/// through three parallel band-pass resonators standing in for the carved
/// wooden body's formants. Harder bowing opens the bow-force low-pass, adding
/// the nasal brightness the instrument is known for. Meend (the sarangi's
/// signature slide) and gamak work through the shared [`PitchLine`].
#[derive(Default)]
pub struct Sarangi;

/// Body formants (Hz, Q, level): air/lower-body, main body, nasal upper band.
const BODY_MODES: [(f32, f32, f32); 3] = [
    (300.0, 4.0, 0.55),
    (1_100.0, 3.0, 0.45),
    (2_400.0, 2.5, 0.35),
];

impl Instrument for Sarangi {
    fn name(&self) -> &str {
        "Sarangi"
    }

    fn make_voice(&self, sample_rate: f32) -> Box<dyn Voice> {
        Box::new(SarangiVoice {
            saw: Saw::new(sample_rate),
            pitch: PitchLine::new(sample_rate),
            vibrato: Sine::new(sample_rate),
            bow_noise: WhiteNoise::new(0x5A4A),
            bow_filter: OnePoleLowPass::new(sample_rate, 4_000.0),
            noise_filter: OnePoleLowPass::new(sample_rate, 2_500.0),
            body: BODY_MODES.map(|(hz, q, _)| BandPass::new(sample_rate, hz, q)),
            envelope: Adsr::new(sample_rate, 0.12, 0.08, 0.92, 0.22),
            sample_rate,
            gain: 0.0,
            t_secs: 0.0,
        })
    }
}

struct SarangiVoice {
    saw: Saw,
    pitch: PitchLine,
    vibrato: Sine,
    bow_noise: WhiteNoise,
    bow_filter: OnePoleLowPass,
    noise_filter: OnePoleLowPass,
    body: [BandPass; 3],
    envelope: Adsr,
    sample_rate: f32,
    gain: f32,
    t_secs: f32,
}

impl Voice for SarangiVoice {
    fn note_on(&mut self, note: NoteParams) {
        let midi = match note.trigger {
            Trigger::Pitch(m) => m,
            // No fingered pitch: stay silent rather than invent one.
            Trigger::Stroke(_) => {
                self.gain = 0.0;
                self.envelope.note_on();
                return;
            }
        };
        self.pitch.trigger(midi, note.ornament);
        self.gain = 0.3 + 0.7 * note.velocity;
        // More bow force → brighter, more nasal tone.
        self.bow_filter
            .set_cutoff(1_800.0 + 5_000.0 * note.velocity);
        for b in &mut self.body {
            b.reset();
        }
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

        // Sarangi vibrato is slower and wider than the flute's, and eases in.
        let vibrato_depth = 0.22 * (self.t_secs / 0.4).min(1.0);
        let midi = self.pitch.tick() + vibrato_depth * self.vibrato.tick(5.0);
        let freq = midi_to_hz(midi);

        let bow = self.bow_filter.tick(self.saw.tick(freq));
        let rosin = self.noise_filter.tick(self.bow_noise.tick()) * 0.05;
        let excitation = bow + rosin;

        let mut out = 0.0;
        for (resonator, (_, _, level)) in self.body.iter_mut().zip(BODY_MODES) {
            out += resonator.tick(excitation) * level;
        }
        // Keep some direct string sound so low notes don't vanish between formants.
        out += excitation * 0.25;

        out * self.envelope.tick() * self.gain
    }

    fn is_active(&self) -> bool {
        !self.envelope.is_finished()
    }
}
