use super::{DelayLine, OnePoleLowPass, WhiteNoise};

/// Karplus-Strong plucked-string model.
///
/// A noise burst circulates in a delay line whose length sets the pitch; a
/// low-pass filter in the feedback loop bleeds energy from high frequencies
/// first, exactly like a real string. Shared by every plucked/struck string
/// instrument (tungna today; sarangi pizzicato, arbajo later) — instruments
/// differ only in the parameters and post-filtering they apply around this core.
pub struct KarplusStrong {
    delay: DelayLine,
    loop_filter: OnePoleLowPass,
    noise: WhiteNoise,
    delay_len: f32,
    /// Loop gain per round trip; <1.0, closer to 1.0 = longer ring-out.
    feedback: f32,
    sample_rate: f32,
}

impl KarplusStrong {
    /// `min_freq_hz` bounds the delay-line allocation (lowest playable note).
    pub fn new(sample_rate: f32, min_freq_hz: f32) -> Self {
        let max_delay = (sample_rate / min_freq_hz).ceil() as usize + 2;
        Self {
            delay: DelayLine::new(max_delay),
            loop_filter: OnePoleLowPass::new(sample_rate, 5_000.0),
            noise: WhiteNoise::new(0x5EED),
            delay_len: 100.0,
            feedback: 0.996,
            sample_rate,
        }
    }

    /// Excite the string at `freq_hz`.
    ///
    /// `brightness_hz` is the loop-filter cutoff (how metallic vs. muted the
    /// pluck sounds); `feedback` sets ring-out length (e.g. 0.99 short, 0.999 long).
    pub fn pluck(&mut self, freq_hz: f32, amplitude: f32, brightness_hz: f32, feedback: f32) {
        self.feedback = feedback.clamp(0.0, 0.9999);
        self.loop_filter.set_cutoff(brightness_hz);
        self.loop_filter.reset();
        // The loop filter adds cutoff-dependent delay inside the feedback path;
        // subtract it from the line length or every note plays flat.
        self.delay_len = (self.sample_rate / freq_hz - self.loop_filter.phase_delay_samples())
            .clamp(2.0, self.delay.max_delay() as f32 - 1.0);

        // Fill one period of the delay line with a noise burst — the "pluck".
        self.delay.clear();
        for _ in 0..(self.delay_len as usize + 1) {
            self.delay.write(self.noise.tick() * amplitude);
        }
    }

    /// Re-target the string's pitch mid-note (meend/gamak glides). Cheap
    /// enough to call every sample.
    pub fn set_freq(&mut self, freq_hz: f32) {
        self.delay_len = (self.sample_rate / freq_hz - self.loop_filter.phase_delay_samples())
            .clamp(2.0, self.delay.max_delay() as f32 - 1.0);
    }

    /// Quickly drain the string's energy (finger damping / note-off).
    pub fn damp(&mut self) {
        self.feedback *= 0.9;
        self.loop_filter.set_cutoff(400.0);
    }

    #[inline]
    pub fn tick(&mut self) -> f32 {
        let out = self.delay.read_fractional(self.delay_len);
        let fed_back = self.loop_filter.tick(out) * self.feedback;
        self.delay.write(fed_back);
        out
    }
}
