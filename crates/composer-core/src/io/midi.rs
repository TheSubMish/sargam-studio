//! Standard MIDI file interchange.
//!
//! Export: format-1 SMF — track 0 carries the tempo map, then one MIDI track
//! per song track. Percussion tracks (any stroke-triggered note) go to channel
//! 10 using the GM-ish mapping in [`Stroke::to_gm_key`]; melodic tracks get
//! the other channels. Ornaments and fractional pitches don't survive MIDI —
//! notes round to the nearest semitone (the .song.json format is the full-
//! fidelity one; MIDI is for interchange with DAWs).
//!
//! Import is the mirror: channel-10 notes become Damphu stroke tracks, other
//! channels become melodic tracks (Harmonium unless the MIDI track name matches
//! a catalog instrument).

use std::path::Path;

use midly::num::{u15, u24, u28, u4, u7};
use midly::{Format, Header, MetaMessage, MidiMessage, Smf, Timing, TrackEvent, TrackEventKind};

use crate::instrument;
use crate::model::{
    Clip, Pattern, PatternNote, Song, Stroke, TempoMap, Ticks, Track, Trigger, TICKS_PER_BEAT,
};

#[derive(Debug)]
pub enum MidiError {
    Io(std::io::Error),
    Parse(String),
    Unsupported(String),
}

impl std::fmt::Display for MidiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "midi io error: {e}"),
            Self::Parse(s) => write!(f, "midi parse error: {s}"),
            Self::Unsupported(s) => write!(f, "unsupported midi feature: {s}"),
        }
    }
}

impl std::error::Error for MidiError {}

// ---------------------------------------------------------------- export ----

struct RawEvent {
    tick: Ticks,
    /// Note-offs sort before note-ons at the same tick so retriggers don't
    /// read as overlapping notes.
    is_on: bool,
    key: u8,
    vel: u8,
}

pub fn export_midi(song: &Song, path: &Path) -> Result<(), MidiError> {
    let mut tracks: Vec<Vec<TrackEvent>> = Vec::new();

    // Track 0: title + tempo map.
    let mut tempo_track = Vec::new();
    tempo_track.push(TrackEvent {
        delta: u28::new(0),
        kind: TrackEventKind::Meta(MetaMessage::TrackName(song.title.as_bytes())),
    });
    let mut prev_tick = 0u32;
    for change in song.tempo.changes() {
        let us_per_beat = (60_000_000.0 / change.bpm) as u32;
        tempo_track.push(TrackEvent {
            delta: u28::new(change.tick - prev_tick),
            kind: TrackEventKind::Meta(MetaMessage::Tempo(u24::new(us_per_beat))),
        });
        prev_tick = change.tick;
    }
    tempo_track.push(TrackEvent {
        delta: u28::new(0),
        kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
    });
    tracks.push(tempo_track);

    let mut next_melodic_channel = 0u8;
    for track in &song.tracks {
        let notes = track.flatten();
        let is_percussion = notes
            .iter()
            .any(|n| matches!(n.trigger, Trigger::Stroke(_)));
        let channel = if is_percussion {
            9
        } else {
            let c = if next_melodic_channel == 9 {
                next_melodic_channel += 1;
                next_melodic_channel
            } else {
                next_melodic_channel
            };
            next_melodic_channel += 1;
            c.min(15)
        };

        let mut raw = Vec::with_capacity(notes.len() * 2);
        for n in &notes {
            let key = match n.trigger {
                Trigger::Pitch(midi) => (midi.round() as i32).clamp(0, 127) as u8,
                Trigger::Stroke(s) => s.to_gm_key(),
            };
            let vel = ((n.velocity * 127.0).round() as u32).clamp(1, 127) as u8;
            raw.push(RawEvent {
                tick: n.start,
                is_on: true,
                key,
                vel,
            });
            raw.push(RawEvent {
                tick: n.start + n.duration,
                is_on: false,
                key,
                vel: 0,
            });
        }
        raw.sort_by_key(|e| (e.tick, e.is_on));

        let mut events = Vec::with_capacity(raw.len() + 2);
        events.push(TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Meta(MetaMessage::TrackName(track.name.as_bytes())),
        });
        let mut prev_tick = 0u32;
        for e in &raw {
            let message = if e.is_on {
                MidiMessage::NoteOn {
                    key: u7::new(e.key),
                    vel: u7::new(e.vel),
                }
            } else {
                MidiMessage::NoteOff {
                    key: u7::new(e.key),
                    vel: u7::new(0),
                }
            };
            events.push(TrackEvent {
                delta: u28::new(e.tick - prev_tick),
                kind: TrackEventKind::Midi {
                    channel: u4::new(channel),
                    message,
                },
            });
            prev_tick = e.tick;
        }
        events.push(TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
        });
        tracks.push(events);
    }

    let smf = Smf {
        header: Header {
            format: Format::Parallel,
            timing: Timing::Metrical(u15::new(TICKS_PER_BEAT as u16)),
        },
        tracks,
    };
    smf.save(path).map_err(MidiError::Io)
}

