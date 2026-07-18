/// White noise via xorshift32 — deterministic given a seed, no external crates,
/// and fast enough to run per-sample. Used as the excitation burst for plucked
/// strings and later as the breath component of wind instruments.
pub struct WhiteNoise {
    state: u32,
}

impl WhiteNoise {
    pub fn new(seed: u32) -> Self {
        Self {
            state: seed.max(1), // xorshift must not start at 0
        }
    }

    /// Uniform in [-1.0, 1.0].
    #[inline]
    pub fn tick(&mut self) -> f32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}
