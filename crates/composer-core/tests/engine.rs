use composer_core::dsp::KarplusStrong;
use composer_core::engine::render_track;
use composer_core::instrument::Tungna;
use composer_core::model::{midi_to_hz, NoteEvent, NoteParams};

#[test]
fn midi_to_hz_reference_points() {
    assert!((midi_to_hz(69.0) - 440.0).abs() < 1e-3); // A4
    assert!((midi_to_hz(57.0) - 220.0).abs() < 1e-3); // A3
    assert!((midi_to_hz(60.0) - 261.626).abs() < 1e-2); // C4
}

/// Estimate the fundamental of a buffer by autocorrelation peak.
fn estimate_pitch(buffer: &[f32], sample_rate: f32, min_hz: f32, max_hz: f32) -> f32 {
    let corr_at = |lag: usize| -> f32 {
        buffer
            .iter()
            .zip(&buffer[lag..])
            .map(|(a, b)| a * b)
            .sum()
    };
    let min_lag = (sample_rate / max_hz) as usize;
    let max_lag = (sample_rate / min_hz) as usize;
    let (mut best_lag, mut best_corr) = (min_lag, f32::MIN);
    for lag in min_lag..=max_lag {
        let corr = corr_at(lag);
        if corr > best_corr {
            best_corr = corr;
            best_lag = lag;
        }
    }
    // Parabolic interpolation around the peak for sub-sample lag precision.
    let (y0, y1, y2) = (
        corr_at(best_lag - 1),
        corr_at(best_lag),
        corr_at(best_lag + 1),
    );
    let delta = 0.5 * (y0 - y2) / (y0 - 2.0 * y1 + y2);
    sample_rate / (best_lag as f32 + delta)
}

#[test]
fn karplus_strong_is_in_tune() {
    let sample_rate = 44_100.0;
    for target_hz in [110.0_f32, 220.0, 440.0] {
        let mut ks = KarplusStrong::new(sample_rate, 60.0);
        ks.pluck(target_hz, 0.8, 4_000.0, 0.996);
        // Skip the noisy attack, analyze the settled tone.
        let buffer: Vec<f32> = (0..(sample_rate as usize)).map(|_| ks.tick()).collect();
        let settled = &buffer[4_410..];
        let estimated = estimate_pitch(settled, sample_rate, 50.0, 1_000.0);
        let cents = 1_200.0 * (estimated / target_hz).log2();
        assert!(
            cents.abs() < 10.0,
            "expected {target_hz} Hz, estimated {estimated:.1} Hz ({cents:+.1} cents)"
        );
    }
}

#[test]
fn render_produces_bounded_non_silent_audio() {
    let events = [NoteEvent::pitch(0.0, 0.5, 57.0, 0.8)];
    let audio = render_track(&Tungna::default(), &events, 44_100.0);
    let peak = audio.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
    assert!(peak > 0.1, "render is near-silent (peak {peak})");
    assert!(peak <= 1.0, "render clips (peak {peak})");
}

#[test]
fn voices_decay_to_silence() {
    let sample_rate = 44_100.0;
    let mut voice = composer_core::instrument::Instrument::make_voice(
        &Tungna::default(),
        sample_rate,
    );
    voice.note_on(NoteParams::pitch(57.0, 0.8));
    voice.note_off();
    for _ in 0..(sample_rate as usize * 2) {
        voice.tick();
    }
    assert!(!voice.is_active(), "voice still active 2s after note_off");
}
