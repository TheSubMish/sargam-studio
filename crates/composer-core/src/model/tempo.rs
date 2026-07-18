use serde::{Deserialize, Serialize};

use super::{Ticks, TICKS_PER_BEAT};

/// Piecewise-constant tempo: each change takes effect at its tick and holds
/// until the next. There is always a change at tick 0.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TempoMap {
    changes: Vec<TempoChange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TempoChange {
    pub tick: Ticks,
    pub bpm: f32,
}

impl TempoMap {
    pub fn constant(bpm: f32) -> Self {
        Self {
            changes: vec![TempoChange { tick: 0, bpm }],
        }
    }

    /// Insert (or overwrite) a tempo change, keeping changes sorted.
    pub fn set_change(&mut self, tick: Ticks, bpm: f32) {
        match self.changes.binary_search_by_key(&tick, |c| c.tick) {
            Ok(i) => self.changes[i].bpm = bpm,
            Err(i) => self.changes.insert(i, TempoChange { tick, bpm }),
        }
    }

    pub fn changes(&self) -> &[TempoChange] {
        &self.changes
    }

    pub fn bpm_at(&self, tick: Ticks) -> f32 {
        match self.changes.binary_search_by_key(&tick, |c| c.tick) {
            Ok(i) => self.changes[i].bpm,
            Err(i) => self.changes[i.saturating_sub(1)].bpm,
        }
    }

    /// Absolute time of a tick: sum the durations of every constant-tempo
    /// segment before it.
    pub fn ticks_to_secs(&self, tick: Ticks) -> f32 {
        let mut secs = 0.0;
        for (i, change) in self.changes.iter().enumerate() {
            if change.tick >= tick {
                break;
            }
            let segment_end = self
                .changes
                .get(i + 1)
                .map_or(tick, |next| next.tick.min(tick));
            let segment_ticks = segment_end - change.tick;
            secs += segment_ticks as f32 / TICKS_PER_BEAT as f32 * 60.0 / change.bpm;
        }
        secs
    }
}
