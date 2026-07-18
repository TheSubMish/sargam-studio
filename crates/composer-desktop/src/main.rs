//! Tauri v2 desktop shell around composer-core.
//!
//! Architecture: the frontend owns a JSON copy of the Song and edits it
//! directly; every mutation round-trips through `set_song` (which feeds the
//! undo stack). Audio runs on a dedicated thread because cpal streams are not
//! Send — commands reach it over an mpsc channel.
//!
//! Transport: the audio thread keeps one compiled song loaded across Play/
//! Stop/Seek so seeking and resuming don't restart playback from zero. A
//! `dirty` flag (set by every song-mutating command) tells `play`/`seek`
//! whether they need to push a fresh `AudioMsg::Load` before acting — so an
//! edit takes effect on the next Play without recompiling on every frame.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Mutex;

use composer_core::engine::{
    compile, render_song, render_stems, write_wav, Command as EngineCommand, Player,
};
use composer_core::io::export_midi;
use composer_core::model::{builtin_taals, taal_by_name, Pattern, Song, SongEditor, Taal};
use tauri::State;

enum AudioMsg {
    /// Compile and load `Song` into the engine, paused at frame 0.
    Load(Song),
    Play,
    Stop,
    SeekSecs(f32),
}

struct AudioHandle {
    tx: mpsc::Sender<AudioMsg>,
}

fn spawn_audio_thread() -> AudioHandle {
    let (tx, rx) = mpsc::channel::<AudioMsg>();
    std::thread::spawn(move || {
        let mut player: Option<Player> = None;
        while let Ok(msg) = rx.recv() {
            if player.is_none() {
                player = Player::new().ok();
            }
            let Some(p) = &mut player else { continue };
            match msg {
                AudioMsg::Load(song) => {
                    if let Ok(compiled) = compile(&song, p.sample_rate) {
                        p.send(EngineCommand::SetSong(Box::new(compiled)));
                    }
                }
                AudioMsg::Play => {
                    p.send(EngineCommand::Play);
                }
                AudioMsg::Stop => {
                    p.send(EngineCommand::Stop);
                }
                AudioMsg::SeekSecs(secs) => {
                    let frame = (secs.max(0.0) * p.sample_rate) as usize;
                    p.send(EngineCommand::Seek(frame));
                }
            }
        }
    });
    AudioHandle { tx }
}

struct AppState {
    editor: Mutex<SongEditor>,
    audio: AudioHandle,
    /// True whenever the audio thread's loaded song may be stale relative to
    /// `editor`. Cleared once `play`/`seek` pushes a fresh `Load`.
    dirty: AtomicBool,
}

impl AppState {
    /// Send the current song state to the audio thread if it might be stale.
    fn ensure_loaded(&self) {
        if self.dirty.swap(false, Ordering::SeqCst) {
            let song = self.editor.lock().unwrap().song().clone();
            let _ = self.audio.tx.send(AudioMsg::Load(song));
        }
    }
}

#[tauri::command]
fn get_song(state: State<AppState>) -> Song {
    state.editor.lock().unwrap().song().clone()
}

#[tauri::command]
fn set_song(state: State<AppState>, song: Song) {
    state.editor.lock().unwrap().edit(|s| *s = song);
    state.dirty.store(true, Ordering::SeqCst);
}

#[tauri::command]
fn undo(state: State<AppState>) -> Song {
    let mut ed = state.editor.lock().unwrap();
    ed.undo();
    state.dirty.store(true, Ordering::SeqCst);
    ed.song().clone()
}

#[tauri::command]
fn redo(state: State<AppState>) -> Song {
    let mut ed = state.editor.lock().unwrap();
    ed.redo();
    state.dirty.store(true, Ordering::SeqCst);
    ed.song().clone()
}

#[tauri::command]
fn list_instruments() -> Vec<String> {
    composer_core::instrument::catalog()
        .iter()
        .map(|i| i.name().to_string())
        .collect()
}

#[tauri::command]
fn list_taals() -> Vec<Taal> {
    builtin_taals()
}

