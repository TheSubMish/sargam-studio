//! UniFFI bindings over composer-core, for a native Kotlin (Android) or Swift
//! (iOS) UI to drive directly — no webview. Same responsibilities as
//! `composer-desktop`'s Tauri commands, just addressed by FFI calls instead of
//! `invoke()`; keep the two in sync when the API surface changes.
//!
//! composer-core stays UI-agnostic: this crate is the only place that knows
//! about UniFFI. Songs cross the FFI boundary as JSON strings (mirroring the
//! Tauri commands) rather than as UniFFI records, so composer-core never
//! depends on uniffi.

uniffi::setup_scaffolding!();

use std::sync::mpsc;
use std::sync::Mutex;

use composer_core::engine::{compile, render_song, write_wav, Command as EngineCommand, Player};
use composer_core::io::export_midi;
use composer_core::model::{builtin_taals, taal_by_name, Clip, Song, SongEditor};

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum MobileError {
    #[error("{0}")]
    Failed(String),
}

// One From impl per source error type (a blanket `impl<E: Error> From<E>`
// would collide with core's reflexive `impl<T> From<T> for T` once MobileError
// itself implements `std::error::Error` via the thiserror derive above).
macro_rules! failed_from {
    ($($ty:ty),* $(,)?) => {
        $(impl From<$ty> for MobileError {
            fn from(e: $ty) -> Self {
                Self::Failed(e.to_string())
            }
        })*
    };
}
failed_from!(
    std::io::Error,
    serde_json::Error,
    hound::Error,
    composer_core::engine::RenderError,
    composer_core::io::MidiError
);

#[derive(uniffi::Enum)]
pub enum MobileEvent {
    Playhead { frame: u64, playing: bool },
    Finished,
    None,
}

enum AudioMsg {
    Play(Song),
    Stop,
}

