//! A full two-track song: damphu playing the built-in Jhyaure taal under a
//! pentatonic tungna melody. Saves the song as JSON and renders stereo WAV.
//!
//! Run with: `cargo run --example jhyaure_demo`

use std::path::Path;

use composer_core::engine::{render_song, write_wav};
use composer_core::model::{
    taal_by_name, Clip, PatternNote, Pattern, Song, Track, Trigger, TICKS_PER_BEAT,
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

    // In jhyaure the beat is an eighth note, so 160 BPM is a relaxed dance tempo.
    let mut song = Song::new("Jhyaure Demo", 160.0);

    // Damphu: 4 cycles of the built-in Jhyaure taal (6 beats per cycle).
    let taal = taal_by_name("Jhyaure").expect("built-in taal");
    let mut damphu = Track::new("Damphu", "Damphu");
    damphu.gain = 0.9;
    damphu.pan = -0.2;
    damphu.patterns.push(taal.to_pattern(4));
    damphu.clips.push(Clip { pattern: 0, start: 0 });
    song.tracks.push(damphu);

    // Tungna: a 12-beat (2-cycle) A-minor-pentatonic melody, played twice.
    let melody = Pattern {
        name: "Melody".into(),
        length: 12 * TICKS_PER_BEAT,
        notes: vec![
            note(0.0, 1.0, 57.0, 0.9),  // A3
            note(1.0, 1.0, 60.0, 0.6),  // C4
            note(2.0, 1.0, 62.0, 0.7),  // D4
            note(3.0, 1.5, 64.0, 0.85), // E4
            note(4.5, 0.5, 62.0, 0.5),  // D4
            note(5.0, 1.0, 60.0, 0.6),  // C4
            note(6.0, 1.0, 62.0, 0.7),  // D4
            note(7.0, 1.0, 64.0, 0.75), // E4
            note(8.0, 1.0, 67.0, 0.8),  // G4
            note(9.0, 1.5, 69.0, 0.9),  // A4
            note(10.5, 0.5, 67.0, 0.55), // G4
            note(11.0, 1.0, 64.0, 0.7), // E4
        ],
    };
    let mut tungna = Track::new("Tungna", "Tungna");
    tungna.pan = 0.2;
    tungna.patterns.push(melody);
    tungna.clips.push(Clip { pattern: 0, start: 0 });
    tungna.clips.push(Clip {
        pattern: 0,
        start: 12 * TICKS_PER_BEAT,
    });
    song.tracks.push(tungna);

    let json_path = Path::new("jhyaure_demo.song.json");
    song.save(json_path).expect("failed to save song");
    println!("Saved project to {}", json_path.display());

    let audio = render_song(&song, SAMPLE_RATE).expect("render failed");
    let wav_path = Path::new("jhyaure_demo.wav");
    write_wav(wav_path, &audio, 2, SAMPLE_RATE as u32).expect("failed to write wav");
    println!(
        "Wrote {} ({:.1}s stereo)",
        wav_path.display(),
        audio.len() as f32 / 2.0 / SAMPLE_RATE
    );
}
