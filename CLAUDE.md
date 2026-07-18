# Sargam (सरगम) — Nepali Folk Music Composer

A music composer app for Nepali folk instruments (tungna, damphu, madal, sarangi, harmonium, shehnai, bansuri, ...). Desktop first (Rust), then mobile (Android + iOS) reusing the same core. Named Sargam after the Sa-Re-Ga-Ma solfège syllables.

## Vision & architecture

- **`composer-core`** is a pure-Rust, UI-free crate holding ALL music logic and audio. Desktop and mobile are thin shells around it — this is what makes the mobile port cheap. It knows nothing about Tauri or UniFFI.
- Instruments are built from **reusable DSP building blocks** (physical modeling / synthesis), optionally hybridized with recorded samples (SFZ). Karplus-Strong for plucked strings, modal synthesis for struck drums, ADSR + band-limited oscillators for sustaining winds/reeds, comb-filter reverb + feedback delay for per-track sends — all shared across instruments/tracks.
- Desktop UI: **Tauri v2**, plain HTML/CSS/JS frontend (no build step) in `ui/`, talking to Rust via `invoke()`. Visual design imported from the Claude Design project `c2fc1636-0a9a-4823-8313-a9adedc9fe63` ("Sargam v3.dc.html", the "Organic" design system — warm cream/terracotta/sage palette, Caprasimo display font over Figtree, pill-shaped controls). Mobile: same Tauri app targets Android/iOS; fallback plan is `composer-mobile` (UniFFI bindings) driving a native Compose/SwiftUI UI if the webview underperforms on real devices.
- Real-time audio rule: **the audio path never allocates, locks, or does I/O.** UI ↔ engine communication goes through lock-free ring buffers (`engine::realtime`).

### Layers inside `composer-core` (each depends only on those above)

| Module | Role |
|---|---|
| `dsp/` | Instrument-agnostic algorithms: `DelayLine`, `OnePoleLowPass`, `ExpDecay`, `Adsr`, `WhiteNoise`, `Phase`/`Sine`/`Saw` (polyBLEP), `KarplusStrong`, `ModalBank`, `AllpassFilter`, `PlateReverb` (4 damped combs + 2 allpass), `EchoDelay` (feedback delay). One file per algorithm; must stay reusable & allocation-free in `tick()`. |
| `instrument/` | One file per instrument, composing `dsp` blocks behind `Instrument` (factory) + `Voice` traits. `modal_drum.rs` is the shared struck-membrane voice (damphu/madal supply only a `Voicing` table). `ornament.rs::PitchLine` turns meend/gamak into a pitch trajectory for melodic voices. `sampler.rs` plays SFZ-mapped recordings through the same `Voice` trait. `Registry` resolves track instrument names, built-ins + user-loaded samplers. |
| `model/` | Serde-backed domain model: `Song` (title, `TempoMap`, `master_gain`, tracks) → `Track` (gain, pan, `reverb`/`delay` send amounts) → `Pattern`/`Clip` → `PatternNote`. `Trigger` (Pitch/Stroke), `Ornament` (Plain/Meend/Gamak), `NoteParams`. `Taal` templates (`builtin_taals`: Jhyaure, Tamang Selo, Teej) expand to patterns. `TempoMap` supports mid-song tempo changes. `SongEditor` wraps a `Song` with undo/redo (snapshot-based). |
| `engine/` | Two paths sharing voice + effects logic: offline (`render_track`, `render_song`/`render_song_with`, `render_stems` + `write_wav`) for exports/tests, and real-time (`realtime::EngineCore` + `player::Player`) for live playback — voice pooling/stealing, Play/Pause/Stop/Seek, playhead + Finished events, per-track reverb/delay inserts, a live-monitoring `master_gain` fader, all behind lock-free `ringbuf` queues; cpal owns the actual audio device. |
| `io/` | File interchange: `midi.rs` exports/imports standard MIDI (percussion ↔ channel 10 via GM key mapping, tempo map ↔ MIDI tempo meta-events). Project persistence itself is native JSON on `Song` (`save`/`load`), not in this module. |

## Project structure

