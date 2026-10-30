use serde::{Deserialize, Serialize};

use super::{Pattern, PatternNote, Stroke, Ticks, Trigger, TICKS_PER_BEAT};

/// A cyclic Nepali rhythm template. Percussion tracks start from a taal and
/// the user edits from there — the folk-musician-friendly alternative to an
/// empty drum grid.
///
/// The cycle is a grid of `beats_per_cycle * subdivisions` slots; each
/// [`TaalStroke`] occupies one slot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Taal {
    pub name: String,
    pub beats_per_cycle: u32,
    /// Grid slots per beat (2 = eighth-note grid on a quarter-note beat).
    pub subdivisions: u32,
    pub strokes: Vec<TaalStroke>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaalStroke {
    /// Grid slot index, 0 <= slot < beats_per_cycle * subdivisions.
    pub slot: u32,
    pub stroke: Stroke,
    pub velocity: f32,
}

impl Taal {
    pub fn slots_per_cycle(&self) -> u32 {
        self.beats_per_cycle * self.subdivisions
    }

    pub fn ticks_per_cycle(&self) -> Ticks {
        self.beats_per_cycle * TICKS_PER_BEAT
    }

    /// Expand `cycles` repetitions of the taal into a percussion pattern.
    pub fn to_pattern(&self, cycles: u32) -> Pattern {
        let slot_ticks = TICKS_PER_BEAT / self.subdivisions;
        let mut notes = Vec::with_capacity(self.strokes.len() * cycles as usize);
        for cycle in 0..cycles {
            let cycle_offset = cycle * self.ticks_per_cycle();
            for s in &self.strokes {
                notes.push(PatternNote::new(
                    cycle_offset + s.slot * slot_ticks,
                    slot_ticks,
                    Trigger::Stroke(s.stroke),
                    s.velocity,
                ));
            }
        }
        Pattern {
            name: self.name.clone(),
            length: self.ticks_per_cycle() * cycles,
            notes,
        }
    }
}

/// Built-in taal starting points.
///
/// These are practical, editable grooves capturing each rhythm's feel — not
/// authoritative transcriptions (the newer entries especially are
/// approximations from the rhythm's general feel). Refine them with folk
/// musicians as the project matures.
pub fn builtin_taals() -> Vec<Taal> {
    fn s(slot: u32, stroke: Stroke, velocity: f32) -> TaalStroke {
        TaalStroke {
            slot,
            stroke,
            velocity,
        }
    }

    vec![
        // Jhyaure: the 6/8 dance lilt of mid-hill folk songs. Beat = one
        // eighth note; cycle = 6 slots grouped 3+3.
        Taal {
            name: "Jhyaure".into(),
            beats_per_cycle: 6,
            subdivisions: 1,
            strokes: vec![
                s(0, Stroke::Bass, 1.0),
                s(2, Stroke::Rim, 0.55),
                s(3, Stroke::Bass, 0.8),
                s(4, Stroke::Rim, 0.5),
                s(5, Stroke::Rim, 0.6),
            ],
        },
        // Tamang Selo: the bouncing damphu groove. 4/4, eighth-note grid.
        Taal {
            name: "Tamang Selo".into(),
            beats_per_cycle: 4,
            subdivisions: 2,
            strokes: vec![
                s(0, Stroke::Bass, 1.0),
                s(2, Stroke::Bass, 0.85),
                s(4, Stroke::Rim, 0.7),
                s(5, Stroke::Bass, 0.75),
                s(6, Stroke::Rim, 0.65),
                s(7, Stroke::Rim, 0.5),
            ],
        },
        // Teej: driving festival rhythm. 4/4, eighth-note grid.
        Taal {
            name: "Teej".into(),
            beats_per_cycle: 4,
            subdivisions: 2,
            strokes: vec![
                s(0, Stroke::Bass, 1.0),
                s(2, Stroke::Rim, 0.6),
                s(3, Stroke::Bass, 0.7),
                s(4, Stroke::Bass, 0.9),
                s(6, Stroke::Rim, 0.6),
                s(7, Stroke::Slap, 0.7),
            ],
        },
        // Deuda: call-and-answer courtship song of the far west. Steady 4/4
        // with a rolling pickup into the second half.
        Taal {
            name: "Deuda".into(),
            beats_per_cycle: 4,
            subdivisions: 2,
            strokes: vec![
                s(0, Stroke::Bass, 1.0),
                s(2, Stroke::Rim, 0.6),
                s(4, Stroke::Bass, 0.85),
                s(5, Stroke::Rim, 0.5),
                s(6, Stroke::Rim, 0.65),
                s(7, Stroke::Open, 0.55),
            ],
        },
        // Maruni: the swaying 6/8 dance rhythm of Tihar. Beat = one eighth
        // note, grouped 3+3 with a lifted second half.
        Taal {
            name: "Maruni".into(),
            beats_per_cycle: 6,
            subdivisions: 1,
            strokes: vec![
                s(0, Stroke::Bass, 1.0),
                s(1, Stroke::Rim, 0.45),
                s(2, Stroke::Rim, 0.55),
                s(3, Stroke::Open, 0.8),
                s(4, Stroke::Rim, 0.5),
                s(5, Stroke::Bass, 0.65),
            ],
        },
        // Dhamari: the dhami-tradition ritual groove. Eight eighth-note slots
        // with an accent pushing against the beat.
        Taal {
            name: "Dhamari".into(),
            beats_per_cycle: 4,
            subdivisions: 2,
            strokes: vec![
                s(0, Stroke::Bass, 1.0),
                s(1, Stroke::Mute, 0.4),
                s(3, Stroke::Slap, 0.8),
                s(4, Stroke::Bass, 0.9),
                s(6, Stroke::Rim, 0.7),
                s(7, Stroke::Slap, 0.6),
            ],
        },
        // Panche Baja: the processional rhythm of the five-instrument wedding
        // band. 7 beats grouped 3+2+2 — an odd, lurching cycle.
        Taal {
            name: "Panche Baja".into(),
            beats_per_cycle: 7,
            subdivisions: 1,
            strokes: vec![
                s(0, Stroke::Bass, 1.0),
                s(1, Stroke::Rim, 0.5),
                s(2, Stroke::Rim, 0.55),
                s(3, Stroke::Open, 0.85),
                s(4, Stroke::Rim, 0.5),
                s(5, Stroke::Bass, 0.8),
                s(6, Stroke::Rim, 0.55),
            ],
        },
    ]
}

/// Look up a built-in taal by (case-insensitive) name.
pub fn taal_by_name(name: &str) -> Option<Taal> {
    builtin_taals()
        .into_iter()
        .find(|t| t.name.eq_ignore_ascii_case(name))
}