fn spawn_audio_thread() -> (
    mpsc::Sender<AudioMsg>,
    Mutex<mpsc::Receiver<composer_core::engine::EngineEvent>>,
) {
    let (cmd_tx, cmd_rx) = mpsc::channel::<AudioMsg>();
    let (evt_tx, evt_rx) = mpsc::channel::<composer_core::engine::EngineEvent>();
    std::thread::spawn(move || {
        let mut player: Option<Player> = None;
        loop {
            match cmd_rx.recv_timeout(std::time::Duration::from_millis(20)) {
                Ok(AudioMsg::Play(song)) => {
                    if player.is_none() {
                        player = Player::new().ok();
                    }
                    if let Some(p) = &mut player {
                        if let Ok(compiled) = compile(&song, p.sample_rate) {
                            p.send(EngineCommand::SetSong(Box::new(compiled)));
                            p.send(EngineCommand::Play);
                        }
                    }
                }
                Ok(AudioMsg::Stop) => {
                    if let Some(p) = &mut player {
                        p.send(EngineCommand::Stop);
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            if let Some(p) = &mut player {
                while let Some(e) = p.poll_event() {
                    let _ = evt_tx.send(e);
                }
            }
        }
    });
    (cmd_tx, Mutex::new(evt_rx))
}

/// The mobile app's single entry point: one engine per app process, held by
/// the platform UI (a `remember`-ed Compose object / a `@State` in SwiftUI).
#[derive(uniffi::Object)]
pub struct MobileEngine {
    editor: Mutex<SongEditor>,
    audio_tx: mpsc::Sender<AudioMsg>,
    audio_rx: Mutex<mpsc::Receiver<composer_core::engine::EngineEvent>>,
}

#[uniffi::export]
impl MobileEngine {
    #[uniffi::constructor]
    pub fn new() -> std::sync::Arc<Self> {
        let (audio_tx, audio_rx) = spawn_audio_thread();
        std::sync::Arc::new(Self {
            editor: Mutex::new(SongEditor::new(Song::new("Untitled", 100.0))),
            audio_tx,
            audio_rx,
        })
    }

    pub fn get_song_json(&self) -> String {
        self.editor.lock().unwrap().song().to_json()
    }

    pub fn set_song_json(&self, json: String) -> Result<(), MobileError> {
        let song = Song::from_json(&json)?;
        self.editor.lock().unwrap().edit(|s| *s = song);
        Ok(())
    }

    pub fn undo(&self) -> String {
        let mut ed = self.editor.lock().unwrap();
        ed.undo();
        ed.song().to_json()
    }

    pub fn redo(&self) -> String {
        let mut ed = self.editor.lock().unwrap();
        ed.redo();
        ed.song().to_json()
    }

    pub fn add_track(&self, name: String, instrument: String) -> String {
        let mut ed = self.editor.lock().unwrap();
        ed.add_track(composer_core::model::Track::new(name, instrument));
        ed.song().to_json()
    }

    /// Replace `track`'s first pattern with `cycles` reps of a built-in taal.
    pub fn apply_taal(
        &self,
        track: u32,
        taal_name: String,
        cycles: u32,
    ) -> Result<String, MobileError> {
        let taal = taal_by_name(&taal_name)
            .ok_or_else(|| MobileError::Failed(format!("unknown taal: {taal_name}")))?;
        let mut ed = self.editor.lock().unwrap();
        ed.edit(|song| {
            if let Some(t) = song.tracks.get_mut(track as usize) {
                let pattern = taal.to_pattern(cycles.max(1));
                if t.patterns.is_empty() {
                    t.patterns.push(pattern);
                } else {
                    t.patterns[0] = pattern;
                }
                t.clips.retain(|c| c.pattern != 0);
                t.clips.push(Clip {
                    pattern: 0,
                    start: 0,
                });
            }
        });
        Ok(ed.song().to_json())
    }

    pub fn play(&self) {
        let song = self.editor.lock().unwrap().song().clone();
        let _ = self.audio_tx.send(AudioMsg::Play(song));
    }

    pub fn stop(&self) {
        let _ = self.audio_tx.send(AudioMsg::Stop);
    }

    /// Drain queued playback events; call from a UI-side poll loop (e.g. a
    /// Compose `LaunchedEffect` ticking every ~30ms).
    pub fn poll_event(&self) -> MobileEvent {
        match self.audio_rx.lock().unwrap().try_recv() {
            Ok(composer_core::engine::EngineEvent::Playhead { frame, playing }) => {
                MobileEvent::Playhead {
                    frame: frame as u64,
                    playing,
                }
            }
            Ok(composer_core::engine::EngineEvent::Finished) => MobileEvent::Finished,
            Err(_) => MobileEvent::None,
        }
    }

    pub fn save_to_path(&self, path: String) -> Result<(), MobileError> {
        self.editor
            .lock()
            .unwrap()
            .song()
            .save(std::path::Path::new(&path))?;
        Ok(())
    }

    pub fn load_from_path(&self, path: String) -> Result<String, MobileError> {
        let song = Song::load(std::path::Path::new(&path))?;
        let mut ed = self.editor.lock().unwrap();
        *ed = SongEditor::new(song.clone());
        Ok(song.to_json())
    }

    pub fn export_wav(&self, path: String) -> Result<(), MobileError> {
        let song = self.editor.lock().unwrap().song().clone();
        let audio = render_song(&song, 44_100.0)?;
        write_wav(std::path::Path::new(&path), &audio, 2, 44_100)?;
        Ok(())
    }

    pub fn export_midi(&self, path: String) -> Result<(), MobileError> {
        let song = self.editor.lock().unwrap().song().clone();
        export_midi(&song, std::path::Path::new(&path))?;
        Ok(())
    }
}

#[uniffi::export]
pub fn list_instruments() -> Vec<String> {
    composer_core::instrument::catalog()
        .iter()
        .map(|i| i.name().to_string())
        .collect()
}

#[uniffi::export]
pub fn list_taal_names() -> Vec<String> {
    builtin_taals().into_iter().map(|t| t.name).collect()
}
