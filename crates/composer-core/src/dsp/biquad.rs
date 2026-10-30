use std::f32::consts::TAU;

/// Two-pole band-pass resonator (RBJ biquad, constant 0 dB peak gain).
///
/// Used to give synthesized instruments a body: a few of these in parallel at
/// the formant frequencies of a wooden resonator shape a raw oscillator into
/// something that sounds like it lives in a box.
pub struct BandPass {
    b0: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl BandPass {
    pub fn new(sample_rate: f32, center_hz: f32, q: f32) -> Self {
        // Clamp below Nyquist so any input leaves the filter stable.
        let w0 = TAU * center_hz.clamp(10.0, sample_rate * 0.45) / sample_rate;
        let alpha = w0.sin() / (2.0 * q.max(0.1));
        let a0 = 1.0 + alpha;
        Self {
            b0: alpha / a0,
            a1: -2.0 * w0.cos() / a0,
            a2: (1.0 - alpha) / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }

    #[inline]
    pub fn tick(&mut self, x: f32) -> f32 {
        // b1 = 0 and b2 = -b0 for the band-pass form.
        let y = self.b0 * (x - self.x2) - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}
