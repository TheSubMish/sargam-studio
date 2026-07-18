use super::{AllpassFilter, DelayLine, OnePoleLowPass};

/// One resonant comb: a delay line with damped feedback. The reverb's decay
/// tail is the sum of several of these at incommensurate lengths.
struct Comb {
    delay: DelayLine,
    delay_samples: usize,
    feedback: f32,
    damping: OnePoleLowPass,
}

impl Comb {
    fn new(delay_samples: usize, feedback: f32, damping_cutoff_hz: f32, sample_rate: f32) -> Self {
        Self {
            delay: DelayLine::new(delay_samples + 2),
            delay_samples,
            feedback,
            damping: OnePoleLowPass::new(sample_rate, damping_cutoff_hz),
        }
    }

    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        let y = self.delay.read(self.delay_samples);
        let damped = self.damping.tick(y);
        self.delay.write(x + damped * self.feedback);
        y
    }
}

/// A compact Freeverb-style plate reverb: four parallel damped combs summed,
/// then two series allpass diffusers. Fixed room character (the UI exposes a
/// single wet-mix knob per track, not the underlying parameters) — reused by
/// every instrument's reverb send, mono in and out.
pub struct PlateReverb {
    combs: [Comb; 4],
    allpass: [AllpassFilter; 2],
}

impl PlateReverb {
    pub fn new(sample_rate: f32) -> Self {
        // Comb/allpass lengths are Freeverb's classic 44.1kHz tuning, scaled
        // to the actual sample rate so the room character stays consistent.
        let scale = sample_rate / 44_100.0;
        let scaled = |samples: usize| ((samples as f32) * scale).round() as usize;

        Self {
            combs: [
                Comb::new(scaled(1_557), 0.84, 6_000.0, sample_rate),
                Comb::new(scaled(1_617), 0.83, 5_400.0, sample_rate),
                Comb::new(scaled(1_491), 0.82, 5_800.0, sample_rate),
                Comb::new(scaled(1_422), 0.81, 5_000.0, sample_rate),
            ],
            allpass: [
                AllpassFilter::new(scaled(556), 0.5),
                AllpassFilter::new(scaled(225), 0.5),
            ],
        }
    }

    #[inline]
    pub fn tick(&mut self, x: f32) -> f32 {
        let mut sum = 0.0;
        for comb in &mut self.combs {
            sum += comb.tick(x);
        }
        sum *= 0.25;
        for ap in &mut self.allpass {
            sum = ap.tick(sum);
        }
        sum
    }
}
