//! Song::duration_secs and multi-bit-depth WAV export — the two engine
//! additions the Projects/Settings pages need.

use composer_core::engine::{render_song, write_wav_with_depth, BitDepth};
use composer_core::model::{Clip, Pattern, PatternNote, Song, Track, Trigger, TICKS_PER_BEAT};

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("composer-core-projects-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

#[test]
fn duration_is_zero_for_an_empty_song() {
    let song = Song::new("Empty", 120.0);
    assert_eq!(song.duration_secs(), 0.0);
}

#[test]
fn duration_matches_the_last_clip_end() {
    // 120 BPM, one 4-beat pattern placed once at tick 0: exactly 2.0 s.
    let mut song = Song::new("Timed", 120.0);
    let mut track = Track::new("Damphu", "Damphu");
    track.patterns.push(Pattern {
        name: "P".into(),
        length: 4 * TICKS_PER_BEAT,
        notes: vec![],
    });
    track.clips.push(Clip { pattern: 0, start: 0 });
    song.tracks.push(track);
    assert!((song.duration_secs() - 2.0).abs() < 1e-4);
}

#[test]
fn duration_uses_the_furthest_clip_across_all_tracks() {
    let mut song = Song::new("Multi", 120.0);
    let mut short = Track::new("A", "Damphu");
    short.patterns.push(Pattern {
        name: "P".into(),
        length: TICKS_PER_BEAT,
        notes: vec![],
    });
    short.clips.push(Clip { pattern: 0, start: 0 });
    song.tracks.push(short);

    let mut long = Track::new("B", "Tungna");
    long.patterns.push(Pattern {
        name: "P".into(),
        length: 2 * TICKS_PER_BEAT,
        notes: vec![],
    });
    long.clips.push(Clip {
        pattern: 0,
        start: 10 * TICKS_PER_BEAT,
    });
    song.tracks.push(long);

    // Furthest clip ends at tick 12 beats -> 6.0s at 120 BPM.
    assert!((song.duration_secs() - 6.0).abs() < 1e-4);
}

fn one_hit_song() -> Song {
    let mut song = Song::new("Depth", 120.0);
    let mut track = Track::new("Tungna", "Tungna");
    track.patterns.push(Pattern {
        name: "P".into(),
        length: TICKS_PER_BEAT,
        notes: vec![PatternNote::new(0, TICKS_PER_BEAT / 2, Trigger::Pitch(57.0), 0.9)],
    });
    track.clips.push(Clip { pattern: 0, start: 0 });
    song.tracks.push(track);
    song
}

#[test]
fn every_bit_depth_writes_a_readable_wav_with_matching_content() {
    let song = one_hit_song();
    let audio = render_song(&song, 44_100.0).unwrap();

    for (depth, expected_bits) in [
        (BitDepth::Sixteen, 16),
        (BitDepth::TwentyFour, 24),
        (BitDepth::ThirtyTwo, 32),
    ] {
        let path = scratch(&format!("depth_{expected_bits}.wav"));
        write_wav_with_depth(&path, &audio, 2, 44_100, depth).unwrap();

        let reader = hound::WavReader::open(&path).unwrap();
        let spec = reader.spec();
        assert_eq!(spec.bits_per_sample, expected_bits, "depth {expected_bits}");
        assert_eq!(spec.channels, 2);
        assert_eq!(spec.sample_rate, 44_100);

        // Round-trip peak should be close to the source peak regardless of depth.
        let src_peak = audio.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        let mut reader = hound::WavReader::open(&path).unwrap();
        let read_peak = match spec.sample_format {
            hound::SampleFormat::Int => reader
                .samples::<i32>()
                .map(|s| (s.unwrap() as f32 / (1i64 << (spec.bits_per_sample - 1)) as f32).abs())
                .fold(0.0_f32, f32::max),
            hound::SampleFormat::Float => reader
                .samples::<f32>()
                .map(|s| s.unwrap().abs())
                .fold(0.0_f32, f32::max),
        };
        assert!(
            (src_peak - read_peak).abs() < 0.01,
            "depth {expected_bits}: src {src_peak} vs read {read_peak}"
        );
    }
}
