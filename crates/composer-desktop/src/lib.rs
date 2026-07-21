//! Tauri v2 shell around composer-core — shared by the desktop binary
//! (`main.rs`) and the Android/iOS app (`gen/android`, `gen/apple`), which
//! both just call [`run`]. Splitting this out of `main.rs` is required for
//! mobile: Tauri loads the app on Android as a JNI `cdylib`, which needs a
//! library crate target, not a binary.
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
//!
//! Projects/Settings/Profile: added for the "Sargam Projects"/"Sargam
//! Settings"/"Sargam Auth" designs. There's no backend server for this local
//! app, so "projects" are just `.song.json` files in a folder (`projects.rs`)
//! and "profile" is a local display name in the settings file (`settings.rs`)
//! — not a real account system. See those modules' docs for the reasoning.

mod auth;
mod marketplace;
mod projects;
mod settings;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Mutex;

use composer_core::engine::{
    compile, render_song, render_stems, write_wav_with_depth, BitDepth, Command as EngineCommand,
    Player,
};
use composer_core::io::export_midi;
use composer_core::model::{builtin_taals, taal_by_name, Pattern, Song, SongEditor, Taal};
use tauri::{AppHandle, Manager, State};

fn bit_depth_from_str(s: &str) -> BitDepth {
    match s {
        "24" => BitDepth::TwentyFour,
        "32" => BitDepth::ThirtyTwo,
        _ => BitDepth::Sixteen,
    }
}

enum AudioMsg {
    /// Compile and load `Song` into the engine, paused at frame 0.
    Load(Song),
    Play,
    /// Stop advancing but keep position — sounding notes release naturally.
    /// Resuming with Play continues from here (unlike Stop, which rewinds).
    Pause,
    Stop,
    SeekSecs(f32),
    /// Re-open the output device with a new preferred buffer size (settings
    /// change) — takes effect on the next Load/Play, not mid-stream.
    SetBufferSize(u32),
}

struct AudioHandle {
    tx: mpsc::Sender<AudioMsg>,
}

/// Write straight to Android's logcat via liblog (the app already links
/// liblog.so). This bypasses the `log` facade — Tauri installs its own logger,
/// so `log::` calls from here don't reliably reach logcat — guaranteeing the
/// audio-init result is visible via `adb logcat -s sargam`. On desktop it just
/// goes to stderr.
#[cfg(target_os = "android")]
fn audio_log(msg: &str) {
    extern "C" {
        fn __android_log_write(
            prio: std::os::raw::c_int,
            tag: *const std::os::raw::c_char,
            text: *const std::os::raw::c_char,
        ) -> std::os::raw::c_int;
    }
    if let (Ok(tag), Ok(text)) = (
        std::ffi::CString::new("sargam"),
        std::ffi::CString::new(msg),
    ) {
        unsafe { __android_log_write(4 /* ANDROID_LOG_INFO */, tag.as_ptr(), text.as_ptr()) };
    }
}
#[cfg(not(target_os = "android"))]
fn audio_log(msg: &str) {
    eprintln!("{msg}");
}

