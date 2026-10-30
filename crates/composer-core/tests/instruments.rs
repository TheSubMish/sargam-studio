//! Phase 1b instrument checks: madal, harmonium, bansuri, ornaments.

use composer_core::instrument::{by_name, Bansuri, Harmonium, Instrument, Madal, Sarangi};
use composer_core::model::{NoteParams, Ornament, Stroke};

const SAMPLE_RATE: f32 = 44_100.0;

fn zero_crossings(buffer: &[f32]) -> usize {
    buffer
        .windows(2)
        .filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0))
        .count()
}

fn estimate_pitch(buffer: &[f32], min_hz: f32, max_hz: f32) -> f32 {
    let corr_at =
        |lag: usize| -> f32 { buffer.iter().zip(&buffer[lag..]).map(|(a, b)| a * b).sum() };
    let min_lag = (SAMPLE_RATE / max_hz) as usize;
    let max_lag = (SAMPLE_RATE / min_hz) as usize;
    let (mut best_lag, mut best_corr) = (min_lag, f32::MIN);
    for lag in min_lag..=max_lag {
        let corr = corr_at(lag);
        if corr > best_corr {
            best_corr = corr;
            best_lag = lag;
        }
    }
    let (y0, y1, y2) = (
        corr_at(best_lag - 1),
        corr_at(best_lag),
        corr_at(best_lag + 1),
    );
    let delta = 0.5 * (y0 - y2) / (y0 - 2.0 * y1 + y2);
    SAMPLE_RATE / (best_lag as f32 + delta)
}

#[test]
fn all_new_instruments_are_in_catalog() {
    for name in ["Madal", "Harmonium", "Bansuri", "Sarangi"] {
        assert!(by_name(name).is_some(), "{name} missing from catalog");
    }
}

#[test]
fn madal_heads_have_distinct_registers() {
    let render = |stroke| {
        let mut voice = Madal.make_voice(SAMPLE_RATE);
        voice.note_on(NoteParams::stroke(stroke, 0.9));
        let buffer: Vec<f32> = (0..(0.3 * SAMPLE_RATE) as usize)
            .map(|_| voice.tick())
            .collect();
        buffer
    };
    let bass_zc = zero_crossings(&render(Stroke::Bass));
    let open_zc = zero_crossings(&render(Stroke::Open));
    assert!(
        open_zc > bass_zc * 3 / 2,
        "high head ({open_zc}) should sit well above low head ({bass_zc})"
    );
}

#[test]
fn harmonium_sustains_until_release_then_stops() {
    let mut voice = Harmonium::default().make_voice(SAMPLE_RATE);
    voice.note_on(NoteParams::pitch(60.0, 0.8));

    // Held for 2 seconds: must still be sounding (this is the first
    // sustaining instrument — decay-style envelopes would have died).
    let held: Vec<f32> = (0..(2.0 * SAMPLE_RATE) as usize)
        .map(|_| voice.tick())
        .collect();
    let late_peak = held[held.len() - 4_410..]
        .iter()
        .fold(0.0_f32, |m, s| m.max(s.abs()));
    assert!(late_peak > 0.05, "harmonium died while held ({late_peak})");

    voice.note_off();
    for _ in 0..(1.0 * SAMPLE_RATE) as usize {
        voice.tick();
    }
    assert!(!voice.is_active(), "still active 1s after release");
}

#[test]
fn harmonium_plays_in_tune() {
    let mut voice = Harmonium::default().make_voice(SAMPLE_RATE);
    voice.note_on(NoteParams::pitch(69.0, 0.8)); // A4 = 440 Hz
    let buffer: Vec<f32> = (0..SAMPLE_RATE as usize).map(|_| voice.tick()).collect();
    let estimated = estimate_pitch(&buffer[8_820..], 100.0, 1_000.0);
    let cents = 1_200.0 * (estimated / 440.0).log2();
    assert!(
        cents.abs() < 15.0,
        "expected ~440 Hz, estimated {estimated:.1} Hz ({cents:+.1} cents)"
    );
}

