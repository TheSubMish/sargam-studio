use std::fs;
use std::path::Path;

fn main() {
    load_env_file();
    tauri_build::build()
}

/// Reads the repo-root `.env` (git-ignored — see `.env.example`) at build
/// time and exposes its values as compile-time env vars via `option_env!` in
/// auth.rs. Keeps real credentials out of every source file; if `.env`
/// doesn't exist (fresh checkout), this is a silent no-op and auth.rs reports
/// a clear "not configured" error at runtime instead of failing the build.
fn load_env_file() {
    let path = Path::new("../../.env");
    println!("cargo:rerun-if-changed={}", path.display());
    let Ok(contents) = fs::read_to_string(path) else {
        return;
    };
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            println!("cargo:rustc-env={}={}", key.trim(), value.trim());
        }
    }
}