/// Replace `track`'s first pattern with `cycles` repetitions of a built-in
/// taal — the "start from a groove" shortcut percussion tracks get instead of
/// an empty grid.
#[tauri::command]
fn apply_taal(
    state: State<AppState>,
    track: usize,
    taal_name: String,
    cycles: u32,
) -> Result<Song, String> {
    let taal = taal_by_name(&taal_name).ok_or_else(|| format!("unknown taal: {taal_name}"))?;
    let mut ed = state.editor.lock().unwrap();
    ed.edit(|song| {
        if let Some(t) = song.tracks.get_mut(track) {
            let pattern = taal.to_pattern(cycles.max(1));
            if t.patterns.is_empty() {
                t.patterns.push(pattern);
            } else {
                t.patterns[0] = pattern;
            }
            t.clips.retain(|c| c.pattern != 0);
            t.clips.push(composer_core::model::Clip { pattern: 0, start: 0 });
        }
    });
    state.dirty.store(true, Ordering::SeqCst);
    Ok(ed.song().clone())
}

#[tauri::command]
fn add_track(state: State<AppState>, name: String, instrument: String) -> Song {
    let mut ed = state.editor.lock().unwrap();
    ed.add_track(composer_core::model::Track::new(name, instrument));
    // Every track starts with one empty pattern so the grid has something to edit.
    let idx = ed.song().tracks.len() - 1;
    ed.edit(|song| {
        song.tracks[idx].patterns.push(Pattern {
            name: "Pattern 1".into(),
            length: 8 * composer_core::model::TICKS_PER_BEAT,
            notes: Vec::new(),
        });
        song.tracks[idx]
            .clips
            .push(composer_core::model::Clip { pattern: 0, start: 0 });
    });
    state.dirty.store(true, Ordering::SeqCst);
    ed.song().clone()
}

#[tauri::command]
fn play(state: State<AppState>) {
    state.ensure_loaded();
    let _ = state.audio.tx.send(AudioMsg::Play);
}

#[tauri::command]
fn stop(state: State<AppState>) {
    let _ = state.audio.tx.send(AudioMsg::Stop);
}

#[tauri::command]
fn seek(state: State<AppState>, position_secs: f32) {
    state.ensure_loaded();
    let _ = state.audio.tx.send(AudioMsg::SeekSecs(position_secs));
}

#[tauri::command]
fn save_song(state: State<AppState>, path: PathBuf) -> Result<(), String> {
    state
        .editor
        .lock()
        .unwrap()
        .song()
        .save(&path)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn load_song(state: State<AppState>, path: PathBuf) -> Result<Song, String> {
    let song = Song::load(&path).map_err(|e| e.to_string())?;
    let mut ed = state.editor.lock().unwrap();
    *ed = SongEditor::new(song.clone());
    state.dirty.store(true, Ordering::SeqCst);
    Ok(song)
}

#[tauri::command]
fn export_wav(state: State<AppState>, path: PathBuf) -> Result<(), String> {
    let song = state.editor.lock().unwrap().song().clone();
    let audio = render_song(&song, 44_100.0).map_err(|e| e.to_string())?;
    write_wav(&path, &audio, 2, 44_100).map_err(|e| e.to_string())
}

#[tauri::command]
fn export_midi_file(state: State<AppState>, path: PathBuf) -> Result<(), String> {
    let song = state.editor.lock().unwrap().song().clone();
    export_midi(&song, &path).map_err(|e| e.to_string())
}

/// One WAV per track, alongside the mixed master — the "export stems"
/// checkbox in the export dialog. `dir` is created if it doesn't exist.
#[tauri::command]
fn export_stems(state: State<AppState>, dir: PathBuf) -> Result<(), String> {
    let song = state.editor.lock().unwrap().song().clone();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stems = render_stems(&song, &composer_core::instrument::Registry::new(), 44_100.0)
        .map_err(|e| e.to_string())?;
    for (i, (name, audio)) in stems.iter().enumerate() {
        let safe_name: String = name
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();
        let path = dir.join(format!("{i:02}_{safe_name}.wav"));
        write_wav(&path, audio, 2, 44_100).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            editor: Mutex::new(SongEditor::new(Song::new("Untitled", 100.0))),
            audio: spawn_audio_thread(),
            dirty: AtomicBool::new(true),
        })
        .invoke_handler(tauri::generate_handler![
            get_song,
            set_song,
            undo,
            redo,
            list_instruments,
            list_taals,
            apply_taal,
            add_track,
            play,
            stop,
            seek,
            save_song,
            load_song,
            export_wav,
            export_midi_file,
            export_stems,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