// ---------------------------------------------------------------- import ----

pub fn import_midi(path: &Path) -> Result<Song, MidiError> {
    let bytes = std::fs::read(path).map_err(MidiError::Io)?;
    let smf = Smf::parse(&bytes).map_err(|e| MidiError::Parse(e.to_string()))?;

    let src_tpb = match smf.header.timing {
        Timing::Metrical(t) => t.as_int() as f64,
        Timing::Timecode(..) => {
            return Err(MidiError::Unsupported("SMPTE timecode timing".into()))
        }
    };
    let rescale = |tick: f64| -> Ticks { (tick * TICKS_PER_BEAT as f64 / src_tpb).round() as Ticks };

    let mut song = Song::new("Imported", 120.0);
    let mut tempo = TempoMap::constant(120.0);

    for smf_track in &smf.tracks {
        let mut abs = 0.0_f64;
        let mut name: Option<String> = None;
        // (channel, key) -> (start_tick, velocity)
        let mut open: std::collections::HashMap<(u8, u8), (Ticks, f32)> =
            std::collections::HashMap::new();
        let mut notes: Vec<(u8, PatternNote)> = Vec::new(); // (channel, note)

        for event in smf_track {
            abs += event.delta.as_int() as f64;
            match event.kind {
                TrackEventKind::Meta(MetaMessage::Tempo(us)) => {
                    tempo.set_change(rescale(abs), 60_000_000.0 / us.as_int() as f32);
                }
                TrackEventKind::Meta(MetaMessage::TrackName(bytes)) => {
                    name = Some(String::from_utf8_lossy(bytes).into_owned());
                }
                TrackEventKind::Midi { channel, message } => {
                    let ch = channel.as_int();
                    match message {
                        MidiMessage::NoteOn { key, vel } if vel.as_int() > 0 => {
                            open.insert(
                                (ch, key.as_int()),
                                (rescale(abs), vel.as_int() as f32 / 127.0),
                            );
                        }
                        MidiMessage::NoteOn { key, .. } | MidiMessage::NoteOff { key, .. } => {
                            if let Some((start, velocity)) = open.remove(&(ch, key.as_int())) {
                                let end = rescale(abs);
                                let trigger = if ch == 9 {
                                    Trigger::Stroke(
                                        Stroke::from_gm_key(key.as_int())
                                            .unwrap_or(Stroke::Bass),
                                    )
                                } else {
                                    Trigger::Pitch(key.as_int() as f32)
                                };
                                notes.push((
                                    ch,
                                    PatternNote::new(
                                        start,
                                        end.saturating_sub(start).max(1),
                                        trigger,
                                        velocity,
                                    ),
                                ));
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        if notes.is_empty() {
            // Tempo/title-only track: adopt its name as the song title.
            if let Some(n) = name {
                song.title = n;
            }
            continue;
        }

        let percussion = notes.iter().any(|(ch, _)| *ch == 9);
        let track_name = name.unwrap_or_else(|| format!("Track {}", song.tracks.len() + 1));
        // If the MIDI track is named after a catalog instrument, honor it.
        let instrument_name = if instrument::by_name(&track_name).is_some() {
            track_name.clone()
        } else if percussion {
            "Damphu".to_string()
        } else {
            "Harmonium".to_string()
        };

        let mut pattern_notes: Vec<PatternNote> = notes.into_iter().map(|(_, n)| n).collect();
        pattern_notes.sort_by_key(|n| n.start);
        let length = pattern_notes
            .iter()
            .map(|n| n.start + n.duration)
            .max()
            .unwrap_or(TICKS_PER_BEAT)
            .next_multiple_of(TICKS_PER_BEAT);

        let mut track = Track::new(track_name, instrument_name);
        track.patterns.push(Pattern {
            name: "Imported".into(),
            length,
            notes: pattern_notes,
        });
        track.clips.push(Clip { pattern: 0, start: 0 });
        song.tracks.push(track);
    }

    song.tempo = tempo;
    Ok(song)
}
