//! Render a short tungna phrase to `tungna_demo.wav`.
//!
//! Run with: `cargo run --example tungna_demo`

use std::path::Path;

use composer_core::engine::{render_track, write_wav};
use composer_core::instrument::Tungna;
use composer_core::model::NoteEvent;

fn main() {
    const SAMPLE_RATE: f32 = 44_100.0;

    // A simple phrase in A minor pentatonic (A3 C4 D4 E4 G4 A4) — a scale
    // family many Himalayan folk melodies live in.
    let scale = [57.0, 60.0, 62.0, 64.0, 67.0, 69.0];
    let phrase = [0, 1, 2, 4, 2, 1, 0, 2, 4, 5, 4, 2, 0, 1, 0, 0];

    let mut events = Vec::new();
    let step = 0.25; // 16th-note feel at 120 BPM equivalent
    for (i, &degree) in phrase.iter().enumerate() {
        let accent = if i % 4 == 0 { 0.9 } else { 0.6 };
        events.push(NoteEvent::pitch(
            i as f32 * step,
            step * 0.9,
            scale[degree],
            accent,
        ));
    }
    // Closing chord: root + fifth ringing out together.
    let end = phrase.len() as f32 * step;
    events.push(NoteEvent::pitch(end, 2.0, 57.0, 0.95));
    events.push(NoteEvent::pitch(end + 0.03, 2.0, 64.0, 0.85));

    println!("Rendering {} notes...", events.len());
    let audio = render_track(&Tungna::default(), &events, SAMPLE_RATE);

    let path = Path::new("tungna_demo.wav");
    write_wav(path, &audio, 1, SAMPLE_RATE as u32).expect("failed to write wav");
    println!(
        "Wrote {} ({:.1}s of audio)",
        path.display(),
        audio.len() as f32 / SAMPLE_RATE
    );
}
