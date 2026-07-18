//! The full ensemble: madal + damphu on a Tamang Selo groove, harmonium
//! drone-chords, bansuri melody with meend/gamak ornaments, tungna answering —
//! with a tempo change into the final section.
//!
//! Renders `ensemble_demo.wav`, saves `ensemble_demo.song.json`, and exports
//! `ensemble_demo.mid`.
//!
//! Run with: `cargo run --example ensemble_demo`
//! Then listen live: `cargo run --example play_song -- ensemble_demo.song.json`

use std::path::Path;

use composer_core::engine::{render_song, write_wav};
use composer_core::io::export_midi;
use composer_core::model::{
    taal_by_name, Clip, Ornament, Pattern, PatternNote, Song, Track, Trigger, TICKS_PER_BEAT,
};

fn note(beat: f32, dur_beats: f32, midi: f32, velocity: f32) -> PatternNote {
    PatternNote::new(
        (beat * TICKS_PER_BEAT as f32) as u32,
        (dur_beats * TICKS_PER_BEAT as f32) as u32,
        Trigger::Pitch(midi),
        velocity,
    )
}

fn main() {
    const SAMPLE_RATE: f32 = 44_100.0;
    const CYCLE: u32 = 4; // Tamang Selo is a 4-beat cycle

    let mut song = Song::new("Himalayan Ensemble", 100.0);
    // Push into the last section a touch faster — folk sets often do.
    song.tempo.set_change(16 * TICKS_PER_BEAT, 112.0);

    let selo = taal_by_name("Tamang Selo").expect("built-in taal");

    // Madal carries the groove for 6 cycles.
    let mut madal = Track::new("Madal", "Madal");
    madal.gain = 0.95;
    madal.pan = -0.25;
    madal.patterns.push(selo.to_pattern(6));
    madal.clips.push(Clip { pattern: 0, start: 0 });
    song.tracks.push(madal);

    // Damphu joins from cycle 3, doubling the groove with its drier voice.
    let mut damphu = Track::new("Damphu", "Damphu");
    damphu.gain = 0.7;
    damphu.pan = 0.3;
    damphu.patterns.push(selo.to_pattern(4));
    damphu.clips.push(Clip {
        pattern: 0,
        start: 2 * CYCLE * TICKS_PER_BEAT,
    });
    song.tracks.push(damphu);

    // Harmonium: held root-fifth dyads underneath (A minor flavor).
    let mut harmonium = Track::new("Harmonium", "Harmonium");
    harmonium.gain = 0.55;
    harmonium.patterns.push(Pattern {
        name: "Drone".into(),
        length: 8 * TICKS_PER_BEAT,
        notes: vec![
            note(0.0, 4.0, 57.0, 0.6), // A3
            note(0.0, 4.0, 64.0, 0.5), // E4
            note(4.0, 4.0, 55.0, 0.6), // G3
            note(4.0, 4.0, 62.0, 0.5), // D4
        ],
    });
    for i in 0..3 {
        harmonium.clips.push(Clip {
            pattern: 0,
            start: i * 8 * TICKS_PER_BEAT,
        });
    }
    song.tracks.push(harmonium);

    // Bansuri: the lead line, sliding into phrase peaks (meend) and shaking
    // the long resolution note (gamak).
    let mut bansuri = Track::new("Bansuri", "Bansuri");
    bansuri.gain = 0.8;
    bansuri.pan = 0.15;
    bansuri.patterns.push(Pattern {
        name: "Lead".into(),
        length: 16 * TICKS_PER_BEAT,
        notes: vec![
            note(0.0, 1.5, 69.0, 0.7),
            note(1.5, 0.5, 67.0, 0.5),
            note(2.0, 2.0, 64.0, 0.65),
            note(4.0, 1.5, 69.0, 0.75).with_ornament(Ornament::Meend {
                from_semitones: -3.0,
                secs: 0.35,
            }),
            note(5.5, 0.5, 72.0, 0.6),
            note(6.0, 2.0, 74.0, 0.85).with_ornament(Ornament::Meend {
                from_semitones: -2.0,
                secs: 0.25,
            }),
            note(8.0, 1.0, 72.0, 0.7),
            note(9.0, 1.0, 69.0, 0.65),
            note(10.0, 2.0, 67.0, 0.6),
            note(12.0, 4.0, 69.0, 0.8).with_ornament(Ornament::Gamak {
                depth_semitones: 0.4,
                rate_hz: 6.0,
            }),
        ],
    });
    bansuri.clips.push(Clip {
        pattern: 0,
        start: 4 * TICKS_PER_BEAT,
    });
    song.tracks.push(bansuri);

    // Tungna: rhythmic answer figure in the last section.
    let mut tungna = Track::new("Tungna", "Tungna");
    tungna.gain = 0.75;
    tungna.pan = -0.1;
    tungna.patterns.push(Pattern {
        name: "Answer".into(),
        length: 8 * TICKS_PER_BEAT,
        notes: vec![
            note(0.0, 0.5, 57.0, 0.85),
            note(0.5, 0.5, 60.0, 0.6),
            note(1.0, 0.5, 62.0, 0.7),
            note(1.5, 0.5, 64.0, 0.8),
            note(2.0, 1.0, 62.0, 0.65),
            note(3.0, 1.0, 60.0, 0.6),
            note(4.0, 2.0, 57.0, 0.9).with_ornament(Ornament::Meend {
                from_semitones: 2.0,
                secs: 0.2,
            }),
            note(6.0, 2.0, 57.0, 0.7),
        ],
    });
    tungna.clips.push(Clip {
        pattern: 0,
        start: 16 * TICKS_PER_BEAT,
    });
    song.tracks.push(tungna);

    song.save(Path::new("ensemble_demo.song.json")).unwrap();
    export_midi(&song, Path::new("ensemble_demo.mid")).unwrap();

    let audio = render_song(&song, SAMPLE_RATE).expect("render failed");
    write_wav(Path::new("ensemble_demo.wav"), &audio, 2, SAMPLE_RATE as u32).unwrap();
    println!(
        "Wrote ensemble_demo.wav ({:.1}s stereo), .song.json and .mid",
        audio.len() as f32 / 2.0 / SAMPLE_RATE
    );
}
