//! SFZ sampler tests using generated WAV fixtures — verifies the whole
//! sample-pack pipeline without any real recordings.

use std::f32::consts::TAU;
use std::path::PathBuf;
use std::sync::Arc;

use composer_core::engine::render_song_with;
use composer_core::instrument::{Instrument, Registry, SamplerInstrument};
use composer_core::model::{
    Clip, NoteParams, Pattern, PatternNote, Song, Track, Trigger, TICKS_PER_BEAT,
};

const SR: f32 = 44_100.0;

/// Per-test fixture dir — tests run in parallel, so they must not share files.
fn scratch_dir(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("composer-sfz-test-{}-{test}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_sine_wav(path: &PathBuf, freq: f32, secs: f32) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SR as u32,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for i in 0..(secs * SR) as usize {
        let s = (TAU * freq * i as f32 / SR).sin() * 0.8;
        w.write_sample((s * i16::MAX as f32) as i16).unwrap();
    }
    w.finalize().unwrap();
}

/// Two-zone sampler: 440 Hz sine rooted at A4 (keys ..=71), 880 Hz sine rooted
/// at A5 (keys 72..). Both loop their middle so notes can sustain.
fn fixture_sfz(test: &str) -> PathBuf {
    let dir = scratch_dir(test);
    write_sine_wav(&dir.join("a4.wav"), 440.0, 0.5);
    write_sine_wav(&dir.join("a5.wav"), 880.0, 0.5);
    let sfz = dir.join("test_tone.sfz");
    std::fs::write(
        &sfz,
        "// generated test instrument\n\
         <global> loop_mode=loop_continuous loop_start=2000 loop_end=20000 ampeg_release=0.05\n\
         <region> sample=a4.wav lokey=0 hikey=71 pitch_keycenter=69\n\
         <region> sample=a5.wav lokey=72 hikey=127 pitch_keycenter=81\n",
    )
    .unwrap();
    sfz
}

fn estimate_pitch(buffer: &[f32], min_hz: f32, max_hz: f32) -> f32 {
    let corr_at =
        |lag: usize| -> f32 { buffer.iter().zip(&buffer[lag..]).map(|(a, b)| a * b).sum() };
    let min_lag = (SR / max_hz) as usize;
    let max_lag = (SR / min_hz) as usize;
    let (mut best_lag, mut best_corr) = (min_lag, f32::MIN);
    for lag in min_lag..=max_lag {
        let c = corr_at(lag);
        if c > best_corr {
            best_corr = c;
            best_lag = lag;
        }
    }
    let (y0, y1, y2) = (
        corr_at(best_lag - 1),
        corr_at(best_lag),
        corr_at(best_lag + 1),
    );
    let delta = 0.5 * (y0 - y2) / (y0 - 2.0 * y1 + y2);
    SR / (best_lag as f32 + delta)
}

fn render_note(
    sampler: &SamplerInstrument,
    midi: f32,
    hold_secs: f32,
    total_secs: f32,
) -> Vec<f32> {
    let mut voice = sampler.make_voice(SR);
    voice.note_on(NoteParams::pitch(midi, 0.9));
    let hold = (hold_secs * SR) as usize;
    (0..(total_secs * SR) as usize)
        .map(|i| {
            if i == hold {
                voice.note_off();
            }
            voice.tick()
        })
        .collect()
}

#[test]
fn plays_root_key_at_recorded_pitch() {
    let sampler = SamplerInstrument::from_sfz(&fixture_sfz("root")).unwrap();
    assert_eq!(sampler.name(), "test_tone");
    let buf = render_note(&sampler, 69.0, 0.8, 1.0);
    let hz = estimate_pitch(&buf[2_205..30_000], 200.0, 1_000.0);
    assert!(
        (hz - 440.0).abs() < 4.0,
        "root key should play 440 Hz, got {hz:.1}"
    );
}

#[test]
fn pitch_shifts_within_zone_and_selects_zones() {
    let sampler = SamplerInstrument::from_sfz(&fixture_sfz("zones")).unwrap();

    // D5 (74) falls in the high zone (root 81 = 880 Hz) → 880 * 2^(-7/12) ≈ 587.3 Hz.
    let buf = render_note(&sampler, 74.0, 0.8, 1.0);
    let hz = estimate_pitch(&buf[2_205..30_000], 300.0, 1_200.0);
    assert!(
        (hz - 587.3).abs() < 6.0,
        "D5 via high zone should be ~587 Hz, got {hz:.1}"
    );

    // C4 (60) falls in the low zone (root 69 = 440 Hz) → 440 * 2^(-9/12) ≈ 261.6 Hz.
    let buf = render_note(&sampler, 60.0, 0.8, 1.0);
    let hz = estimate_pitch(&buf[2_205..30_000], 150.0, 600.0);
    assert!(
        (hz - 261.6).abs() < 4.0,
        "C4 via low zone should be ~262 Hz, got {hz:.1}"
    );
}

#[test]
fn loop_sustains_past_sample_end_until_release() {
    let sampler = SamplerInstrument::from_sfz(&fixture_sfz("loop")).unwrap();
    // Source is 0.5 s; hold for 1.5 s — only the loop can carry it that far.
    let buf = render_note(&sampler, 69.0, 1.5, 2.0);
    let held = buf[(1.3 * SR) as usize..(1.45 * SR) as usize]
        .iter()
        .fold(0.0_f32, |m, s| m.max(s.abs()));
    assert!(held > 0.1, "loop did not sustain ({held})");
    let after = buf[(1.9 * SR) as usize..]
        .iter()
        .fold(0.0_f32, |m, s| m.max(s.abs()));
    assert!(after < 0.01, "note did not release ({after})");
}

#[test]
fn registered_sampler_renders_in_a_song() {
    let sampler = SamplerInstrument::from_sfz(&fixture_sfz("registry")).unwrap();
    let mut registry = Registry::new();
    registry.register(Arc::new(sampler));

    let mut song = Song::new("Sampled", 120.0);
    let mut track = Track::new("Lead", "test_tone");
    track.patterns.push(Pattern {
        name: "P".into(),
        length: 2 * TICKS_PER_BEAT,
        notes: vec![PatternNote::new(
            0,
            TICKS_PER_BEAT,
            Trigger::Pitch(69.0),
            0.9,
        )],
    });
    track.clips.push(Clip {
        pattern: 0,
        start: 0,
    });
    song.tracks.push(track);

    let stereo = render_song_with(&song, &registry, SR).expect("render with sampler");
    let peak = stereo.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
    assert!(peak > 0.1, "sampler track is silent");
}
