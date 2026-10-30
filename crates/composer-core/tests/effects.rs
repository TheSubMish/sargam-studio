//! Per-track reverb/delay inserts and the live-monitoring master fader.

use composer_core::engine::{compile, render_song, render_stems, EngineCore, EngineEvent};
use composer_core::engine::{Command, RenderError};
use composer_core::model::{
    Clip, Pattern, PatternNote, Song, Stroke, Track, Trigger, TICKS_PER_BEAT,
};
use ringbuf::traits::{Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};

const SR: f32 = 44_100.0;

fn one_hit_song(reverb: f32, delay: f32) -> Song {
    let mut song = Song::new("FX", 120.0);
    let mut track = Track::new("Damphu", "Damphu");
    track.reverb = reverb;
    track.delay = delay;
    track.patterns.push(Pattern {
        name: "Hit".into(),
        length: 2 * TICKS_PER_BEAT,
        notes: vec![PatternNote::new(
            0,
            TICKS_PER_BEAT / 4,
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

fn peak(buf: &[f32]) -> f32 {
    buf.iter().fold(0.0_f32, |m, s| m.max(s.abs()))
}

#[test]
fn dry_and_wet_renders_differ_in_tail() {
    let dry = render_song(&one_hit_song(0.0, 0.0), SR).unwrap();
    let wet = render_song(&one_hit_song(0.8, 0.6), SR).unwrap();

    // A window well after the short stroke has decayed on its own (damphu
    // rings for well under half a second): dry should be silent there, wet
    // should still show reverb/echo tail energy.
    let start = (0.6 * SR) as usize * 2;
    let end = (0.9 * SR) as usize * 2;
    let dry_tail = peak(&dry[start..end.min(dry.len())]);
    let wet_tail = peak(&wet[start..end.min(wet.len())]);
    assert!(
        dry_tail < 0.01,
        "dry render should be silent by 0.6s ({dry_tail})"
    );
    assert!(
        wet_tail > dry_tail,
        "wet render should have audible tail ({wet_tail} vs {dry_tail})"
    );
}

#[test]
fn zero_send_costs_nothing_and_matches_plain_render() {
    // With reverb/delay at 0, the effect path is skipped entirely — output
    // should be identical to a track with no effect fields touched at all.
    let a = render_song(&one_hit_song(0.0, 0.0), SR).unwrap();
    let mut plain = Song::new("Plain", 120.0);
    let mut track = Track::new("Damphu", "Damphu");
    track.patterns.push(Pattern {
        name: "Hit".into(),
        length: 2 * TICKS_PER_BEAT,
        notes: vec![PatternNote::new(
            0,
            TICKS_PER_BEAT / 4,
            Trigger::Stroke(Stroke::Bass),
            1.0,
        )],
    });
    track.clips.push(Clip {
        pattern: 0,
        start: 0,
    });
    plain.tracks.push(track);
    let b = render_song(&plain, SR).unwrap();
    assert_eq!(a, b);
}

#[test]
fn unknown_instrument_stems_is_a_clean_error() {
    let mut song = Song::new("Bad", 120.0);
    song.tracks.push(Track::new("X", "Yeti Horn"));
    let result: Result<_, RenderError> =
        render_stems(&song, &composer_core::instrument::Registry::new(), SR);
    assert!(result.is_err());
}

#[test]
fn stems_render_one_file_per_track_independently() {
    let mut song = Song::new("Stems", 120.0);
    for name in ["Damphu", "Tungna"] {
        let mut track = Track::new(name, name);
        track.patterns.push(Pattern {
            name: "P".into(),
            length: TICKS_PER_BEAT,
            notes: vec![PatternNote::new(
                0,
                TICKS_PER_BEAT / 4,
                if name == "Damphu" {
                    Trigger::Stroke(Stroke::Bass)
                } else {
                    Trigger::Pitch(57.0)
                },
                0.9,
            )],
        });
        track.clips.push(Clip {
            pattern: 0,
            start: 0,
        });
        song.tracks.push(track);
    }

    let stems = render_stems(&song, &composer_core::instrument::Registry::new(), SR).unwrap();
    assert_eq!(stems.len(), 2);
    assert_eq!(stems[0].0, "Damphu");
    assert_eq!(stems[1].0, "Tungna");
    for (_, buf) in &stems {
        assert!(peak(buf) > 0.1, "stem should not be silent");
    }
}

struct Harness {
    core: EngineCore,
    cmds: HeapProd<Command>,
}

fn harness() -> Harness {
    let (cmds, cmd_rx) = HeapRb::<Command>::new(64).split();
    let (evt_tx, _evt_rx): (_, HeapCons<EngineEvent>) = HeapRb::new(256).split();
    Harness {
        core: EngineCore::new(cmd_rx, evt_tx),
        cmds,
    }
}

fn run(core: &mut EngineCore, frames: usize) -> f32 {
    let mut max = 0.0_f32;
    let mut buf = [0.0_f32; 512];
    let mut remaining = frames;
    while remaining > 0 {
        let n = remaining.min(256);
        let chunk = &mut buf[..n * 2];
        core.process(chunk);
        max = max.max(chunk.iter().fold(0.0_f32, |m, s| m.max(s.abs())));
        remaining -= n;
    }
    max
}

#[test]
fn master_gain_scales_realtime_output_and_clamps() {
    let mut song = one_hit_song(0.0, 0.0);
    song.master_gain = 0.1;
    let mut h = harness();
    let compiled = compile(&song, SR).unwrap();
    h.cmds.try_push(Command::SetSong(Box::new(compiled))).ok();
    h.cmds.try_push(Command::Play).ok();
    let quiet_peak = run(&mut h.core, 4_096);

    let mut song_loud = one_hit_song(0.0, 0.0);
    song_loud.master_gain = 0.1;
    // Sanity: same low master_gain twice should give the same peak (determinism).
    let mut h2 = harness();
    let compiled2 = compile(&song_loud, SR).unwrap();
    h2.cmds.try_push(Command::SetSong(Box::new(compiled2))).ok();
    h2.cmds.try_push(Command::Play).ok();
    let quiet_peak_2 = run(&mut h2.core, 4_096);
    assert!((quiet_peak - quiet_peak_2).abs() < 1e-6);

    let mut song_hot = one_hit_song(0.0, 0.0);
    song_hot.master_gain = 50.0; // deliberately absurd — must clamp, not distort wildly
    let mut h3 = harness();
    let compiled3 = compile(&song_hot, SR).unwrap();
    h3.cmds.try_push(Command::SetSong(Box::new(compiled3))).ok();
    h3.cmds.try_push(Command::Play).ok();
    let hot_peak = run(&mut h3.core, 4_096);
    assert!(
        hot_peak <= 1.0,
        "master gain must clamp to [-1,1] ({hot_peak})"
    );
    assert!(hot_peak > quiet_peak, "50x should be louder than 0.1x");
}