```
program/
├── CLAUDE.md
├── Cargo.toml                      # workspace; default-members = [composer-core] only
├── ui/                              # Tauri frontend: plain HTML/CSS/JS, no build step
│   ├── index.html  style.css  app.js  # style.css tokens imported from Claude Design ("Organic" system)
└── crates/
    ├── composer-core/
    │   ├── src/
    │   │   ├── dsp/         # delay_line, filter, envelope, adsr, oscillator, noise, karplus_strong,
    │   │   │                # modal, allpass, reverb (PlateReverb), delay_fx (EchoDelay)
    │   │   ├── instrument/  # mod.rs, tungna, damphu, madal, modal_drum, harmonium, bansuri, ornament, sampler
    │   │   ├── model/       # mod.rs, song, taal, tempo, editor
    │   │   ├── engine/      # mod.rs, render (offline + stems), realtime (EngineCore), player (cpal)
    │   │   └── io/          # mod.rs, midi
    │   ├── examples/        # tungna_demo, jhyaure_demo, ensemble_demo, play_song
    │   └── tests/           # engine, model, damphu, instruments, tempo, realtime, persistence, sampler, effects
    ├── composer-desktop/     # Tauri v2 app (package: sargam-desktop)
    │   ├── Cargo.toml  tauri.conf.json  build.rs  icons/
    │   └── src/main.rs       # Tauri commands: get/set_song, undo/redo, play/stop/seek, save/load,
    │                         #   export wav/midi/stems, apply_taal, add_track
    └── composer-mobile/      # UniFFI fallback (package: sargam-mobile)
        ├── Cargo.toml  uniffi-bindgen.rs
        └── src/lib.rs        # MobileEngine object mirroring the desktop commands
```

## How to add a new instrument

1. If it needs a new DSP algorithm, add it as its own file in `dsp/` — generic, reusable, no instrument-specific constants.
2. Create `src/instrument/<name>.rs`: an `Instrument` impl (parameters + `make_voice`) and a `Voice` impl composing DSP blocks. Struck drums: supply a `Voicing` table to the shared `modal_drum::ModalDrumVoice` instead of writing a new voice. Melodic instruments that should bend: drive pitch through `ornament::PitchLine` so meend/gamak work for free.
3. Register: declare the module + re-export in `instrument/mod.rs`, add it to `catalog()`.
4. Add an example rendering a short phrase, and tests (non-silent bounded output, decays to silence, tuning test if pitched, timbre distinction between articulations if relevant).
5. Log it in the progress log below.

## Conventions

- `cargo test` (from repo root) covers `composer-core` only by design — `default-members` keeps it free of Tauri/UniFFI system deps. Must pass before any milestone is called done. Listen to rendered WAVs too — tests don't hear timbre.
- Percussion instruments use stroke types (Bass/Open/Rim/Slap/Mute), not MIDI pitches. `Stroke::to_gm_key`/`from_gm_key` map them to GM percussion for MIDI/sampler interchange.
- DSP gotcha: filters inside feedback loops add phase delay — compensate in delay-line length or notes play flat (`OnePoleLowPass::phase_delay_samples`, used by `KarplusStrong`).
- Real-time engine gotcha: `EngineCore::process` must never allocate/lock — the one exception is dropping the previous `CompiledSong` on `Command::SetSong`, an explicit, infrequent trade.
- `composer-desktop` and `composer-mobile` are workspace members but **not** default-members, so they don't need their system deps (Tauri webview libs / Android NDK) just to work on `composer-core`. Build them explicitly: `cargo build -p sargam-desktop`, `cargo build -p sargam-mobile`.
- Temporary/experiment files go to the session scratchpad, not the repo.

## Environment setup done on this machine (2026-07-18)

- Installed `tauri-cli` (`cargo install tauri-cli --version '^2'`) — pure Rust, no system libs needed.
- Installed Tauri's Linux webview system packages via `sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev patchelf` (user ran this manually — sudo needs an interactive password the harness can't supply).
- Bootstrapped Android SDK cmdline-tools (`~/Android/Sdk/cmdline-tools/latest`, downloaded from `dl.google.com/android/repository/commandlinetools-linux-14742923_latest.zip` — Google rotates this build number; re-search if it 404s) and installed NDK 27.1.12297006 + platform-tools + platforms;android-34 + build-tools;34.0.0 via `sdkmanager`.
- Installed Rust Android targets: `aarch64-linux-android`, `armv7-linux-androideabi`, `x86_64-linux-android`, `i686-linux-android`.
- iOS cannot be built or tested on this machine — Xcode only runs on macOS. `composer-mobile`'s Swift bindings and any `cargo tauri ios` work must happen on a Mac.