#[test]
fn bansuri_meend_slides_into_the_note() {
    // Meend from 2 semitones below, over 0.25 s, landing on A4.
    let note = NoteParams::pitch(69.0, 0.8).with_ornament(Ornament::Meend {
        from_semitones: -2.0,
        secs: 0.25,
    });
    let mut voice = Bansuri.make_voice(SAMPLE_RATE);
    voice.note_on(note);
    let buffer: Vec<f32> = (0..(1.2 * SAMPLE_RATE) as usize)
        .map(|_| voice.tick())
        .collect();

    // Early window (during the slide) must sit clearly below the target;
    // late window must have arrived at it.
    let early = estimate_pitch(&buffer[2_205..11_025], 200.0, 800.0); // 0.05–0.25 s
    let late = estimate_pitch(&buffer[26_460..48_000], 200.0, 800.0); // 0.6 s onward
    assert!(
        early < 430.0,
        "during meend pitch should be flat of target, got {early:.1} Hz"
    );
    let cents = 1_200.0 * (late / 440.0).log2();
    assert!(
        cents.abs() < 25.0,
        "after meend should settle at 440 Hz, got {late:.1} Hz ({cents:+.1} cents)"
    );
}

#[test]
fn kill_silences_immediately() {
    let mut voice = Harmonium::default().make_voice(SAMPLE_RATE);
    voice.note_on(NoteParams::pitch(60.0, 0.9));
    for _ in 0..4_410 {
        voice.tick();
    }
    voice.kill();
    assert!(!voice.is_active());
    let residue: f32 = (0..100).map(|_| voice.tick().abs()).fold(0.0, f32::max);
    assert!(residue < 1e-3, "audio after kill ({residue})");
}

fn render_sarangi(note: NoteParams, secs: f32) -> Vec<f32> {
    let mut voice = Sarangi.make_voice(SAMPLE_RATE);
    voice.note_on(note);
    (0..(secs * SAMPLE_RATE) as usize)
        .map(|_| voice.tick())
        .collect()
}

#[test]
fn sarangi_is_audible_bounded_and_in_tune() {
    // G4 = 392 Hz; vibrato wobbles it by design, so allow 40 cents.
    let buffer = render_sarangi(NoteParams::pitch(67.0, 0.8), 1.0);
    let peak = buffer.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(peak > 0.05, "sarangi is near-silent (peak {peak})");
    assert!(peak < 1.5, "sarangi is too hot (peak {peak})");
    assert!(buffer.iter().all(|s| s.is_finite()));

    let f = estimate_pitch(&buffer[22_050..38_000], 300.0, 500.0);
    let cents = 1_200.0 * (f / 392.0).log2();
    assert!(
        cents.abs() < 40.0,
        "sarangi pitch {f:.1} Hz ({cents:+.1} cents)"
    );
}

#[test]
fn sarangi_bows_harder_means_brighter() {
    // Share of energy in the first difference = high-frequency content,
    // independent of overall loudness.
    let brightness = |buf: &[f32]| {
        let diff: f32 = buf.windows(2).map(|w| (w[1] - w[0]).powi(2)).sum();
        let total: f32 = buf.iter().map(|s| s * s).sum();
        diff / total
    };
    let soft = brightness(&render_sarangi(NoteParams::pitch(60.0, 0.2), 0.6));
    let hard = brightness(&render_sarangi(NoteParams::pitch(60.0, 1.0), 0.6));
    assert!(
        hard > soft * 1.1,
        "hard bow ({hard}) should be brighter than soft ({soft})"
    );
}

#[test]
fn sarangi_meend_slides_and_voice_decays() {
    let note = NoteParams::pitch(69.0, 0.8).with_ornament(Ornament::Meend {
        from_semitones: -3.0,
        secs: 0.3,
    });
    let buffer = render_sarangi(note, 1.4);
    let early = estimate_pitch(&buffer[2_205..11_025], 200.0, 800.0);
    let late = estimate_pitch(&buffer[30_000..52_000], 200.0, 800.0);
    assert!(early < 420.0, "should start flat of 440 Hz, got {early:.1}");
    assert!(
        (1_200.0 * (late / 440.0).log2()).abs() < 40.0,
        "should land on 440 Hz, got {late:.1}"
    );

    let mut voice = Sarangi.make_voice(SAMPLE_RATE);
    voice.note_on(NoteParams::pitch(60.0, 0.9));
    for _ in 0..4_410 {
        voice.tick();
    }
    voice.note_off();
    for _ in 0..(1.0 * SAMPLE_RATE) as usize {
        voice.tick();
    }
    assert!(
        !voice.is_active(),
        "sarangi voice should release to silence"
    );
}