fn spawn_audio_thread(initial_buffer_size: u32) -> AudioHandle {
    let (tx, rx) = mpsc::channel::<AudioMsg>();
    std::thread::spawn(move || {
        let mut player: Option<Player> = None;
        let mut buffer_size = initial_buffer_size;
        while let Ok(msg) = rx.recv() {
            if let AudioMsg::SetBufferSize(frames) = msg {
                buffer_size = frames;
                player = None; // reopened lazily below with the new size
                continue;
            }
            if player.is_none() {
                match Player::with_buffer_size(Some(buffer_size)) {
                    Ok(p) => {
                        audio_log(&format!("audio: output stream opened @ {} Hz", p.sample_rate));
                        player = Some(p);
                    }
                    Err(e) => {
                        // Swallowed before (silent "no sound"); now surfaced so
                        // the failure is diagnosable.
                        audio_log(&format!("audio: failed to open output stream: {e}"));
                    }
                }
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
                AudioMsg::Pause => {
                    p.send(EngineCommand::Pause);
                }
                AudioMsg::Stop => {
                    p.send(EngineCommand::Stop);
                }
                AudioMsg::SeekSecs(secs) => {
                    let frame = (secs.max(0.0) * p.sample_rate) as usize;
                    p.send(EngineCommand::Seek(frame));
                }
                AudioMsg::SetBufferSize(_) => unreachable!("handled above"),
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
    // Refuse to "play" a song with nothing in it — avoids a transport that
    // looks active for a few silent seconds and then just stops.
    if state.editor.lock().unwrap().song().tracks.is_empty() {
        return;
    }
    state.ensure_loaded();
    let _ = state.audio.tx.send(AudioMsg::Play);
}

#[tauri::command]
fn pause(state: State<AppState>) {
    let _ = state.audio.tx.send(AudioMsg::Pause);
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
fn export_wav(app: AppHandle, state: State<AppState>, path: PathBuf) -> Result<(), String> {
    let settings = settings::load(&app);
    let song = state.editor.lock().unwrap().song().clone();
    let audio = render_song(&song, settings.sample_rate as f32).map_err(|e| e.to_string())?;
    write_wav_with_depth(
        &path,
        &audio,
        2,
        settings.sample_rate,
        bit_depth_from_str(&settings.bit_depth),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn export_midi_file(state: State<AppState>, path: PathBuf) -> Result<(), String> {
    let song = state.editor.lock().unwrap().song().clone();
    export_midi(&song, &path).map_err(|e| e.to_string())
}

/// One WAV per track, alongside the mixed master — the "export stems"
/// checkbox in the export dialog. `dir` is created if it doesn't exist.
#[tauri::command]
fn export_stems(app: AppHandle, state: State<AppState>, dir: PathBuf) -> Result<(), String> {
    let settings = settings::load(&app);
    let song = state.editor.lock().unwrap().song().clone();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stems = render_stems(
        &song,
        &composer_core::instrument::Registry::new(),
        settings.sample_rate as f32,
    )
    .map_err(|e| e.to_string())?;
    let depth = bit_depth_from_str(&settings.bit_depth);
    for (i, (name, audio)) in stems.iter().enumerate() {
        let safe_name: String = name
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();
        let path = dir.join(format!("{i:02}_{safe_name}.wav"));
        write_wav_with_depth(&path, audio, 2, settings.sample_rate, depth).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn set_buffer_size(state: State<AppState>, frames: u32) {
    let _ = state.audio.tx.send(AudioMsg::SetBufferSize(frames));
}

/// Android calls this once when `libsargam_desktop_lib.so` is loaded, handing
/// us the process JavaVM. Tao/wry keep the VM in their own state and never
/// populate the global `ndk_context`, but cpal's AAudio backend reads
/// `ndk_context::android_context().vm()` to run its JNI buffer-size query — so
/// without this it gets a null VM and every audio output stream fails to open
/// (the "no sound on Android" bug). We initialize ndk_context with the VM here;
/// the context jobject is null because cpal's default-device path only ever
/// uses the VM (AudioTrack.getMinBufferSize is a static call), never the
/// activity context.
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn JNI_OnLoad(
    vm: *mut std::ffi::c_void,
    _reserved: *mut std::ffi::c_void,
) -> std::ffi::c_int {
    unsafe {
        ndk_context::initialize_android_context(vm, std::ptr::null_mut());
    }
    0x0001_0006 // JNI_VERSION_1_6
}

/// Route `log` to Android's logcat (tag "sargam") so Rust diagnostics — which
/// otherwise vanish on Android (no console, stdout/stderr not captured) — are
/// visible via `adb logcat -s sargam`.
#[cfg(target_os = "android")]
fn init_logging() {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("sargam"),
    );
}
#[cfg(not(target_os = "android"))]
fn init_logging() {}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logging();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let initial = settings::load(&app.handle()).buffer_size;
            app.manage(AppState {
                editor: Mutex::new(SongEditor::new(Song::new("Untitled", 100.0))),
                audio: spawn_audio_thread(initial),
                dirty: AtomicBool::new(true),
            });
            Ok(())
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
            pause,
            stop,
            seek,
            save_song,
            load_song,
            export_wav,
            export_midi_file,
            export_stems,
            set_buffer_size,
            settings::get_settings,
            settings::set_settings,
            projects::list_projects,
            projects::new_project,
            projects::delete_project,
            auth::get_auth_status,
            auth::signup,
            auth::login,
            auth::logout,
            auth::delete_account,
            marketplace::get_public_config,
            marketplace::report_track,
            marketplace::block_creator,
            marketplace::unblock_creator,
            marketplace::list_blocked_creators,
            marketplace::explore_tracks,
            marketplace::list_wishlist,
            marketplace::toggle_wishlist,
            marketplace::upload_track,
            marketplace::download_track,
            marketplace::start_stripe_purchase,
            marketplace::finalize_purchase,
            marketplace::list_my_earnings,
            marketplace::request_payout,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
