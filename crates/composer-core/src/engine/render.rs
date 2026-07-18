use std::path::Path;

use crate::dsp::{EchoDelay, PlateReverb};
use crate::instrument::{Instrument, Registry, Voice};
use crate::model::{NoteEvent, NoteParams, Song, Track};

/// Ring-out time appended after the last note.
const TAIL_SECS: f32 = 3.0;

#[derive(Debug)]
pub enum RenderError {
    UnknownInstrument(String),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownInstrument(name) => write!(f, "unknown instrument: {name}"),
        }
    }
}

impl std::error::Error for RenderError {}

/// A tempo-resolved event in samples — the engine's internal scheduling unit.
struct VoiceEvent {
    on_sample: usize,
    off_sample: usize,
    params: NoteParams,
}

/// Core scheduler: spawn a voice per event, mix all voices, until `total_samples`.
fn render_events(
    instrument: &dyn Instrument,
    mut events: Vec<VoiceEvent>,
    total_samples: usize,
    sample_rate: f32,
) -> Vec<f32> {
    events.sort_by_key(|e| e.on_sample);
    let mut out = vec![0.0_f32; total_samples];
    let mut voices: Vec<(Box<dyn Voice>, usize)> = Vec::new(); // (voice, off_sample)
    let mut next_event = 0;

    for (i, sample_out) in out.iter_mut().enumerate() {
        while next_event < events.len() && events[next_event].on_sample <= i {
            let ev = &events[next_event];
            let mut voice = instrument.make_voice(sample_rate);
            voice.note_on(ev.params);
            voices.push((voice, ev.off_sample));
            next_event += 1;
        }

        let mut mix = 0.0;
        for (voice, off_sample) in voices.iter_mut() {
            if *off_sample == i {
                voice.note_off();
            }
            mix += voice.tick();
        }
        voices.retain(|(v, _)| v.is_active());
        *sample_out = mix;
    }
    out
}

fn events_end_samples(events: &[VoiceEvent], sample_rate: f32) -> usize {
    events
        .iter()
        .map(|e| e.off_sample)
        .max()
        .unwrap_or(0)
        + (TAIL_SECS * sample_rate) as usize
}

/// Echo send time — fixed; only the wet mix (`Track::delay`) is user-facing.
const DELAY_TIME_SECS: f32 = 0.28;

/// Apply a track's reverb/delay inserts to its mono buffer in place, mixing
/// wet with dry by the stored send amount. Skips instantiating an effect
/// entirely when its send is zero (the common case), so plain tracks pay
/// nothing for effects they don't use.
fn apply_track_effects(mono: &mut [f32], track: &Track, sample_rate: f32) {
    if track.delay > 0.0 {
        let mut echo = EchoDelay::new(sample_rate, DELAY_TIME_SECS);
        let wet = track.delay.clamp(0.0, 1.0);
        for s in mono.iter_mut() {
            *s += echo.tick(*s) * wet;
        }
    }
    if track.reverb > 0.0 {
        let mut reverb = PlateReverb::new(sample_rate);
        let wet = track.reverb.clamp(0.0, 1.0);
        for s in mono.iter_mut() {
            *s = *s * (1.0 - wet) + reverb.tick(*s) * wet;
        }
    }
}

/// Resolve one track's notes into scheduling events (in samples) via the
/// song's tempo map.
fn track_events(track: &Track, song: &Song, sample_rate: f32) -> Vec<VoiceEvent> {
    track
        .flatten()
        .into_iter()
        .map(|n| VoiceEvent {
            on_sample: (song.ticks_to_secs(n.start) * sample_rate) as usize,
            off_sample: (song.ticks_to_secs(n.start + n.duration) * sample_rate) as usize,
            params: NoteParams {
                trigger: n.trigger,
                velocity: n.velocity,
                ornament: n.ornament,
            },
        })
        .collect()
}

