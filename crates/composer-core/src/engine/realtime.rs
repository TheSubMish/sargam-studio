//! Real-time playback core.
//!
//! [`EngineCore::process`] is the audio callback body: it is driven by cpal in
//! production (see [`super::player`]) and by plain buffer loops in tests. The
//! control thread talks to it exclusively through lock-free ring buffers —
//! no locks, and (after `compile`) no allocation on the audio path.

use ringbuf::traits::{Consumer, Producer};
use ringbuf::{HeapCons, HeapProd};

use crate::dsp::{EchoDelay, PlateReverb};
use crate::instrument::{Registry, Voice};
use crate::model::{NoteParams, Song};

use super::render::RenderError;

/// Echo send time — fixed; only the wet mix (`Track::delay`) is user-facing.
/// Kept identical to the offline renderer so a track sounds the same live and
/// exported.
const DELAY_TIME_SECS: f32 = 0.28;

/// Pre-allocated voices per track; the softest-spoken oldest voice is stolen
/// beyond this.
pub const VOICES_PER_TRACK: usize = 8;
/// Playhead event cadence in frames (~21 ms at 48 kHz).
const PLAYHEAD_INTERVAL: usize = 1_024;
/// Ring-out margin after the last event before Finished fires.
const TAIL_SECS: f32 = 3.0;

