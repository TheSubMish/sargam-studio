//! Local app preferences — persisted as JSON under Tauri's app config dir.
//! Real user identity (name/email/password) lives in `auth.rs`, not here —
//! this file is workspace/audio/export prefs only.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

fn default_timeline_zoom() -> String {
    "normal".into()
}
fn default_sample_rate() -> u32 {
    44_100
}
fn default_bit_depth() -> String {
    "16".into()
}
fn default_buffer_size() -> u32 {
    512
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    // — Workspace —
    /// Grid cell size preset for the piano-roll/taal editor: "compact" |
    /// "normal" | "expanded".
    #[serde(default = "default_timeline_zoom")]
    pub timeline_zoom: String,
    /// Periodically re-save to the project's current path (only takes effect
    /// once a project has been saved/opened at least once, same as any
    /// autosave — there's nowhere to autosave an unsaved file to).
    #[serde(default)]
    pub auto_save: bool,
    /// UI language. Only "en" is actually translated right now; "ne" is
    /// offered in the picker (matching the source design's own precedent of
    /// listing "Dark (coming soon)") but doesn't change anything yet.
    #[serde(default = "default_language")]
    pub language: String,
    /// Only "light" actually renders anything right now — "dark" is offered
    /// disabled in the picker, same honesty pattern as the language field.
    #[serde(default = "default_theme")]
    pub theme: String,

    // — Audio / export —
    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,
    /// "16" | "24" | "32" (32 = float, not integer — see `BitDepth`).
    #[serde(default = "default_bit_depth")]
    pub bit_depth: String,
    /// Requested cpal output buffer size in frames; applied on next audio
    /// device open (best-effort — see `Player::with_buffer_size`).
    #[serde(default = "default_buffer_size")]
    pub buffer_size: u32,
    /// Pre-checks "Export stems" in the export dialog.
    #[serde(default)]
    pub export_stems_default: bool,
    /// Folder the Projects page scans/saves into.
    pub projects_dir: PathBuf,
}

fn default_language() -> String {
    "en".into()
}
fn default_theme() -> String {
    "light".into()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            timeline_zoom: default_timeline_zoom(),
            auto_save: false,
            language: default_language(),
            theme: default_theme(),
            sample_rate: default_sample_rate(),
            bit_depth: default_bit_depth(),
            buffer_size: default_buffer_size(),
            export_stems_default: false,
            projects_dir: PathBuf::new(), // filled in by `load`, which always has an AppHandle
        }
    }
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("settings.json"))
}

fn default_projects_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .document_dir()
        .map(|d| d.join("Sargam Projects"))
        .unwrap_or_else(|_| PathBuf::from("Sargam Projects"))
}

pub fn load(app: &AppHandle) -> AppSettings {
    let path = match settings_path(app) {
        Ok(p) => p,
        Err(_) => {
            return AppSettings {
                projects_dir: default_projects_dir(app),
                ..Default::default()
            }
        }
    };
    match std::fs::read_to_string(&path) {
        Ok(json) => {
            let mut settings: AppSettings = serde_json::from_str(&json).unwrap_or_default();
            if settings.projects_dir.as_os_str().is_empty() {
                settings.projects_dir = default_projects_dir(app);
            }
            settings
        }
        Err(_) => AppSettings {
            projects_dir: default_projects_dir(app),
            ..Default::default()
        },
    }
}

pub fn save(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    let path = settings_path(app)?;
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> AppSettings {
    load(&app)
}

#[tauri::command]
pub fn set_settings(app: AppHandle, settings: AppSettings) -> Result<(), String> {
    save(&app, &settings)
}