## Roadmap

- [x] **Phase 0** — workspace scaffold, DSP core, first instrument proves the architecture
- [x] **Phase 1a** — song model (serde), taal templates, damphu (modal synthesis), multi-track stereo render
- [x] **Phase 1b** — madal, harmonium, bansuri, tempo map, meend/gamak ornaments
- [x] **Phase 2** — real-time playback: cpal, voice pool + stealing, lock-free command/event queues, transport (Play/Pause/Stop/Seek), playhead/Finished events
- [x] **Phase 3** — Tauri v2 desktop app, running and verified on this machine (screenshotted, real cpal audio)
- [x] **Phase 4** — `SongEditor` undo/redo, standard MIDI import/export
- [x] **Phase 5** — SFZ sampler (`SamplerInstrument`) + `Registry`, so recorded instrument packs plug in alongside synthesized ones
- [~] **Phase 6 (scaffolded)** — `composer-mobile` UniFFI crate (builds natively); Android NDK/SDK bootstrapped on this machine; Tauri Android/iOS build not yet run
- [x] **Phase 3 visual redesign** — real UI imported from a Claude Design mockup, with reverb/delay/master-fader effects added to back it
- [ ] **Phase 7** — polish & distribution (installers, stores, bundled demo song)

## Progress log

### 2026-07-18 — Phase 0 complete
- Decided architecture (shared Rust core, layered dsp → instrument → model/engine; Tauri v2 planned for UI).
- DSP primitives: `DelayLine`, `OnePoleLowPass`, `ExpDecay`, `WhiteNoise`, `KarplusStrong`.
- First instrument: **Tungna**. Offline renderer + WAV export. 4 tests passing.
- Fixed a real tuning bug: loop-filter phase delay made notes ~5–13 cents flat; compensated via `phase_delay_samples()`.

### 2026-07-18 — Phase 1a complete
- Domain model with serde: `Song`/`Track`/`Pattern`/`Clip`/`PatternNote`, tick-based, JSON save/load.
- `Trigger` enum (Pitch/Stroke) — percussion is stroke-based. New `ModalBank` DSP block. **Damphu** instrument.
- Taal templates (`Taal::to_pattern`; Jhyaure, Tamang Selo, Teej). `render_song`: multi-track stereo mixing.
- 12 tests passing. Deferred the SFZ sampler to Phase 5 deliberately (nothing to load yet).

### 2026-07-18 — Phase 1b complete
- New DSP: `Adsr` (linear ADSR for sustaining instruments), `Phase`/`Sine`/`Saw` (polyBLEP band-limited oscillators).
- Refactored damphu into `instrument::modal_drum` (shared `ModalDrumVoice` + per-drum `Voicing` fn) so **Madal** (two heads, harmonic-loaded high head vs. inharmonic low head) reuses it entirely.
- **Harmonium**: two detuned band-limited saws (reed banks) → cabinet low-pass → ADSR → bellows tremolo. First sustaining instrument — proved `Voice::is_active` tied to envelope state, not a fixed decay.
- **Bansuri**: phase-locked harmonic stack + filtered breath noise + delayed-onset vibrato. Fully supports ornaments.
- `Ornament` (Plain/Meend/Gamak) + `NoteParams` threaded through the whole pipeline (model → engine → `Voice::note_on`). `ornament::PitchLine` converts an ornament to a per-sample pitch trajectory; `KarplusStrong::set_freq` and bansuri/tungna retune mid-note for meend glides.
- `TempoMap`: piecewise-constant tempo changes, `Song.tempo` replaces the old flat `bpm` field.
- `Voice` trait gained `kill()` (immediate silence — voice stealing / transport stop).
- 27 tests passing (added `instruments.rs`, `tempo.rs`; timbre/tuning/sustain/ornament-slide checks via autocorrelation).

