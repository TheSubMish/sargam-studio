//! Transport tests drive EngineCore directly with buffers — no audio device.

use composer_core::engine::{compile, Command, EngineCore, EngineEvent};
use composer_core::model::{Clip, Pattern, PatternNote, Song, Stroke, Track, Trigger, TICKS_PER_BEAT};
use ringbuf::traits::{Consumer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};

const SAMPLE_RATE: f32 = 44_100.0;

fn test_song() -> Song {
    // Damphu hits on beats 0 and 2 at 120 BPM (0.0 s and 1.0 s).
    let mut song = Song::new("RT", 120.0);
    let mut track = Track::new("Damphu", "Damphu");
    track.patterns.push(Pattern {
        name: "Hits".into(),
        length: 4 * TICKS_PER_BEAT,
        notes: vec![
            PatternNote::new(0, TICKS_PER_BEAT / 2, Trigger::Stroke(Stroke::Bass), 1.0),
            PatternNote::new(
                2 * TICKS_PER_BEAT,
                TICKS_PER_BEAT / 2,
                Trigger::Stroke(Stroke::Bass),
                1.0,
            ),
        ],
    });
    track.clips.push(Clip { pattern: 0, start: 0 });
    song.tracks.push(track);
    song
}

struct Harness {
    core: EngineCore,
    cmds: HeapProd<Command>,
    evts: HeapCons<EngineEvent>,
}

fn harness() -> Harness {
    let (cmds, cmd_rx) = HeapRb::<Command>::new(64).split();
    let (evt_tx, evts) = HeapRb::<EngineEvent>::new(256).split();
    Harness {
        core: EngineCore::new(cmd_rx, evt_tx),
        cmds,
        evts,
    }
}

fn peak(buf: &[f32]) -> f32 {
    buf.iter().fold(0.0_f32, |m, s| m.max(s.abs()))
}

/// Run `frames` frames through the core in 256-frame chunks; return peak.
fn run(core: &mut EngineCore, frames: usize) -> f32 {
    let mut max = 0.0_f32;
    let mut buf = [0.0_f32; 512];
    let mut remaining = frames;
    while remaining > 0 {
        let n = remaining.min(256);
        let chunk = &mut buf[..n * 2];
        core.process(chunk);
        max = max.max(peak(chunk));
        remaining -= n;
    }
    max
}

#[test]
fn silent_until_play_then_audible() {
    let mut h = harness();
    let song = compile(&test_song(), SAMPLE_RATE).unwrap();
    assert!(h.cmds.try_push(Command::SetSong(Box::new(song))).is_ok());

    let before = run(&mut h.core, 4_096);
    assert!(before < 1e-6, "audio before Play ({before})");

    h.cmds.try_push(Command::Play).ok();
    let after = run(&mut h.core, 4_096);
    assert!(after > 0.01, "no audio after Play ({after})");
}

#[test]
fn stop_silences_and_rewinds() {
    let mut h = harness();
    let song = compile(&test_song(), SAMPLE_RATE).unwrap();
    h.cmds.try_push(Command::SetSong(Box::new(song))).ok();
    h.cmds.try_push(Command::Play).ok();
    run(&mut h.core, 8_192);

    h.cmds.try_push(Command::Stop).ok();
    let stopped = run(&mut h.core, 4_096);
    assert!(stopped < 1e-6, "audio after Stop ({stopped})");

    // Play again from the top: the beat-0 hit must retrigger.
    h.cmds.try_push(Command::Play).ok();
    let restarted = run(&mut h.core, 4_096);
    assert!(restarted > 0.01, "no audio after restart ({restarted})");
}

#[test]
fn seek_skips_ahead_correctly() {
    let mut h = harness();
    let song = compile(&test_song(), SAMPLE_RATE).unwrap();
    h.cmds.try_push(Command::SetSong(Box::new(song))).ok();

    // Seek to 0.95 s: the beat-0 hit is skipped, and the next ~23 ms
    // (1024 frames, still short of the 1.0 s hit) must stay silent.
    h.cmds
        .try_push(Command::Seek((0.95 * SAMPLE_RATE) as usize))
        .ok();
    h.cmds.try_push(Command::Play).ok();
    let head = run(&mut h.core, 1_024);
    assert!(head < 1e-6, "audio before the seeked-to hit ({head})");
    let hit = run(&mut h.core, 8_192);
    assert!(hit > 0.01, "seeked-past hit never sounded ({hit})");
}

#[test]
fn replay_after_natural_finish_produces_sound_again() {
    let mut h = harness();
    let song = compile(&test_song(), SAMPLE_RATE).unwrap();
    let total = song.total_frames;
    h.cmds.try_push(Command::SetSong(Box::new(song))).ok();
    h.cmds.try_push(Command::Play).ok();

    // Let it run all the way to a natural finish (no Stop/Seek involved) —
    // the bug this guards: event cursors are exhausted here, so a naive
    // Command::Play would just sit silently at the end.
    run(&mut h.core, total + 44_100);
    while let Some(_e) = h.evts.try_pop() {} // drain the first Finished

    h.cmds.try_push(Command::Play).ok();
    let replayed = run(&mut h.core, 4_096);
    assert!(replayed > 0.01, "replay after natural finish produced no sound ({replayed})");

    // It must be able to finish — and fire Finished — a second time too.
    run(&mut h.core, total + 44_100);
    let mut saw_second_finish = false;
    while let Some(e) = h.evts.try_pop() {
        if e == EngineEvent::Finished {
            saw_second_finish = true;
        }
    }
    assert!(saw_second_finish, "no Finished event on the second playthrough");
}

#[test]
fn playhead_and_finished_events_arrive() {
    let mut h = harness();
    let song = compile(&test_song(), SAMPLE_RATE).unwrap();
    let total = song.total_frames;
    h.cmds.try_push(Command::SetSong(Box::new(song))).ok();
    h.cmds.try_push(Command::Play).ok();

    // Run past the end of the song plus its tail.
    run(&mut h.core, total + 44_100);

    let mut saw_playhead = false;
    let mut saw_finished = false;
    while let Some(e) = h.evts.try_pop() {
        match e {
            EngineEvent::Playhead { .. } => saw_playhead = true,
            EngineEvent::Finished => saw_finished = true,
        }
    }
    assert!(saw_playhead, "no playhead events");
    assert!(saw_finished, "no Finished event");
}
