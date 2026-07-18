use std::f32::consts::TAU;

/// One resonant mode of a vibrating body: a frequency, how long it rings, and
/// how strongly it's excited.
#[derive(Debug, Clone, Copy)]
pub struct Mode {
    pub freq_hz: f32,
    /// Time to decay to -60 dB.
    pub decay_secs: f32,
    pub amplitude: f32,
}

/// Modal synthesis: a bank of two-pole resonators, each ringing at one mode of
/// the modeled body. Feed it a short excitation burst (the "strike") and the
/// modes ring out and decay like a struck membrane, bar, or bell.
///
/// This is the generic engine behind all struck percussion — damphu today,
/// madal/dhime later — which differ only in their mode tables and excitation.
pub struct ModalBank {
    states: Vec<ModeState>,
    sample_rate: f32,
}

struct ModeState {
    // y[n] = a1*y[n-1] + a2*y[n-2] + gain*x[n]
    a1: f32,
    a2: f32,
    gain: f32,
    y1: f32,
    y2: f32,
}

impl ModalBank {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            // Reserve typical mode counts up front so `set_modes` in a voice's
            // note_on never reallocates on the audio thread.
            states: Vec::with_capacity(8),
            sample_rate,
        }
    }

    /// Configure the bank's modes. Allocates — call from voice setup, not the
    /// per-sample path. Modes near/above Nyquist are skipped.
    pub fn set_modes(&mut self, modes: &[Mode]) {
        self.states.clear();
        for mode in modes {
            if mode.freq_hz >= self.sample_rate * 0.45 {
                continue;
            }
            let omega = TAU * mode.freq_hz / self.sample_rate;
            let r = 0.001_f32.powf(1.0 / (mode.decay_secs * self.sample_rate).max(1.0));
            self.states.push(ModeState {
                a1: 2.0 * r * omega.cos(),
                a2: -r * r,
                // (1 - r) tames the resonator's ~1/(1-r) gain at resonance so
                // long-ringing modes don't dwarf short ones.
                gain: mode.amplitude * (1.0 - r),
                y1: 0.0,
                y2: 0.0,
            });
        }
    }

    pub fn reset(&mut self) {
        for s in &mut self.states {
            s.y1 = 0.0;
            s.y2 = 0.0;
        }
    }

    /// Drive the bank with an excitation sample, get the summed ring-out.
    #[inline]
    pub fn tick(&mut self, input: f32) -> f32 {
        let mut sum = 0.0;
        for s in &mut self.states {
            let y = s.a1 * s.y1 + s.a2 * s.y2 + s.gain * input;
            s.y2 = s.y1;
            s.y1 = y;
            sum += y;
        }
        sum
    }
}