### 2026-07-18 — Phase 2 complete
- `engine::realtime`: `EngineCore` (the actual cpal callback body) driven by `Command`s (SetSong/Play/Pause/Stop/Seek) over a `ringbuf` queue, emitting `EngineEvent` (Playhead/Finished) back — no locks or allocation in `process()` except the one explicit song-swap on `SetSong`.
- Per-track voice pools (8 voices) with round-robin stealing once exhausted; `compile()`/`compile_with()` resolve a `Song` into frame-accurate events ahead of time on the control thread.
- `engine::player::Player` wraps cpal device setup; example `play_song` plays a saved `.song.json` through real speakers with a live playhead readout — verified audibly on this machine's sof-hda-dsp output.
- 4 transport tests (`realtime.rs`) drive `EngineCore` directly with plain buffers, no audio device needed: silence-until-play, stop-rewinds, seek-lands-correctly, playhead/Finished events fire.

### 2026-07-18 — Phase 3 (code) + Phase 4 + Phase 5 complete
- **Phase 4**: `model::editor::SongEditor` — snapshot-based undo/redo (songs are small; simplicity over a diff-based history), typed helpers (`add_track`, `add_note`, `set_tempo`, ...) plus a raw `edit(FnOnce(&mut Song))` escape hatch. `io::midi`: format-1 SMF export/import — percussion ↔ channel 10 via `Stroke::to_gm_key`, tempo map ↔ tempo meta-events, instrument resolved from MIDI track name when it matches the catalog. Ornaments/fractional pitch don't survive MIDI by design (JSON is the full-fidelity format).
- **Phase 5**: `instrument::sampler::SamplerInstrument` loads a hand-rolled SFZ subset (`<region>` key/velocity ranges, `pitch_keycenter`, `loop_mode`/`loop_start`/`loop_end`, `ampeg_release`), decodes WAV via `hound`, resamples by playback-rate ratio (linear interpolation — fine until real recordings need `rubato`/`symphonia` for non-WAV/higher-quality resampling). `instrument::Registry` resolves track instrument names through user-loaded samplers first, falling back to the built-in catalog, so a sampled "Madal" can shadow the synthesized one. Tests generate sine-wave WAV fixtures rather than depending on real recordings.
- **Phase 3**: `composer-desktop` (package `sargam-desktop`) — Tauri v2 app. Commands mirror `SongEditor`/`engine` 1:1 (`get_song`/`set_song`/`undo`/`redo`/`play`/`stop`/`save_song`/`load_song`/`export_wav`/`export_midi_file`/`apply_taal`/`add_track`/`list_instruments`/`list_taals`). Audio runs on its own thread (cpal streams aren't `Send`) talking to the main state over `mpsc`. Frontend (`ui/`) is dependency-free HTML/CSS/JS: transport bar, track sidebar (gain/pan sliders, add/remove), and a canvas grid editor that switches between a piano roll (melodic tracks, with a subtle A-minor-pentatonic highlight) and a 5-row stroke grid (percussion, with an "Apply Taal" shortcut) — both edit the same `PatternNote` JSON that round-trips through `set_song`.
- Workspace restructured: `composer-desktop`/`composer-mobile` added as members but excluded from `default-members`, so `cargo build`/`cargo test` at the root stay scoped to `composer-core` and don't need Tauri/Android toolchains installed.
- 33 tests passing across `composer-core` (added `persistence.rs`, `sampler.rs`).

### 2026-07-18 — Phase 6 scaffolded, environment bootstrapped
- `composer-mobile` (package `sargam-mobile`): UniFFI bindings over `composer-core`, mirroring the desktop Tauri commands as `MobileEngine` methods (JSON in/out, same reasoning: keep `composer-core` free of any UI-framework dependency, including UniFFI). Builds natively (`cargo build -p sargam-mobile`) to validate the proc-macro scaffolding without needing a device toolchain.
- Bootstrapped this machine for real mobile builds: installed `tauri-cli`; user installed Tauri's Linux webview packages via `apt`; downloaded Android cmdline-tools and installed NDK 27.1.12297006 + platform-tools + android-34 platform/build-tools via `sdkmanager`; added the four Android Rust targets.
- **Not done**: actually running `cargo tauri android init`/`build` (needs the desktop app itself buildable first) or anything iOS (categorically needs a Mac — documented, not attempted).

### 2026-07-18 — Desktop app confirmed running; visual redesign from Claude Design import
- User installed Tauri's Linux webview packages (`libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `librsvg2-dev`, `patchelf`); `sargam-desktop` builds and **runs for real** on this machine (Wayland session, X11/GTK backend) — confirmed via a real screenshot (captured through `xwd` + a hand-rolled XWD→PNG parser, since no screenshot tool/`xdotool`/pip was available without another sudo prompt).
- Fixed two first-run bugs: `frontendDist` in `tauri.conf.json` was one directory level short (`../ui` → `../../ui`, since it resolves relative to `crates/composer-desktop`); Tauri's context macro also requires bundle icons to exist — generated placeholder solid-color PNGs (`crates/composer-desktop/icons/`) with a small inline Python PNG writer (no ImageMagick/PIL available).
- **Imported the visual design** from Claude Design project `c2fc1636-0a9a-4823-8313-a9adedc9fe63` ("Sargam v3.dc.html", via the `DesignSync` MCP tool) and rebuilt `ui/` around it: the "Organic" design system (cream/terracotta/sage palette, Caprasimo display font over Figtree, 999px pill controls, 28px card radii — tokens copied into `ui/style.css`). Layout: header (brand mark, project-name pill, File menu, Export), transport (round Play/Stop, seek bar, Tempo pill, Taal quick-pick pills), 3-pane main area (instrument sidebar → track list + grid editor → mixer inspector), export modal. The mockup's own per-track "16 clip blocks" were empty no-op placeholders in its reference JS, so that area was adapted into a track list (with a small note-position sparkline) sitting above the existing functional piano-roll/taal-grid editor for whichever track is selected — more useful than reproducing an inert sketch pixel-for-pixel.
- **Backed the design's mixer controls with real DSP** rather than decorative sliders: added `dsp::AllpassFilter`, `dsp::PlateReverb` (4 damped combs + 2 series allpasses, Freeverb-style tuning scaled to sample rate) and `dsp::EchoDelay` (single-tap feedback echo). `Track` gained `reverb`/`delay` wet-mix fields (serde-defaulted to 0 for old JSON); both offline (`render.rs`) and real-time (`realtime.rs`) paths apply them identically post-voice-mix, pre-gain/pan, so a track sounds the same live and exported.
- `Song` gained `master_gain` (serde-defaulted to 1.0) — a live-monitoring-only output fader (documented as such: offline exports are peak-normalized afterward, which makes a uniform pre-normalize gain a no-op, so it's deliberately only applied in `realtime::EngineCore::process`, clamped to [-1, 1] to avoid harsh clipping if cranked up).
- Added real transport **seek/resume**: `engine::Command::Seek` already existed but wasn't exposed; `composer-desktop` now keeps one compiled song loaded on its audio thread across Play/Stop/Seek (an atomic `dirty` flag set by every song-mutating command tells `play`/`seek` whether to push a fresh recompile first), so dragging the design's seek bar and pressing Play actually resume from that position instead of always restarting at 0.
- Added **stem export**: `engine::render_stems` renders each track independently (through its own effects/pan, individually peak-normalized) and a new `export_stems` Tauri command writes one WAV per track — wired to the export dialog's "Export stems" checkbox. The dialog's MP3 toggle is disabled with a tooltip (no MP3 encoder in the project); "Loop region" is present but intentionally inert, matching the source design's own no-op reference implementation.
- Mute/Solo are implemented as session-only UI state (`app.js` `trackUi`, not persisted on `Track`) that recomputes each track's real `gain` via standard mixer algebra (any solo → only soloed tracks play; else muted tracks are silenced) before pushing — deliberately not added as new persisted model fields since they're mixer-session concepts, not composition data.
- 5 new tests (`effects.rs`): wet vs. dry tail energy differs, zero-send costs nothing (byte-identical to a plain render), stems render independently and error cleanly on an unknown instrument, master gain scales and clamps real-time output. **38 tests passing total.**
- Regenerated the placeholder app icons in the new terracotta accent (`#c67139`) to match.

**Next up:** `cargo tauri android init` for the real Android build (SDK/NDK already bootstrapped); Phase 7 polish after that. Consider self-hosting the Caprasimo/Figtree webfonts instead of the Google Fonts `@import` for full offline operation.
