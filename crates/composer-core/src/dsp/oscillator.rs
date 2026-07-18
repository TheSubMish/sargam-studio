use std::f32::consts::TAU;

/// A shared phase accumulator, 0.0..1.0 per period. Instruments that need
/// phase-locked harmonics (bansuri's overtone stack) drive it directly.
pub struct Phase {
    phase: f32,
    sample_rate: f32,
}

impl Phase {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            phase: 0.0,
            sample_rate,
        }
    }

    /// Advance by one sample at `freq_hz`, returning the phase before wrapping.
    #[inline]
    pub fn advance(&mut self, freq_hz: f32) -> f32 {
        let p = self.phase;
        self.phase += freq_hz / self.sample_rate;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        p
    }

    #[inline]
    pub fn delta(&self, freq_hz: f32) -> f32 {
        freq_hz / self.sample_rate
    }
}

/// Sine oscillator — also serves as an LFO for vibrato/tremolo.
pub struct Sine {
    phase: Phase,
}

impl Sine {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            phase: Phase::new(sample_rate),
        }
    }

    #[inline]
    pub fn tick(&mut self, freq_hz: f32) -> f32 {
        (self.phase.advance(freq_hz) * TAU).sin()
    }
}

/// Band-limited sawtooth via polyBLEP — the raw buzz of a free reed. A naive
/// saw aliases badly; polyBLEP smooths the discontinuity just enough to kill
/// the aliasing without dulling the tone.
pub struct Saw {
    phase: Phase,
}

impl Saw {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            phase: Phase::new(sample_rate),
        }
    }

    #[inline]
    pub fn tick(&mut self, freq_hz: f32) -> f32 {
        let dt = self.phase.delta(freq_hz);
        let t = self.phase.advance(freq_hz);
        2.0 * t - 1.0 - poly_blep(t, dt)
    }
}

#[inline]
fn poly_blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let x = t / dt;
        2.0 * x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + 2.0 * x + 1.0
    } else {
        0.0
    }
}
