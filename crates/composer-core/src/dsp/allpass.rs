use super::DelayLine;

/// Schroeder allpass filter: flattens the spectrum while smearing an impulse
/// into a dense decay — the diffusion stage inside [`super::PlateReverb`].
pub struct AllpassFilter {
    delay: DelayLine,
    delay_samples: usize,
    gain: f32,
}

impl AllpassFilter {
    pub fn new(delay_samples: usize, gain: f32) -> Self {
        Self {
            delay: DelayLine::new(delay_samples + 2),
            delay_samples,
            gain,
        }
    }

    #[inline]
    pub fn tick(&mut self, x: f32) -> f32 {
        let delayed = self.delay.read(self.delay_samples);
        let y = -self.gain * x + delayed;
        self.delay.write(x + self.gain * y);
        y
    }
}
