//! Rule-based folk composition generator.
//!
//! `generate(seed)` builds a complete, editable [`Song`] — the same JSON a
//! user would save from the app — by picking a taal, a scale and a root note
//! from the seed, then laying out a fixed form (intro, A, B, A', outro) over
//! madal, damphu, harmonium drone, a bansuri lead and a tungna answer.
//!
//! It is deterministic (same seed, same song), allocation-happy, and meant for
//! the offline control path only — never call it from the audio thread.

use crate::model::{
    taal_by_name, Clip, Ornament, Pattern, PatternNote, Song, Taal, Track, Trigger, TICKS_PER_BEAT,
};

/// A generated song plus the marketplace metadata that describes it.
#[derive(Debug, Clone)]
pub struct GeneratedSong {
    pub song: Song,
    pub genre: &'static str,
    pub description: String,
    /// One of the emoji the marketplace UI maps to an illustration.
    pub emoji: &'static str,
}

/// SplitMix64: tiny, fast, good enough for musical choices, no dependencies.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n` (n > 0).
    fn below(&mut self, n: u32) -> u32 {
        (self.next_u64() % n as u64) as u32
    }

    fn chance(&mut self, p: f32) -> bool {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32 <= p
    }

    fn range_f32(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32 * (hi - lo)
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u32) as usize]
    }
}

struct Style {
    taal: &'static str,
    bpm: (f32, f32),
    scale: &'static [i32],
    scale_name: &'static str,
    emoji: &'static str,
}

const STYLES: [Style; 3] = [
    Style {
        taal: "Jhyaure",
        bpm: (118.0, 146.0),
        scale: &[0, 2, 4, 7, 9],
        scale_name: "major pentatonic",
        emoji: "🪶",
    },
    Style {
        taal: "Tamang Selo",
        bpm: (96.0, 118.0),
        scale: &[0, 3, 5, 7, 10],
        scale_name: "minor pentatonic",
        emoji: "🥁",
    },
    Style {
        taal: "Teej",
        bpm: (108.0, 132.0),
        scale: &[0, 2, 3, 5, 7, 9, 10],
        scale_name: "dorian",
        emoji: "🪕",
    },
];

const ADJECTIVES: [&str; 14] = [
    "Misty", "Golden", "Monsoon", "Festival", "Hilltop", "Terraced", "Morning", "River", "Pine",
    "Lantern", "Harvest", "Village", "Snowmelt", "Twilight",
];
const NOUNS: [&str; 14] = [
    "Dance",
    "Lullaby",
    "Procession",
    "Echoes",
    "Pastures",
    "Bazaar",
    "Trail",
    "Wind",
    "Fields",
    "Courtyard",
    "Crossing",
    "Blossoms",
    "Journey",
    "Gathering",
];

/// Eighth-note slot, in ticks. All melodic rhythm is built on this grid.
const SLOT: u32 = TICKS_PER_BEAT / 2;

/// One melody note before it is turned into a [`PatternNote`]: slot position,
/// length in slots, and a scale-degree index (can exceed the scale length to
/// climb into the next octave).
struct Step {
    start: u32,
    len: u32,
    degree: i32,
}

/// Scale-degree index -> MIDI pitch: `base` plus whole octaves for every
/// `scale.len()` indices, plus the scale offset.
fn degree_pitch(base: i32, scale: &[i32], degree: i32) -> f32 {
    let n = scale.len() as i32;
    let octave = degree.div_euclid(n);
    let idx = degree.rem_euclid(n) as usize;
    (base + 12 * octave + scale[idx]) as f32
}

