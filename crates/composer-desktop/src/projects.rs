//! The Projects page's backend: local `.song.json` files in a folder are
//! *the* projects — there's no database and no server, just files.

use std::path::PathBuf;

use serde::Serialize;

use composer_core::model::Song;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub path: PathBuf,
    pub title: String,
    pub track_count: usize,
    pub duration_secs: f32,
    /// Seconds since the Unix epoch; the frontend formats this as relative
    /// time ("2 days ago") since that's inherently a display concern.
    pub modified_epoch_secs: u64,
}

const EXTENSION: &str = ".song.json";

#[tauri::command]
pub fn list_projects(dir: PathBuf) -> Result<Vec<ProjectSummary>, String> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.ends_with(EXTENSION) {
            continue;
        }
        // Skip anything that doesn't parse as a Song rather than failing the
        // whole listing — a hand-edited or corrupt file shouldn't hide every
        // other project.
        let Ok(song) = Song::load(&path) else { continue };
        let modified_epoch_secs = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let title = if song.title.trim().is_empty() {
            name.trim_end_matches(EXTENSION).to_string()
        } else {
            song.title.clone()
        };
        out.push(ProjectSummary {
            path,
            title,
            track_count: song.tracks.len(),
            duration_secs: song.duration_secs(),
            modified_epoch_secs,
        });
    }
    Ok(out)
}

/// Create a new, empty project file in `dir` and return its path. Never
/// clobbers an existing file — appends " 2", " 3", ... on a name collision.
#[tauri::command]
pub fn new_project(dir: PathBuf, title: String) -> Result<PathBuf, String> {
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let safe: String = title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let safe = safe.trim();
    let base = if safe.is_empty() { "Untitled" } else { safe };

    let mut path = dir.join(format!("{base}{EXTENSION}"));
    let mut suffix = 2;
    while path.exists() {
        path = dir.join(format!("{base} {suffix}{EXTENSION}"));
        suffix += 1;
    }

    let title = if title.trim().is_empty() {
        "Untitled".to_string()
    } else {
        title
    };
    Song::new(title, 100.0)
        .save(&path)
        .map_err(|e| e.to_string())?;
    Ok(path)
}

#[tauri::command]
pub fn delete_project(path: PathBuf) -> Result<(), String> {
    std::fs::remove_file(path).map_err(|e| e.to_string())
}