/// Render one track (its own instrument + effects, panned to stereo) up to
/// `total_samples`, independent of every other track and NOT gain-applied or
/// normalized — [`render_song_with`] applies gain and mixes these together;
/// [`render_stems`] normalizes each one independently for standalone files.
fn render_track_panned(
    track: &Track,
    registry: &Registry,
    total_samples: usize,
    sample_rate: f32,
    events: Vec<VoiceEvent>,
) -> Result<Vec<f32>, RenderError> {
    let instrument = registry
        .resolve(&track.instrument)
        .ok_or_else(|| RenderError::UnknownInstrument(track.instrument.clone()))?;
    let mut mono = render_events(instrument.as_ref(), events, total_samples, sample_rate);
    apply_track_effects(&mut mono, track, sample_rate);

    let angle = (track.pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
    let (left, right) = (angle.cos(), angle.sin());
    let mut stereo = vec![0.0_f32; total_samples * 2];
    for (i, s) in mono.iter().enumerate() {
        stereo[i * 2] = s * left;
        stereo[i * 2 + 1] = s * right;
    }
    Ok(stereo)
}

/// Render each track independently to its own normalized stereo buffer —
/// for "export stems" (per-instrument files instead of one mixed master).
/// Gain is applied (relative balance matters even per-stem) but each stem is
/// then peak-normalized on its own, same as every other export.
pub fn render_stems(
    song: &Song,
    registry: &Registry,
    sample_rate: f32,
) -> Result<Vec<(String, Vec<f32>)>, RenderError> {
    let per_track_events: Vec<Vec<VoiceEvent>> = song
        .tracks
        .iter()
        .map(|t| track_events(t, song, sample_rate))
        .collect();
    let total = per_track_events
        .iter()
        .map(|e| events_end_samples(e, sample_rate))
        .max()
        .unwrap_or(0);

    song.tracks
        .iter()
        .zip(per_track_events)
        .map(|(track, events)| {
            let mut stereo = render_track_panned(track, registry, total, sample_rate, events)?;
            stereo.iter_mut().for_each(|s| *s *= track.gain);
            normalize(&mut stereo);
            Ok((track.name.clone(), stereo))
        })
        .collect()
}

/// Render one instrument's note events to a normalized mono buffer.
/// Handy for auditioning a single instrument or phrase.
pub fn render_track(
    instrument: &dyn Instrument,
    events: &[NoteEvent],
    sample_rate: f32,
) -> Vec<f32> {
    let events: Vec<VoiceEvent> = events
        .iter()
        .map(|e| VoiceEvent {
            on_sample: (e.start_secs * sample_rate) as usize,
            off_sample: ((e.start_secs + e.duration_secs) * sample_rate) as usize,
            params: e.params(),
        })
        .collect();
    let total = events_end_samples(&events, sample_rate);
    let mut out = render_events(instrument, events, total, sample_rate);
    normalize(&mut out);
    out
}

/// Render a full song to normalized interleaved stereo using only built-in
/// instruments. See [`render_song_with`] to include registered samplers.
pub fn render_song(song: &Song, sample_rate: f32) -> Result<Vec<f32>, RenderError> {
    render_song_with(song, &Registry::new(), sample_rate)
}

/// Render a full song to normalized interleaved stereo.
///
/// Each track renders mono through its instrument (resolved through
/// `registry`, falling back to the built-in catalog), then its gain and
/// constant-power pan place it in the stereo master.
pub fn render_song_with(
    song: &Song,
    registry: &Registry,
    sample_rate: f32,
) -> Result<Vec<f32>, RenderError> {
    let per_track_events: Vec<Vec<VoiceEvent>> = song
        .tracks
        .iter()
        .map(|t| track_events(t, song, sample_rate))
        .collect();
    let total = per_track_events
        .iter()
        .map(|e| events_end_samples(e, sample_rate))
        .max()
        .unwrap_or(0);

    let mut master = vec![0.0_f32; total * 2];
    for (track, events) in song.tracks.iter().zip(per_track_events) {
        let stereo = render_track_panned(track, registry, total, sample_rate, events)?;
        for i in 0..total {
            master[i * 2] += stereo[i * 2] * track.gain;
            master[i * 2 + 1] += stereo[i * 2 + 1] * track.gain;
        }
    }

    normalize(&mut master);
    Ok(master)
}

/// Scale so the loudest peak sits at -1 dBFS; leaves silence untouched.
fn normalize(buffer: &mut [f32]) {
    let peak = buffer.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
    if peak > 1e-6 {
        let gain = 0.891 / peak; // -1 dBFS
        buffer.iter_mut().for_each(|s| *s *= gain);
    }
}

/// Write an interleaved f32 buffer as a 16-bit WAV file.
pub fn write_wav(
    path: &Path,
    buffer: &[f32],
    channels: u16,
    sample_rate: u32,
) -> Result<(), hound::Error> {
    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for &s in buffer {
        writer.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
    }
    writer.finalize()
}
