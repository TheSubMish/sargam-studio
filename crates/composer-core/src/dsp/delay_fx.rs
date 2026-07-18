use super::{DelayLine, OnePoleLowPass};

/// A single-tap feedback echo (slap-back/delay send). Fixed time and
/// feedback amount — the UI exposes one wet-mix knob per track, not the
/// underlying parameters — mono in, mono out, reused by every instrument's
/// delay send.
pub struct EchoDelay {
    line: DelayLine,
    delay_samples: usize,
    feedback: f32,
    damping: OnePoleLowPass,
}

impl EchoDelay {
    /// `time_secs` is the echo spacing; keep it under the effect's musical
    /// range (roughly 0.1–0.6s) so it reads as a slap-back, not a stutter.
    pub fn new(sample_rate: f32, time_secs: f32) -> Self {
        let delay_samples = (time_secs * sample_rate).round() as usize;
        Self {
            line: DelayLine::new(delay_samples + 2),
            delay_samples,
            feedback: 0.38,
            damping: OnePoleLowPass::new(sample_rate, 3_500.0),
        }
    }

    #[inline]
    pub fn tick(&mut self, x: f32) -> f32 {
        let y = self.line.read(self.delay_samples);
        let damped = self.damping.tick(y);
        self.line.write(x + damped * self.feedback);
        y
    }
}