/// A random-walk melody of `cycles` cycles that always resolves to the tonic.
/// `density` is the chance a slot-run is a note rather than a rest, `lo..=hi`
/// bounds the degree walk.
fn gen_melody(
    rng: &mut Rng,
    total_slots: u32,
    lo: i32,
    hi: i32,
    start_degree: i32,
    rest_chance: f32,
    lengths: &[u32],
) -> Vec<Step> {
    let mut steps: Vec<Step> = Vec::new();
    let mut pos = 0;
    let mut degree = start_degree.clamp(lo, hi);
    while pos < total_slots {
        let len = (*rng.pick(lengths)).min(total_slots - pos);
        // Always sound on the downbeat so every phrase starts clearly.
        if pos == 0 || !rng.chance(rest_chance) {
            let leap = rng.chance(0.12);
            let step = if leap {
                *rng.pick(&[-3, 3, -4, 4])
            } else {
                *rng.pick(&[-2, -1, -1, 0, 1, 1, 2])
            };
            degree = (degree + step).clamp(lo, hi);
            steps.push(Step {
                start: pos,
                len,
                degree,
            });
        }
        pos += len;
    }
    // Cadence: land on the lowest allowed degree (the tonic for motif A, an
    // open second degree for B) and hold it to the end of the phrase.
    if let Some(last) = steps.last_mut() {
        last.degree = lo.max(0);
        last.len = (total_slots - last.start).min(8);
    }
    steps
}

fn steps_to_pattern(
    rng: &mut Rng,
    name: &str,
    steps: &[Step],
    length_ticks: u32,
    base: i32,
    scale: &[i32],
    ornamented: bool,
) -> Pattern {
    let mut notes = Vec::with_capacity(steps.len());
    for (i, s) in steps.iter().enumerate() {
        let velocity = rng.range_f32(0.55, 0.9);
        let mut note = PatternNote::new(
            s.start * SLOT,
            s.len * SLOT,
            Trigger::Pitch(degree_pitch(base, scale, s.degree)),
            velocity,
        );
        let is_last = i + 1 == steps.len();
        if ornamented && s.len >= 2 {
            if is_last {
                note = note.with_ornament(Ornament::Gamak {
                    depth_semitones: 0.4,
                    rate_hz: 5.5,
                });
            } else if rng.chance(0.28) {
                note = note.with_ornament(Ornament::Meend {
                    from_semitones: *rng.pick(&[-1.0, -2.0, -3.0]),
                    secs: rng.range_f32(0.15, 0.35),
                });
            } else if rng.chance(0.12) {
                note = note.with_ornament(Ornament::Gamak {
                    depth_semitones: rng.range_f32(0.25, 0.5),
                    rate_hz: rng.range_f32(5.0, 6.5),
                });
            }
        }
        notes.push(note);
    }
    Pattern {
        name: name.into(),
        length: length_ticks,
        notes,
    }
}

fn clip_at(pattern: usize, cycle: u32, cycle_ticks: u32) -> Clip {
    Clip {
        pattern,
        start: cycle * cycle_ticks,
    }
}

/// Number of cycles in the fixed form: intro 2, A 4, B 4, A' 4, outro 2.
pub const FORM_CYCLES: u32 = 16;

