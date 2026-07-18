# Getting started with Sargam

Sargam (सरगम) is a music composer for Nepali folk instruments. This doc covers how to build and launch it. For architecture, roadmap, and the day-by-day development log, see [`CLAUDE.md`](../CLAUDE.md) at the repo root.

## Prerequisites

- **Rust** (stable, tested with 1.89). Install via [rustup](https://rustup.rs) if you don't have it.
- **Desktop app only** — Tauri's webview system packages. On Ubuntu/Debian:
  ```
  sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev patchelf
  ```
  (Other platforms: see [Tauri's prerequisites guide](https://v2.tauri.app/start/prerequisites/) — macOS and Windows need no extra system packages beyond Xcode Command Line Tools / the Visual Studio Build Tools.)

Nothing else is required to build and test `composer-core` (the engine) — it's a plain Rust crate with no system dependencies.

## Run the desktop app

From the repository root:

```
cargo build -p sargam-desktop
./target/debug/sargam-desktop
```

Or, for a dev loop with the Tauri CLI (`cargo install tauri-cli --version '^2'` once, first):

```
cd crates/composer-desktop
cargo tauri dev
```

A window titled **"Sargam — Nepali Folk Music Composer"** should open. From there:

1. Click an instrument in the left sidebar (बाजाहरू) to add a track — or **+ Track** for a custom name.
2. Select a track, then click cells in the grid to add notes (piano roll for melodic instruments, a stroke grid for percussion). Percussion tracks get a **Taal** picker to start from a built-in groove (Jhyaure, Tamang Selo, Teej).
3. Press **▶** to play, drag the seek bar to scrub, **File → Save…** to keep your work.

First launch shows a short onboarding flow; replay it anytime via **File → Replay onboarding**.

The top navbar switches between three pages:

- **Compose** — the editor above.
- **Projects** — a real local project browser: every `.song.json` file in your project folder (default `~/Documents/Sargam Projects`, changeable in Settings → Export) shows up here with its actual track count and duration. **+ New Project** creates and opens one; there's no separate "save to projects" step — Save/Open still work with any file path, same as before.
- **Settings** — workspace (timeline zoom, auto-save, language), audio (sample rate, bit depth, buffer size — these actually feed exports and playback), your local display name, and export defaults. This app has no user accounts or cloud sync — the display name is just a local label shown as initials in the navbar, stored in the same settings file, nothing more.

## Run the tests

```
cargo test
```

This runs only `composer-core`'s test suite (unit + integration tests covering every instrument, the taal/tempo/undo model, MIDI import/export, the SFZ sampler, and the real-time transport). The workspace's `default-members` deliberately excludes `composer-desktop`/`composer-mobile`, so plain `cargo test` never needs Tauri or Android toolchains installed.

## Render audio without the UI

A few standalone examples exercise the engine directly and write real audio/project files to the current directory:

```
cargo run --example tungna_demo    # solo tungna phrase → tungna_demo.wav
cargo run --example jhyaure_demo   # 2-track song (damphu + tungna) → .wav + .song.json
cargo run --example ensemble_demo  # full 5-instrument arrangement → .wav + .song.json + .mid
cargo run --example play_song -- ensemble_demo.song.json   # play a saved song through real speakers
```

## Mobile (Android)

The Android SDK cmdline-tools, NDK 27.1.12297006, and the four Rust Android targets are already set up if you're on the machine this was originally bootstrapped on (see the Environment setup note in `CLAUDE.md`). To build:

```
export ANDROID_HOME=~/Android/Sdk
export NDK_HOME=$ANDROID_HOME/ndk/27.1.12297006
cd crates/composer-desktop
cargo tauri android init   # first time only
cargo tauri android dev    # or `build` for a release APK
```

`composer-mobile` (the UniFFI fallback, if the Tauri Android webview underperforms on real devices) builds natively today to validate its bindings compile:

```
cargo build -p sargam-mobile
```

**iOS cannot be built here** — it requires Xcode, which only runs on macOS.

## Troubleshooting

- **A file dialog (Save/Open/Export) silently does nothing**: check `crates/composer-desktop/capabilities/default.json` exists and grants `dialog:default` to the `main` window. Tauri v2 rejects plugin commands (like the native file picker) at runtime without an explicit capability grant — and by default that rejection is a silently-swallowed promise rejection in the frontend, not a visible error.
- **Build fails looking for `frontendDist`**: `tauri.conf.json`'s `build.frontendDist` is `"../../ui"`, resolved relative to `crates/composer-desktop` — don't move `tauri.conf.json` without updating that path.
- **Build fails on a missing icon**: `crates/composer-desktop/icons/` must contain the PNGs listed in `tauri.conf.json`'s `bundle.icon`. They're currently flat placeholder color swatches — replace with real artwork whenever branding is finalized.
- **`cargo build -p sargam-desktop` fails with pkg-config/webkit errors**: the system packages above aren't installed. `cargo check -p sargam-desktop` will point at exactly which library is missing.