pub enum Command {
    /// Swap in a new compiled song (replaces any current one).
    SetSong(Box<CompiledSong>),
    Play,
    /// Stop advancing; sounding notes release naturally.
    Pause,
    /// Stop, silence everything, rewind to zero.
    Stop,
    /// Jump to a frame position (silences sounding voices).
    Seek(usize),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EngineEvent {
    Playhead { frame: usize, playing: bool },
    Finished,
}

struct CompiledEvent {
    on_frame: usize,
    off_frame: usize,
    params: NoteParams,
}

struct CompiledTrack {
    events: Vec<CompiledEvent>,
    voices: Vec<VoiceSlot>,
    next_event: usize,
    steal_cursor: usize,
    gain_left: f32,
    gain_right: f32,
    /// `None` when the track's send is zero, so silent tracks pay nothing.
    delay_fx: Option<(EchoDelay, f32)>,
    reverb_fx: Option<(PlateReverb, f32)>,
}

impl CompiledTrack {
    /// Apply this track's reverb/delay inserts to one mono sample.
    #[inline]
    fn process_effects(&mut self, mono: f32) -> f32 {
        let mut sample = mono;
        if let Some((echo, wet)) = &mut self.delay_fx {
            sample += echo.tick(sample) * *wet;
        }
        if let Some((reverb, wet)) = &mut self.reverb_fx {
            sample = sample * (1.0 - *wet) + reverb.tick(sample) * *wet;
        }
        sample
    }
}

struct VoiceSlot {
    voice: Box<dyn Voice>,
    off_frame: usize,
}

pub struct CompiledSong {
    tracks: Vec<CompiledTrack>,
    pub total_frames: usize,
    pub sample_rate: f32,
    master_gain: f32,
}

/// Resolve a [`Song`] into frame-accurate events with pre-allocated voice
/// pools, using only built-in instruments. See [`compile_with`] for samplers.
pub fn compile(song: &Song, sample_rate: f32) -> Result<CompiledSong, RenderError> {
    compile_with(song, &Registry::new(), sample_rate)
}

/// Resolve a [`Song`] into frame-accurate events with pre-allocated voice
/// pools. Do this on the control thread; the result is handed to the audio
/// thread via [`Command::SetSong`].
pub fn compile_with(
    song: &Song,
    registry: &Registry,
    sample_rate: f32,
) -> Result<CompiledSong, RenderError> {
    let mut tracks = Vec::new();
    let mut last_frame = 0usize;

    for track in &song.tracks {
        let instr = registry
            .resolve(&track.instrument)
            .ok_or_else(|| RenderError::UnknownInstrument(track.instrument.clone()))?;

        let mut events: Vec<CompiledEvent> = track
            .flatten()
            .into_iter()
            .map(|n| CompiledEvent {
                on_frame: (song.ticks_to_secs(n.start) * sample_rate) as usize,
                off_frame: (song.ticks_to_secs(n.start + n.duration) * sample_rate) as usize,
                params: NoteParams {
                    trigger: n.trigger,
                    velocity: n.velocity,
                    ornament: n.ornament,
                },
            })
            .collect();
        events.sort_by_key(|e| e.on_frame);
        last_frame = last_frame.max(events.last().map_or(0, |e| e.off_frame));

        let voices = (0..VOICES_PER_TRACK)
            .map(|_| VoiceSlot {
                voice: instr.make_voice(sample_rate),
                off_frame: usize::MAX,
            })
            .collect();

        let angle = (track.pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
        tracks.push(CompiledTrack {
            events,
            voices,
            next_event: 0,
            steal_cursor: 0,
            gain_left: angle.cos() * track.gain,
            gain_right: angle.sin() * track.gain,
            delay_fx: (track.delay > 0.0)
                .then(|| (EchoDelay::new(sample_rate, DELAY_TIME_SECS), track.delay.clamp(0.0, 1.0))),
            reverb_fx: (track.reverb > 0.0)
                .then(|| (PlateReverb::new(sample_rate), track.reverb.clamp(0.0, 1.0))),
        });
    }

    Ok(CompiledSong {
        tracks,
        total_frames: last_frame + (TAIL_SECS * sample_rate) as usize,
        sample_rate,
        master_gain: song.master_gain,
    })
}

pub struct EngineCore {
    commands: HeapCons<Command>,
    events: HeapProd<EngineEvent>,
    song: Option<Box<CompiledSong>>,
    position: usize,
    playing: bool,
    frames_since_report: usize,
    finished_sent: bool,
}

impl EngineCore {
    pub fn new(commands: HeapCons<Command>, events: HeapProd<EngineEvent>) -> Self {
        Self {
            commands,
            events,
            song: None,
            position: 0,
            playing: false,
            frames_since_report: 0,
            finished_sent: false,
        }
    }

    fn apply_commands(&mut self) {
        while let Some(cmd) = self.commands.try_pop() {
            match cmd {
                Command::SetSong(song) => {
                    // Dropping the previous song here frees memory on the audio
                    // thread — the one tolerated exception to the no-alloc rule,
                    // paid only at explicit song swaps.
                    self.song = Some(song);
                    self.position = 0;
                    self.playing = false;
                    self.finished_sent = false;
                }
                Command::Play => {
                    // A song that already ran to its end has every track's
                    // event cursor exhausted; resuming "play" from there would
                    // just sit at the end producing silence. Treat Play from
                    // (at-or-past) the end as a replay: rewind and re-arm
                    // cursors exactly like an explicit Seek(0) would.
                    let at_end = self
                        .song
                        .as_ref()
                        .is_some_and(|s| self.position >= s.total_frames);
                    if at_end {
                        self.position = 0;
                        self.finished_sent = false;
                        self.reset_voices_and_cursors();
                    }
                    self.playing = true;
                }
                Command::Pause => {
                    self.playing = false;
                    self.for_all_voices(|s| s.voice.note_off());
                }
                Command::Stop => {
                    self.playing = false;
                    self.position = 0;
                    self.finished_sent = false;
                    self.reset_voices_and_cursors();
                }
                Command::Seek(frame) => {
                    self.position = frame;
                    self.finished_sent = false;
                    self.reset_voices_and_cursors();
                }
            }
        }
    }

    fn for_all_voices(&mut self, mut f: impl FnMut(&mut VoiceSlot)) {
        if let Some(song) = &mut self.song {
            for t in &mut song.tracks {
                for s in &mut t.voices {
                    f(s);
                }
            }
        }
    }

    fn reset_voices_and_cursors(&mut self) {
        let position = self.position;
        if let Some(song) = &mut self.song {
            for t in &mut song.tracks {
                for s in &mut t.voices {
                    s.voice.kill();
                    s.off_frame = usize::MAX;
                }
                t.next_event = t
                    .events
                    .partition_point(|e| e.on_frame < position);
            }
        }
    }

    /// Fill an interleaved stereo buffer. Callable from any thread, but
    /// designed as a cpal callback body: no locks, no allocation.
    pub fn process(&mut self, out: &mut [f32]) {
        self.apply_commands();
        out.fill(0.0);

        let Some(song) = &mut self.song else {
            return;
        };

        let frames = out.len() / 2;
        for frame_idx in 0..frames {
            if self.playing {
                for track in &mut song.tracks {
                    while track.next_event < track.events.len()
                        && track.events[track.next_event].on_frame <= self.position
                    {
                        let ev = &track.events[track.next_event];
                        // Free slot, or steal round-robin.
                        let slot_idx = track
                            .voices
                            .iter()
                            .position(|s| !s.voice.is_active())
                            .unwrap_or_else(|| {
                                track.steal_cursor = (track.steal_cursor + 1) % VOICES_PER_TRACK;
                                track.steal_cursor
                            });
                        let slot = &mut track.voices[slot_idx];
                        slot.voice.note_on(ev.params);
                        slot.off_frame = ev.off_frame;
                        track.next_event += 1;
                    }
                }
            }

            let mut any_active = false;
            for track in &mut song.tracks {
                let mut mono = 0.0;
                for slot in &mut track.voices {
                    if self.playing && slot.off_frame == self.position {
                        slot.voice.note_off();
                        slot.off_frame = usize::MAX;
                    }
                    if slot.voice.is_active() {
                        mono += slot.voice.tick();
                        any_active = true;
                    }
                }
                let mono = track.process_effects(mono);
                out[frame_idx * 2] += mono * track.gain_left;
                out[frame_idx * 2 + 1] += mono * track.gain_right;
            }

            // Live-monitoring fader only (see `Song::master_gain`); clamp so
            // cranking it doesn't turn into harsh digital clipping.
            out[frame_idx * 2] = (out[frame_idx * 2] * song.master_gain).clamp(-1.0, 1.0);
            out[frame_idx * 2 + 1] =
                (out[frame_idx * 2 + 1] * song.master_gain).clamp(-1.0, 1.0);

            if self.playing {
                self.position += 1;
                if self.position >= song.total_frames && !any_active && !self.finished_sent {
                    self.playing = false;
                    self.finished_sent = true;
                    let _ = self.events.try_push(EngineEvent::Finished);
                }
            }
        }

        self.frames_since_report += frames;
        if self.frames_since_report >= PLAYHEAD_INTERVAL {
            self.frames_since_report = 0;
            let _ = self.events.try_push(EngineEvent::Playhead {
                frame: self.position,
                playing: self.playing,
            });
        }
    }
}
