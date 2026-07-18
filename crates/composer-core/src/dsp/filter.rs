use std::f32::consts::TAU;

/// One-pole low-pass filter: `y[n] = y[n-1] + a * (x[n] - y[n-1])`.
///
/// Cheap and stable — used both as a tone/body filter on instrument outputs and
/// as the damping element inside feedback loops (where its gentle 6 dB/oct slope
/// is exactly what a vibrating string's high-frequency loss looks like).
pub struct OnePoleLowPass {
    a: f32,
    y: f32,
    sample_rate: f32,
}

impl OnePoleLowPass {
    pub fn new(sample_rate: f32, cutoff_hz: f32) -> Self {
        let mut f = Self {
            a: 1.0,
            y: 0.0,
            sample_rate,
        };
        f.set_cutoff(cutoff_hz);
        f
    }

    pub fn set_cutoff(&mut self, cutoff_hz: f32) {
        // Impulse-invariant mapping; clamp keeps the filter stable for any input.
        let x = (-TAU * cutoff_hz / self.sample_rate).exp();
        self.a = (1.0 - x).clamp(0.0, 1.0);
    }

    pub fn reset(&mut self) {
        self.y = 0.0;
    }

    /// Low-frequency phase delay in samples. Feedback loops (Karplus-Strong)
    /// must subtract this from their delay length or notes play flat.
    pub fn phase_delay_samples(&self) -> f32 {
        (1.0 - self.a) / self.a
    }

    #[inline]
    pub fn tick(&mut self, x: f32) -> f32 {
        self.y += self.a * (x - self.y);
        self.y
    }
}