/// Build a complete song from `seed`. Same seed, same song.
pub fn generate(seed: u64) -> GeneratedSong {
    let mut rng = Rng(seed);
    let style = &STYLES[rng.below(STYLES.len() as u32) as usize];
    let taal: Taal = taal_by_name(style.taal).expect("built-in taal");
    let root = 57 + rng.below(6) as i32; // A3..D4
    let bpm = rng.range_f32(style.bpm.0, style.bpm.1).round();
    let n = style.scale.len() as i32;

    let cycle_ticks = taal.ticks_per_cycle();
    let slots_per_cycle = cycle_ticks / SLOT;
    let phrase_slots = slots_per_cycle * 2; // melodies are two cycles long
    let phrase_ticks = cycle_ticks * 2;

    let title = format!("{} {}", rng.pick(&ADJECTIVES), rng.pick(&NOUNS));
    let mut song = Song::new(title, bpm);

    // Madal carries the groove start to finish.
    let mut madal = Track::new("Madal", "Madal");
    madal.gain = 0.95;
    madal.pan = -0.25;
    madal.reverb = 0.12;
    madal.patterns.push(taal.to_pattern(FORM_CYCLES));
    madal.clips.push(clip_at(0, 0, cycle_ticks));
    song.tracks.push(madal);

    // Damphu doubles it from the first melodic section until the outro.
    let mut damphu = Track::new("Damphu", "Damphu");
    damphu.gain = 0.6;
    damphu.pan = 0.3;
    damphu.patterns.push(taal.to_pattern(FORM_CYCLES - 4));
    damphu.clips.push(clip_at(0, 2, cycle_ticks));
    song.tracks.push(damphu);

    // Harmonium: a root-fifth drone that moves with the harmony every 2 cycles.
    let mut harmonium = Track::new("Harmonium", "Harmonium");
    harmonium.gain = 0.5;
    harmonium.reverb = 0.2;
    let blocks = (FORM_CYCLES - 2) / 2;
    for b in 0..blocks {
        let degree = if b == blocks - 1 {
            0 // settle on the tonic for the ending
        } else {
            *rng.pick(&[0, 0, 2, n - 2, n - 1]) // tonic-heavy, with a few colors
        };
        let low = degree_pitch(root, style.scale, degree);
        harmonium.patterns.push(Pattern {
            name: format!("Drone {}", b + 1),
            length: phrase_ticks,
            notes: vec![
                PatternNote::new(0, phrase_ticks, Trigger::Pitch(low), 0.6),
                PatternNote::new(0, phrase_ticks, Trigger::Pitch(low + 7.0), 0.5),
            ],
        });
        harmonium
            .clips
            .push(clip_at(b as usize, 2 + b * 2, cycle_ticks));
    }
    song.tracks.push(harmonium);

    // Bansuri lead: motif A, a contrasting motif B, back to A, then a held close.
    let lead_base = root + 7;
    let mut bansuri = Track::new("Bansuri", "Bansuri");
    bansuri.gain = 0.8;
    bansuri.pan = 0.15;
    bansuri.reverb = 0.25;
    let lengths = [1, 1, 2, 2, 2, 3, 4];
    let steps_a = gen_melody(&mut rng, phrase_slots, 0, n + 1, n / 2, 0.14, &lengths);
    let steps_b = gen_melody(&mut rng, phrase_slots, 1, n + 1, n, 0.18, &lengths);
    let pat_a = steps_to_pattern(
        &mut rng,
        "Lead A",
        &steps_a,
        phrase_ticks,
        lead_base,
        style.scale,
        true,
    );
    let pat_b = steps_to_pattern(
        &mut rng,
        "Lead B",
        &steps_b,
        phrase_ticks,
        lead_base,
        style.scale,
        true,
    );
    let close = Pattern {
        name: "Close".into(),
        length: phrase_ticks,
        notes: vec![PatternNote::new(
            0,
            phrase_ticks - SLOT,
            Trigger::Pitch(degree_pitch(lead_base, style.scale, 0)),
            0.7,
        )
        .with_ornament(Ornament::Gamak {
            depth_semitones: 0.35,
            rate_hz: 5.5,
        })],
    };
    bansuri.patterns.extend([pat_a, pat_b, close]);
    for cycle in [2, 4, 10, 12] {
        bansuri.clips.push(clip_at(0, cycle, cycle_ticks));
    }
    for cycle in [6, 8] {
        bansuri.clips.push(clip_at(1, cycle, cycle_ticks));
    }
    bansuri.clips.push(clip_at(2, 14, cycle_ticks));
    song.tracks.push(bansuri);

    // Tungna answers in the B section and the return of A.
    let mut tungna = Track::new("Tungna", "Tungna");
    tungna.gain = 0.7;
    tungna.pan = -0.1;
    tungna.reverb = 0.1;
    tungna.delay = 0.12;
    let steps_t = gen_melody(&mut rng, phrase_slots, 0, n, 1, 0.3, &[1, 1, 1, 2, 2, 3]);
    tungna.patterns.push(steps_to_pattern(
        &mut rng,
        "Answer",
        &steps_t,
        phrase_ticks,
        root,
        style.scale,
        true,
    ));
    for cycle in [6, 8, 12] {
        tungna.clips.push(clip_at(0, cycle, cycle_ticks));
    }
    song.tracks.push(tungna);

    let description = format!(
        "Auto-composed by Sargam Studio's rule-based folk generator: a {} groove in {} at {} BPM. \
         Generated from seed {:016x}. Download it free and open it in Sargam to remix.",
        style.taal, style.scale_name, bpm as u32, seed
    );

    GeneratedSong {
        song,
        genre: "Folk",
        description,
        emoji: style.emoji,
    }
}
