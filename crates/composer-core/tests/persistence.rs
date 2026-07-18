//! Phase 4: undo/redo editor and MIDI interchange.

use composer_core::io::{export_midi, import_midi};
use composer_core::model::{
    taal_by_name, Clip, Pattern, PatternNote, Song, SongEditor, Stroke, Track, Trigger,
    TICKS_PER_BEAT,
};

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("composer-core-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn demo_song() -> Song {
    let mut song = Song::new("Interchange", 140.0);
    song.tempo.set_change(8 * TICKS_PER_BEAT, 90.0);

    let mut damphu = Track::new("Damphu", "Damphu");
    damphu.patterns.push(taal_by_name("Teej").unwrap().to_pattern(2));
    damphu.clips.push(Clip { pattern: 0, start: 0 });
    song.tracks.push(damphu);

    let mut melody = Track::new("Bansuri", "Bansuri");
    melody.patterns.push(Pattern {
        name: "Line".into(),
        length: 4 * TICKS_PER_BEAT,
        notes: vec![
            PatternNote::new(0, TICKS_PER_BEAT, Trigger::Pitch(64.0), 0.8),
            PatternNote::new(TICKS_PER_BEAT, TICKS_PER_BEAT, Trigger::Pitch(67.0), 0.7),
            PatternNote::new(2 * TICKS_PER_BEAT, 2 * TICKS_PER_BEAT, Trigger::Pitch(69.0), 0.9),
        ],
    });
    melody.clips.push(Clip { pattern: 0, start: 0 });
    song.tracks.push(melody);
    song
}

// ------------------------------------------------------------------ editor --

#[test]
fn edits_are_undoable_and_redoable() {
    let mut ed = SongEditor::new(demo_song());
    let original = ed.song().clone();

    ed.add_note(
        1,
        0,
        PatternNote::new(3 * TICKS_PER_BEAT, TICKS_PER_BEAT, Trigger::Pitch(72.0), 0.5),
    );
    assert_eq!(ed.song().tracks[1].patterns[0].notes.len(), 4);

    assert!(ed.undo());
    assert_eq!(*ed.song(), original);
    assert!(ed.redo());
    assert_eq!(ed.song().tracks[1].patterns[0].notes.len(), 4);
}

#[test]
fn new_edit_clears_redo_history() {
    let mut ed = SongEditor::new(demo_song());
    ed.set_tempo(0, 100.0);
    ed.undo();
    assert!(ed.can_redo());
    ed.set_tempo(0, 80.0);
    assert!(!ed.can_redo());
    assert!((ed.song().tempo.bpm_at(0) - 80.0).abs() < 1e-6);
}

#[test]
fn undo_on_empty_stack_is_a_noop() {
    let mut ed = SongEditor::new(demo_song());
    assert!(!ed.undo());
    assert!(!ed.redo());
}

// -------------------------------------------------------------------- midi --

#[test]
fn midi_roundtrip_preserves_notes_and_tempo() {
    let song = demo_song();
    let path = scratch("roundtrip.mid");
    export_midi(&song, &path).expect("export");
    let imported = import_midi(&path).expect("import");

    assert_eq!(imported.title, "Interchange");
    assert_eq!(imported.tracks.len(), 2);

    // Tempo map survives.
    assert!((imported.tempo.bpm_at(0) - 140.0).abs() < 0.5);
    assert!((imported.tempo.bpm_at(8 * TICKS_PER_BEAT) - 90.0).abs() < 0.5);

    // Note starts/counts survive per track (order of tracks preserved).
    for (orig, imp) in song.tracks.iter().zip(&imported.tracks) {
        let orig_notes = orig.flatten();
        let imp_notes = imp.flatten();
        assert_eq!(orig_notes.len(), imp_notes.len(), "track {}", orig.name);
        for (a, b) in orig_notes.iter().zip(&imp_notes) {
            assert_eq!(a.start, b.start);
        }
    }

    // Percussion strokes survive the GM key mapping.
    let strokes: Vec<Stroke> = imported.tracks[0]
        .flatten()
        .iter()
        .map(|n| match n.trigger {
            Trigger::Stroke(s) => s,
            Trigger::Pitch(_) => panic!("percussion note came back pitched"),
        })
        .collect();
    let original: Vec<Stroke> = song.tracks[0]
        .flatten()
        .iter()
        .map(|n| match n.trigger {
            Trigger::Stroke(s) => s,
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(strokes, original);

    // Instrument names survive via track naming.
    assert_eq!(imported.tracks[0].instrument, "Damphu");
    assert_eq!(imported.tracks[1].instrument, "Bansuri");
}
