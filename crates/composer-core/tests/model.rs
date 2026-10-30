use composer_core::model::{
    builtin_taals, taal_by_name, Clip, Pattern, PatternNote, Song, Stroke, Track, Trigger,
    TICKS_PER_BEAT,
};

fn demo_song() -> Song {
    let mut song = Song::new("Test Song", 160.0);
    let mut track = Track::new("Damphu", "Damphu");
    track.gain = 0.9;
    track.pan = -0.2;
    track.patterns.push(Pattern {
        name: "Groove".into(),
        length: 6 * TICKS_PER_BEAT,
        notes: vec![PatternNote::new(
            0,
            TICKS_PER_BEAT,
            Trigger::Stroke(Stroke::Bass),
            1.0,
        )],
    });
    track.clips.push(Clip {
        pattern: 0,
        start: 0,
    });
    song.tracks.push(track);
    song
}

#[test]
fn song_json_roundtrip() {
    let song = demo_song();
    let json = song.to_json();
    let restored = Song::from_json(&json).expect("valid json");
    assert_eq!(song, restored);
}

#[test]
fn builtin_taals_are_well_formed() {
    let taals = builtin_taals();
    assert!(taals.len() >= 7);
    let mut names: Vec<_> = taals.iter().map(|t| t.name.to_lowercase()).collect();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), taals.len(), "taal names must be unique");
    for taal in &taals {
        assert!(!taal.strokes.is_empty(), "{} has no strokes", taal.name);
        for s in &taal.strokes {
            assert!(
                s.slot < taal.slots_per_cycle(),
                "{}: slot {} out of range",
                taal.name,
                s.slot
            );
            assert!(s.velocity > 0.0 && s.velocity <= 1.0);
        }
        // Every taal must land its "sam" (cycle start) — slot 0 accented.
        assert_eq!(taal.strokes[0].slot, 0, "{} misses the sam", taal.name);
    }
}

#[test]
fn taal_expands_to_correct_pattern() {
    let jhyaure = taal_by_name("jhyaure").expect("case-insensitive lookup");
    let cycles = 2;
    let pattern = jhyaure.to_pattern(cycles);

    assert_eq!(pattern.length, 6 * TICKS_PER_BEAT * cycles);
    assert_eq!(pattern.notes.len(), jhyaure.strokes.len() * cycles as usize);
    // Second cycle is the first shifted by exactly one cycle length.
    let per_cycle = jhyaure.strokes.len();
    for i in 0..per_cycle {
        assert_eq!(
            pattern.notes[i + per_cycle].start,
            pattern.notes[i].start + jhyaure.ticks_per_cycle()
        );
    }
}

#[test]
fn track_flatten_offsets_clips() {
    let mut song = demo_song();
    song.tracks[0].clips.push(Clip {
        pattern: 0,
        start: 6 * TICKS_PER_BEAT,
    });
    let notes = song.tracks[0].flatten();
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0].start, 0);
    assert_eq!(notes[1].start, 6 * TICKS_PER_BEAT);
}
