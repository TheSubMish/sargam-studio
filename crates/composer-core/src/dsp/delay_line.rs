/// A circular delay line with fractional (linearly interpolated) reads.
///
/// The workhorse behind string models (Karplus-Strong, waveguides), echo and
/// chorus effects. Fractional reads matter for tuning: an integer-only delay
/// quantizes pitch audibly in the upper octaves.
pub struct DelayLine {
    buffer: Vec<f32>,
    write_pos: usize,
}

impl DelayLine {
    /// `max_delay` is the longest delay (in samples) this line must support.
    pub fn new(max_delay: usize) -> Self {
        Self {
            buffer: vec![0.0; max_delay.max(2)],
            write_pos: 0,
        }
    }

    pub fn clear(&mut self) {
        self.buffer.fill(0.0);
    }

    pub fn write(&mut self, sample: f32) {
        self.buffer[self.write_pos] = sample;
        self.write_pos = (self.write_pos + 1) % self.buffer.len();
    }

    /// Read the sample written `delay` samples ago (1 <= delay < len).
    pub fn read(&self, delay: usize) -> f32 {
        let len = self.buffer.len();
        let idx = (self.write_pos + len - delay.clamp(1, len - 1)) % len;
        self.buffer[idx]
    }

    /// Linearly interpolated read at a non-integer delay.
    pub fn read_fractional(&self, delay: f32) -> f32 {
        let whole = delay.floor();
        let frac = delay - whole;
        let a = self.read(whole as usize);
        let b = self.read(whole as usize + 1);
        a + frac * (b - a)
    }

    pub fn max_delay(&self) -> usize {
        self.buffer.len() - 1
    }
}
