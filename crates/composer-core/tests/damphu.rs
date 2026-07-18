use composer_core::engine::render_song;
use composer_core::instrument::{Damphu, Instrument};
use composer_core::model::{
    Clip, NoteParams, Pattern, PatternNote, Song, Stroke, Track, Trigger, TICKS_PER_BEAT,
};

const SAMPLE_RATE: f32 = 44_100.0;

fn render_stroke(stroke: Stroke, secs: f32) -> Vec<f32> {
    let mut voice = Damphu::default().make_voice(SAMPLE_RATE);
    voice.note_on(NoteParams::stroke(stroke, 0.9));
    (0..(secs * SAMPLE_RATE) as usize)
        .map(|_| voice.tick())
        .collect()
}

fn zero_crossings(buffer: &[f32]) -> usize {
    buffer
        .windows(2)
        .filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0))
        .count()
}

#[test]
fn bass_stroke_is_darker_than_rim() {
    let bass = render_stroke(Stroke::Bass, 0.3);
    let rim = render_stroke(Stroke::Rim, 0.3);
    let (bass_zc, rim_zc) = (zero_crossings(&bass), zero_crossings(&rim));
    assert!(
        rim_zc > bass_zc * 3 / 2,
        "rim ({rim_zc} crossings) should be much brighter than bass ({bass_zc})"
    );
}

#[test]
fn strokes_ring_then_decay_to_silence() {
    let mut voice = Damphu::default().make_voice(SAMPLE_RATE);
    voice.note_on(NoteParams::stroke(Stroke::Bass, 1.0));
    let buffer: Vec<f32> = (0..(2.0 * SAMPLE_RATE) as usize)
        .map(|_| voice.tick())
        .collect();
    let early_peak = buffer[..4_410].iter().fold(0.0_f32, |m, s| m.max(s.abs()));
    assert!(early_peak > 0.01, "stroke is near-silent ({early_peak})");
    assert!(!voice.is_active(), "voice still active 2s after a bass hit");
}

#[test]
fn song_render_places_notes_at_the_right_time() {
    // One damphu hit on beat 2 at 120 BPM → exactly 1.0 second in.
    let mut song = Song::new("Timing", 120.0);
    let mut track = Track::new("Damphu", "Damphu");
    track.patterns.push(Pattern {
        name: "Hit".into(),
        length: 4 * TICKS_PER_BEAT,
        notes: vec![PatternNote::new(
            2 * TICKS_PER_BEAT,
            TICKS_PER_BEAT / 2,
            Trigger::Stroke(Stroke::Bass),
            1.0,
        )],
    });
    track.clips.push(Clip { pattern: 0, start: 0 });
    song.tracks.push(track);

    let stereo = render_song(&song, SAMPLE_RATE).expect("render");
    let onset = SAMPLE_RATE as usize; // 1.0s, in frames

    let before: f32 = stereo[..(onset - 100) * 2]
        .iter()
        .fold(0.0, |m, s| m.max(s.abs()));
    let after: f32 = stereo[onset * 2..(onset + 4_410) * 2]
        .iter()
        .fold(0.0, |m, s| m.max(s.abs()));
    assert!(before < 1e-3, "audio before the first note (peak {before})");
    assert!(after > 0.1, "no audio at the note onset (peak {after})");
}

#[test]
fn unknown_instrument_is_a_clean_error() {
    let mut song = Song::new("Bad", 120.0);
    song.tracks.push(Track::new("X", "Yeti Horn"));
    assert!(render_song(&song, SAMPLE_RATE).is_err());
}
