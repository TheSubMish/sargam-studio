//! Play a saved song through the default audio output.
//!
//! Run with: `cargo run --example play_song -- jhyaure_demo.song.json`
//! (generate the file first with `cargo run --example jhyaure_demo`)

use std::path::Path;
use std::time::Duration;

use composer_core::engine::{compile, Command, EngineEvent, Player};
use composer_core::model::Song;

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "jhyaure_demo.song.json".into());
    let song = Song::load(Path::new(&path)).expect("failed to load song file");
    println!("Loaded \"{}\" ({} tracks)", song.title, song.tracks.len());

    let mut player = match Player::new() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Cannot open audio output: {e}");
            std::process::exit(1);
        }
    };
    let compiled = compile(&song, player.sample_rate).expect("compile failed");
    let total_frames = compiled.total_frames;
    let sample_rate = player.sample_rate;

    player.send(Command::SetSong(Box::new(compiled)));
    player.send(Command::Play);
    println!("Playing at {sample_rate} Hz — Ctrl-C to abort");

    loop {
        while let Some(event) = player.poll_event() {
            match event {
                EngineEvent::Playhead { frame, playing } if playing => {
                    print!(
                        "\r{:5.1}s / {:.1}s",
                        frame as f32 / sample_rate,
                        total_frames as f32 / sample_rate
                    );
                    use std::io::Write;
                    std::io::stdout().flush().ok();
                }
                EngineEvent::Finished => {
                    println!("\nDone.");
                    return;
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(30));
    }
}
