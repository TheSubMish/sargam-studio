// Sargam desktop UI — plain JS, no build step. Talks to composer-core only
// through the Tauri commands defined in composer-desktop/src/main.rs; the
// frontend holds a local copy of the Song JSON and pushes it back with
// `set_song` after every edit (undo/redo happen server-side on that history).
//
// Visual design imported from the Claude Design "Organic" system (project
// c2fc1636-0a9a-4823-8313-a9adedc9fe63, "Sargam v3.dc.html"); tokens live in
// style.css. Layout adapts that mockup's static clip-grid sketch into a real
// track list + functional piano-roll/taal-grid editor underneath, since the
// mockup's own clip cells were empty no-op placeholders.

const invoke = window.__TAURI__.core.invoke;
const TICKS_PER_BEAT = 480;
const STEP_TICKS = TICKS_PER_BEAT / 4; // 16th-note editing grid

const STROKES = ["Bass", "Open", "Rim", "Slap", "Mute"];
const PERCUSSION_INSTRUMENTS = new Set(["Damphu", "Madal"]);
const INSTRUMENT_NE = {
  Tungna: "टुङ्ना",
  Damphu: "डम्फु",
  Madal: "मादल",
  Harmonium: "हार्मोनियम",
  Bansuri: "बाँसुरी",
};
const DEVANAGARI_DIGITS = ["०", "१", "२", "३", "४", "५", "६", "७", "८", "९"];

const PITCH_LOW = 48; // C3
const PITCH_HIGH = 84; // C6
const NOTE_NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
const PENTATONIC = new Set([9, 0, 2, 4, 7]); // A minor pentatonic, relative to A

// Onboarding's "choose your instruments" screen. The source design featured
// Sarangi here, which composer-core doesn't synthesize yet — swapped for
// Tungna (a real, playable instrument) rather than tease something silent.
const ONBOARD_INSTRUMENTS = [
  { name: "Madal", emoji: "🥁", selected: true },
  { name: "Tungna", emoji: "🪕", selected: false },
  { name: "Bansuri", emoji: "🪶", selected: false },
];
const ONBOARD_SCREEN_COUNT = 5;
let onboardScreen = 0;

let song = null;
let instruments = [];
let taals = [];
let selectedTrack = 0;
/// Ephemeral mute/solo UI state, one entry per track index; NOT persisted in
/// the Song model (mute/solo are a mixer-session concept, not composition
/// data). `baseGain` is the "real" volume the slider edits; `track.gain` sent
/// to the backend is always recomputed from base/muted/solo before pushing.
let trackUi = [];
let transportPlaying = false;
let transportBaseSecs = 0;
let transportStartWallMs = 0;
/// True while the user has the seek-bar thumb grabbed — suspends
/// tickTransport's own auto-advance so the drag isn't fighting playback.
let isDraggingSeek = false;

const el = (id) => document.getElementById(id);

let toastTimer = null;
function showToast(message, kind) {
  const toast = el("toast");
  toast.textContent = message;
  toast.className = "show " + kind;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => toast.classList.remove("show"), 4000);
}

function cssVar(name) {
  return getComputedStyle(document.body).getPropertyValue(name).trim();
}

function toDevanagari(n) {
  return String(n)
    .split("")
    .map((d) => DEVANAGARI_DIGITS[+d] ?? d)
    .join("");
}

function midiName(midi) {
  const m = Math.round(midi);
  return `${NOTE_NAMES[((m % 12) + 12) % 12]}${Math.floor(m / 12) - 1}`;
}

function isPercussion(instrumentName) {
  return PERCUSSION_INSTRUMENTS.has(instrumentName);
}

function newSong() {
  return {
    title: "Untitled",
    tempo: { changes: [{ tick: 0, bpm: 100.0 }] },
    master_gain: 1.0,
    tracks: [],
  };
}

// ------------------------------------------------------------ server sync --

async function pullSong() {
  song = await invoke("get_song");
  syncTrackUi();
  render();
}

async function pushSong() {
  await invoke("set_song", { song });
}

/// Recompute every track's effective `gain` from baseGain + mute/solo, then
/// push once. Standard mixer algebra: if anything is soloed, only soloed
/// tracks sound; otherwise muted tracks are silent, everything else plays at
/// its base volume.
async function recomputeGainsAndPush() {
  const anySolo = trackUi.some((u) => u.solo);
  song.tracks.forEach((t, i) => {
    const u = trackUi[i];
    if (!u) return;
    t.gain = anySolo ? (u.solo ? u.baseGain : 0) : u.muted ? 0 : u.baseGain;
  });
  await pushSong();
}

/// Keep `trackUi` aligned with `song.tracks`. Only resets state (assumes a
/// same-length song is the same set of tracks) when the count actually
/// changes — good enough for add/remove/load, doesn't survive undo/redo
/// perfectly across a track-count round-trip, which is an acceptable v1 gap
/// for a session-only mixer convenience.
function syncTrackUi() {
  if (trackUi.length === song.tracks.length) return;
  trackUi = song.tracks.map((t) => ({ baseGain: t.gain, muted: false, solo: false }));
}

// ---------------------------------------------------------------- tracks ---

function trackPattern(track) {
  if (track.patterns.length === 0) {
    track.patterns.push({ name: "Pattern 1", length: 8 * TICKS_PER_BEAT, notes: [] });
    track.clips.push({ pattern: 0, start: 0 });
  }
  return track.patterns[0];
}

function trackSparklineDots(track) {
  const pattern = trackPattern(track);
  const cols = Math.max(16, Math.round(pattern.length / (TICKS_PER_BEAT / 2)));
  const on = new Array(cols).fill(false);
  pattern.notes.forEach((n) => {
    const i = Math.floor((n.start / pattern.length) * cols);
    if (i >= 0 && i < cols) on[i] = true;
  });
  return on;
}

function renderTrackList() {
  const list = el("track-list");
  list.innerHTML = "";
  song.tracks.forEach((track, i) => {
    const row = document.createElement("div");
    row.className = "track-row" + (i === selectedTrack ? " selected" : "");
    const dots = trackSparklineDots(track)
      .map((on) => `<div class="dot${on ? " on" : ""}"></div>`)
      .join("");
    row.innerHTML = `
      <div class="names">
        <div class="name">${track.name}</div>
        <div class="ne">${INSTRUMENT_NE[track.instrument] ?? track.instrument}</div>
      </div>
      <div class="sparkline">${dots}</div>
      <button class="remove" title="Remove track">✕</button>`;
    row.addEventListener("click", (e) => {
      if (e.target.closest(".remove")) return;
      selectedTrack = i;
      render();
    });
    row.querySelector(".remove").addEventListener("click", async (e) => {
      e.stopPropagation();
      song.tracks.splice(i, 1);
      trackUi.splice(i, 1);
      if (selectedTrack >= song.tracks.length) selectedTrack = Math.max(0, song.tracks.length - 1);
      await pushSong();
      render();
    });
    list.appendChild(row);
  });
  el("track-count").textContent = song.tracks.length;
}

function renderInstrumentSidebar() {
  const list = el("instrument-list");
  list.innerHTML = "";
  instruments.forEach((name) => {
    const btn = document.createElement("button");
    btn.className = "instrument-btn";
    btn.innerHTML = `<span>${name}</span><span class="ne">${INSTRUMENT_NE[name] ?? ""}</span>`;
    btn.addEventListener("click", async () => {
      const existing = song.tracks.filter((t) => t.instrument === name).length;
      const trackName = existing === 0 ? name : `${name} ${existing + 1}`;
      song = await invoke("add_track", { name: trackName, instrument: name });
      selectedTrack = song.tracks.length - 1;
      syncTrackUi();
      render();
    });
    list.appendChild(btn);
  });
}

function showAddTrackForm() {
  const slot = el("add-track-slot");
  const form = document.createElement("div");
  form.className = "add-track-form";
  const options = instruments.map((n) => `<option value="${n}">${n}</option>`).join("");
  form.innerHTML = `
    <input class="input" type="text" placeholder="Track name" value="New Track" />
    <select class="input">${options}</select>
    <div class="row">
      <button class="btn btn-primary btn-block" data-action="confirm">Add</button>
      <button class="btn btn-secondary" data-action="cancel">Cancel</button>
    </div>`;
  slot.innerHTML = "";
  slot.appendChild(form);
  const nameInput = form.querySelector("input");
  const select = form.querySelector("select");
  form.querySelector('[data-action="cancel"]').addEventListener("click", () => {
    slot.innerHTML = "";
  });
  form.querySelector('[data-action="confirm"]').addEventListener("click", async () => {
    song = await invoke("add_track", { name: nameInput.value || "Track", instrument: select.value });
    selectedTrack = song.tracks.length - 1;
    syncTrackUi();
    slot.innerHTML = "";
    render();
  });
}

// ----------------------------------------------------------------- grid ----

function gridDimensions(track) {
  const pattern = trackPattern(track);
  const rows = isPercussion(track.instrument) ? STROKES.length : PITCH_HIGH - PITCH_LOW + 1;
  const cols = Math.max(1, Math.round(pattern.length / STEP_TICKS));
  return { pattern, rows, cols };
}

function rowTrigger(track, row) {
  if (isPercussion(track.instrument)) {
    return { Stroke: STROKES[row] };
  }
  return { Pitch: PITCH_HIGH - row };
}

function rowLabel(track, row) {
  return isPercussion(track.instrument) ? STROKES[row] : midiName(PITCH_HIGH - row);
}

function triggerRow(track, trigger) {
  if (trigger.Stroke !== undefined) return STROKES.indexOf(trigger.Stroke);
  if (trigger.Pitch !== undefined) return PITCH_HIGH - Math.round(trigger.Pitch);
  return -1;
}

const CELL_W = 24;
const CELL_H = 20;
const LEFT_MARGIN = 54;

function renderGrid() {
  const canvas = el("grid");
  const taalControls = el("taal-controls");

  if (song.tracks.length === 0) {
    el("editor-track-name").textContent = "No track selected — add one to start";
    taalControls.classList.add("hidden");
    canvas.width = 0;
    canvas.height = 0;
    return;
  }

  const track = song.tracks[selectedTrack];
  const { pattern, rows, cols } = gridDimensions(track);
  const percussion = isPercussion(track.instrument);

  el("editor-track-name").textContent = `${track.name} (${track.instrument})`;
  taalControls.classList.toggle("hidden", !percussion);
  el("pattern-length").value = Math.round(pattern.length / TICKS_PER_BEAT);

  const dpr = window.devicePixelRatio || 1;
  const width = LEFT_MARGIN + cols * CELL_W;
  const height = rows * CELL_H;
  canvas.width = width * dpr;
  canvas.height = height * dpr;
  canvas.style.width = width + "px";
  canvas.style.height = height + "px";
  const ctx = canvas.getContext("2d");
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, width, height);

  const bgBase = cssVar("--color-bg");
  const bgSurface = cssVar("--color-surface");
  const accentTint = cssVar("--color-accent-100");
  const gridLine = cssVar("--color-divider");
  const textColor = cssVar("--color-accent-700");
  const noteColor = percussion ? cssVar("--color-accent") : cssVar("--color-accent-2");

  for (let r = 0; r < rows; r++) {
    let bg = r % 2 === 0 ? bgBase : bgSurface;
    if (!percussion) {
      const midi = PITCH_HIGH - r;
      if (PENTATONIC.has(((midi % 12) + 12) % 12)) bg = accentTint;
    }
    ctx.fillStyle = bg;
    ctx.fillRect(LEFT_MARGIN, r * CELL_H, cols * CELL_W, CELL_H);
  }

  ctx.strokeStyle = gridLine;
  ctx.lineWidth = 1;
  for (let c = 0; c <= cols; c++) {
    const x = LEFT_MARGIN + c * CELL_W + 0.5;
    ctx.globalAlpha = c % 4 === 0 ? 1 : 0.35;
    ctx.beginPath();
    ctx.moveTo(x, 0);
    ctx.lineTo(x, height);
    ctx.stroke();
  }
  ctx.globalAlpha = 1;
  for (let r = 0; r <= rows; r++) {
    const y = r * CELL_H + 0.5;
    ctx.beginPath();
    ctx.moveTo(LEFT_MARGIN, y);
    ctx.lineTo(width, y);
    ctx.stroke();
  }

  ctx.fillStyle = textColor;
  ctx.font = "11px sans-serif";
  ctx.textBaseline = "middle";
  for (let r = 0; r < rows; r++) {
    ctx.fillText(rowLabel(track, r), 8, r * CELL_H + CELL_H / 2);
  }

  pattern.notes.forEach((note) => {
    const row = triggerRow(track, note.trigger);
    if (row < 0 || row >= rows) return;
    const x = LEFT_MARGIN + (note.start / STEP_TICKS) * CELL_W;
    const w = Math.max(4, (note.duration / STEP_TICKS) * CELL_W - 2);
    const alpha = 0.4 + 0.6 * note.velocity;
    ctx.globalAlpha = alpha;
    ctx.fillStyle = noteColor;
    roundRect(ctx, x + 1, row * CELL_H + 2, w, CELL_H - 4, 4);
    ctx.fill();
    ctx.globalAlpha = 1;
  });
}

function roundRect(ctx, x, y, w, h, r) {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}

async function onGridClick(e) {
  if (song.tracks.length === 0) return;
  const track = song.tracks[selectedTrack];
  const { pattern, rows, cols } = gridDimensions(track);
  const rect = e.target.getBoundingClientRect();
  const x = e.clientX - rect.left;
  const y = e.clientY - rect.top;
  if (x < LEFT_MARGIN) return;

  const col = Math.floor((x - LEFT_MARGIN) / CELL_W);
  const row = Math.floor(y / CELL_H);
  if (col < 0 || col >= cols || row < 0 || row >= rows) return;

  const tick = col * STEP_TICKS;
  const trigger = rowTrigger(track, row);
  const existingIdx = pattern.notes.findIndex(
    (n) => n.start === tick && JSON.stringify(n.trigger) === JSON.stringify(trigger)
  );

  if (existingIdx >= 0) {
    pattern.notes.splice(existingIdx, 1);
  } else {
    pattern.notes.push({ start: tick, duration: STEP_TICKS, trigger, velocity: 0.8, ornament: "Plain" });
    pattern.notes.sort((a, b) => a.start - b.start);
  }
  await pushSong();
  renderGrid();
  renderTrackList();
}

// -------------------------------------------------------------- inspector --

function renderInspector() {
  const panel = el("inspector");
  if (song.tracks.length === 0) {
    panel.innerHTML = `<p class="empty-inspector">Select a track to see its mixer.</p>`;
    return;
  }
  const track = song.tracks[selectedTrack];
  const ui = trackUi[selectedTrack] ?? { baseGain: track.gain, muted: false, solo: false };
  trackUi[selectedTrack] = ui;

  const volumePct = Math.round((ui.baseGain / 1.5) * 100);
  const panPct = Math.round(track.pan * 100);
  const volumeDb = ui.baseGain > 0 ? (20 * Math.log10(ui.baseGain)).toFixed(1) : "-inf";

  panel.innerHTML = `
    <h3>${track.name}</h3>
    <div class="control-group">
      <label>Volume</label>
      <input type="range" id="ins-volume" min="0" max="100" value="${volumePct}">
      <div class="readout">${volumeDb} dB</div>
    </div>
    <div class="control-group">
      <label>Pan</label>
      <input type="range" id="ins-pan" min="-100" max="100" value="${panPct}">
    </div>
    <div class="control-group">
      <label>Effects</label>
      <div class="effects-row">
        <div>
          <div class="mini-label">Reverb</div>
          <input type="range" id="ins-reverb" min="0" max="100" value="${Math.round(track.reverb * 100)}">
        </div>
        <div>
          <div class="mini-label">Delay</div>
          <input type="range" id="ins-delay" min="0" max="100" value="${Math.round(track.delay * 100)}">
        </div>
      </div>
    </div>
    <div class="toggle-row">
      <button id="ins-mute" class="btn ${ui.muted ? "btn-accent" : "btn-secondary"}">${ui.muted ? "🔇 Muted" : "🔊 Audio"}</button>
      <button id="ins-solo" class="btn ${ui.solo ? "btn-accent-2" : "btn-secondary"}">${ui.solo ? "★ Solo" : "◯ Off"}</button>
    </div>
    <div class="action-row">
      <button id="ins-loop" class="btn btn-secondary btn-block">Loop region</button>
      <button id="ins-remove" class="btn btn-secondary">×</button>
    </div>
    <div class="master-section">
      <h4>Master</h4>
      <div class="control-group">
        <label>Output level</label>
        <input type="range" id="ins-master" min="0" max="150" value="${Math.round(song.master_gain * 100)}">
      </div>
      <p class="master-hint">Live monitoring only — exported files are always peak-normalized.</p>
    </div>
  `;

  el("ins-volume").addEventListener("input", async (e) => {
    ui.baseGain = (+e.target.value / 100) * 1.5;
    await recomputeGainsAndPush();
    renderInspector();
  });
  el("ins-pan").addEventListener("input", async (e) => {
    track.pan = +e.target.value / 100;
    await pushSong();
  });
  el("ins-reverb").addEventListener("input", async (e) => {
    track.reverb = +e.target.value / 100;
    await pushSong();
  });
  el("ins-delay").addEventListener("input", async (e) => {
    track.delay = +e.target.value / 100;
    await pushSong();
  });
  el("ins-mute").addEventListener("click", async () => {
    ui.muted = !ui.muted;
    await recomputeGainsAndPush();
    renderInspector();
  });
  el("ins-solo").addEventListener("click", async () => {
    ui.solo = !ui.solo;
    await recomputeGainsAndPush();
    renderInspector();
  });
  el("ins-loop").addEventListener("click", () => {
    // Matches the source design exactly: a present but inert control (its own
    // reference implementation's loopRegion handler is a no-op too).
  });
  el("ins-remove").addEventListener("click", async () => {
    song.tracks.splice(selectedTrack, 1);
    trackUi.splice(selectedTrack, 1);
    selectedTrack = Math.max(0, selectedTrack - 1);
    await pushSong();
    render();
  });
  el("ins-master").addEventListener("input", async (e) => {
    song.master_gain = +e.target.value / 100;
    await pushSong();
  });
}

// ------------------------------------------------------------- transport ---

function computeSongDurationSecs() {
  const bpm = song.tempo.changes[0]?.bpm || 100;
  let maxTick = 0;
  for (const track of song.tracks) {
    for (const clip of track.clips) {
      const pattern = track.patterns[clip.pattern];
      if (!pattern) continue;
      maxTick = Math.max(maxTick, clip.start + pattern.length);
    }
  }
  return (maxTick / TICKS_PER_BEAT) * (60 / bpm);
}

function formatTime(secs) {
  const m = Math.floor(secs / 60);
  const s = Math.floor(secs % 60);
  return `${m}:${String(s).padStart(2, "0")}`;
}

function updateTransportDisplay(nowSecs, totalSecs) {
  el("time-now").textContent = formatTime(nowSecs);
  el("time-total").textContent = formatTime(totalSecs);
  const pct = totalSecs > 0 ? Math.min(100, (nowSecs / totalSecs) * 100) : 0;
  el("seek-fill").style.width = pct + "%";
  el("seek-thumb").style.left = pct + "%";
}

function tickTransport() {
  if (isDraggingSeek) {
    requestAnimationFrame(tickTransport);
    return;
  }
  const totalSecs = computeSongDurationSecs();
  if (transportPlaying) {
    const elapsed = (performance.now() - transportStartWallMs) / 1000;
    const pos = Math.min(transportBaseSecs + elapsed, totalSecs);
    updateTransportDisplay(pos, totalSecs);
    if (pos >= totalSecs) {
      // Mirror the engine: a natural finish leaves position at the end (not
      // rewound) so the next Play click's "were we at the end?" check below
      // knows to replay from the top instead of just resuming into silence.
      transportBaseSecs = totalSecs;
      transportPlaying = false;
      el("play").classList.remove("playing");
      el("play").textContent = "▶";
    }
  } else {
    updateTransportDisplay(transportBaseSecs, totalSecs);
  }
  requestAnimationFrame(tickTransport);
}

/// Freeze the displayed position at "now" — used by Pause, where (unlike
/// Stop) we keep the position instead of rewinding to 0.
function currentDisplayedPositionSecs() {
  if (!transportPlaying) return transportBaseSecs;
  const totalSecs = computeSongDurationSecs();
  const elapsed = (performance.now() - transportStartWallMs) / 1000;
  return Math.min(transportBaseSecs + elapsed, totalSecs);
}

async function startPlayback() {
  if (song.tracks.length === 0) return;
  // Replaying after a natural finish (or an explicit seek to the very end)
  // must restart from the top — the engine does the equivalent rewind
  // server-side; mirror it here so the UI's seek bar doesn't stay pinned at
  // 100% while audio actually plays from 0.
  if (transportBaseSecs >= computeSongDurationSecs()) transportBaseSecs = 0;
  await invoke("play");
  transportPlaying = true;
  transportStartWallMs = performance.now();
  el("play").classList.add("playing");
  el("play").textContent = "⏸";
}

async function pausePlayback() {
  transportBaseSecs = currentDisplayedPositionSecs();
  await invoke("pause");
  transportPlaying = false;
  el("play").classList.remove("playing");
  el("play").textContent = "▶";
}

async function stopPlayback() {
  await invoke("stop");
  transportPlaying = false;
  transportBaseSecs = 0;
  el("play").classList.remove("playing");
  el("play").textContent = "▶";
}

function seekBarFracFromEvent(e) {
  const rect = el("seek-bar").getBoundingClientRect();
  return Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width));
}

/// A real scrubber: grab the thumb (or click anywhere on the bar) and drag —
/// the displayed position follows the pointer live, and the actual `seek`
/// only fires once on release, so dragging back a few seconds and letting go
/// resumes playback from exactly there instead of flooding the audio thread
/// with a seek per pixel of mouse movement.
function setupSeekBarDrag() {
  const bar = el("seek-bar");
  let pendingSecs = 0;

  const preview = (e) => {
    const totalSecs = computeSongDurationSecs();
    pendingSecs = seekBarFracFromEvent(e) * totalSecs;
    updateTransportDisplay(pendingSecs, totalSecs);
  };

  const onMove = (e) => preview(e);

  const onUp = async () => {
    if (!isDraggingSeek) return;
    isDraggingSeek = false;
    bar.classList.remove("dragging");
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerup", onUp);
    await invoke("seek", { positionSecs: pendingSecs });
    transportBaseSecs = pendingSecs;
    transportStartWallMs = performance.now();
  };

  bar.addEventListener("pointerdown", (e) => {
    isDraggingSeek = true;
    bar.classList.add("dragging");
    preview(e);
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  });
}

function renderTaalPills() {
  const container = el("taal-pills");
  container.innerHTML = "";
  taals.forEach((t) => {
    const btn = document.createElement("button");
    btn.className = "taal-pill";
    btn.textContent = `${t.name} ${toDevanagari(t.beats_per_cycle)}`;
    btn.addEventListener("click", async () => {
      container.querySelectorAll(".taal-pill").forEach((b) => b.classList.remove("active"));
      btn.classList.add("active");
      if (song.tracks.length === 0) return;
      song = await invoke("apply_taal", { track: selectedTrack, taalName: t.name, cycles: 4 });
      syncTrackUi();
      render();
    });
    container.appendChild(btn);
  });
}

// ------------------------------------------------------------- onboarding --

function renderOnboardInstrumentGrid() {
  const grid = el("ob-instrument-grid");
  grid.innerHTML = "";
  ONBOARD_INSTRUMENTS.forEach((inst) => {
    const card = document.createElement("button");
    card.className = "ob-instrument-card" + (inst.selected ? " selected" : "");
    card.innerHTML = `
      <div class="emoji">${inst.emoji}</div>
      <div class="name">${inst.name}</div>
      <div class="ne">${INSTRUMENT_NE[inst.name] ?? ""}</div>`;
    card.addEventListener("click", () => {
      inst.selected = !inst.selected;
      renderOnboardInstrumentGrid();
    });
    grid.appendChild(card);
  });
}

function showOnboardScreen(n) {
  onboardScreen = Math.max(0, Math.min(ONBOARD_SCREEN_COUNT - 1, n));
  for (let i = 0; i < ONBOARD_SCREEN_COUNT; i++) {
    el(`ob-screen-${i}`).classList.toggle("hidden", i !== onboardScreen);
  }
  const dots = el("ob-dots");
  dots.innerHTML = "";
  for (let i = 0; i < ONBOARD_SCREEN_COUNT; i++) {
    const dot = document.createElement("span");
    dot.className = "ob-dot" + (i === onboardScreen ? " active" : "");
    dots.appendChild(dot);
  }
}

async function startOnboarding() {
  // Covers both first-launch and "Replay onboarding" — the latter is
  // reachable while a song is actively playing, and letting it keep sounding
  // underneath the onboarding overlay is confusing (and easy to lose track
  // of, since the overlay covers the transport controls that would stop it).
  await stopPlayback();
  showOnboardScreen(0);
  renderOnboardInstrumentGrid();
  el("onboarding").classList.remove("hidden");
}

async function finishOnboarding() {
  localStorage.setItem("sargam-launched", "true");
  el("onboarding").classList.add("hidden");
  const chosen = ONBOARD_INSTRUMENTS.filter((i) => i.selected);
  for (const inst of chosen) {
    const existing = song.tracks.filter((t) => t.instrument === inst.name).length;
    const trackName = existing === 0 ? inst.name : `${inst.name} ${existing + 1}`;
    song = await invoke("add_track", { name: trackName, instrument: inst.name });
  }
  selectedTrack = Math.max(0, song.tracks.length - 1);
  syncTrackUi();
  render();
}

function setupOnboardingEvents() {
  el("onboarding").addEventListener("click", (e) => {
    if (e.target.hasAttribute("data-next")) showOnboardScreen(onboardScreen + 1);
    else if (e.target.hasAttribute("data-prev")) showOnboardScreen(onboardScreen - 1);
  });
  el("ob-finish").addEventListener("click", finishOnboarding);
  el("replay-onboarding").addEventListener("click", () => {
    el("file-menu-panel").classList.add("hidden");
    startOnboarding();
  });
}

// ------------------------------------------------------------- top level ---

function render() {
  renderTrackList();
  renderGrid();
  renderInspector();
  el("song-title").value = song.title;
  el("tempo-value").textContent = Math.round(song.tempo.changes[0]?.bpm ?? 100);
}

function populateTaalSelect() {
  el("taal-select").innerHTML = taals.map((t) => `<option value="${t.name}">${t.name}</option>`).join("");
}

async function setupEvents() {
  el("file-menu-btn").addEventListener("click", () => {
    el("file-menu-panel").classList.toggle("hidden");
  });
  document.addEventListener("click", (e) => {
    if (!e.target.closest(".file-menu")) el("file-menu-panel").classList.add("hidden");
  });

  el("add-track-custom").addEventListener("click", showAddTrackForm);

  el("song-title").addEventListener("change", async (e) => {
    song.title = e.target.value;
    await pushSong();
  });
  el("tempo-up").addEventListener("click", async () => {
    song.tempo.changes[0].bpm = Math.min(300, (song.tempo.changes[0].bpm || 100) + 4);
    el("tempo-value").textContent = Math.round(song.tempo.changes[0].bpm);
    await pushSong();
  });
  el("tempo-down").addEventListener("click", async () => {
    song.tempo.changes[0].bpm = Math.max(20, (song.tempo.changes[0].bpm || 100) - 4);
    el("tempo-value").textContent = Math.round(song.tempo.changes[0].bpm);
    await pushSong();
  });

  el("pattern-length").addEventListener("change", async (e) => {
    if (song.tracks.length === 0) return;
    const track = song.tracks[selectedTrack];
    const beats = Math.max(1, parseInt(e.target.value, 10) || 8);
    trackPattern(track).length = beats * TICKS_PER_BEAT;
    await pushSong();
    renderGrid();
    renderTrackList();
  });

  el("apply-taal").addEventListener("click", async () => {
    if (song.tracks.length === 0) return;
    const taalName = el("taal-select").value;
    const cycles = Math.max(1, parseInt(el("taal-cycles").value, 10) || 4);
    song = await invoke("apply_taal", { track: selectedTrack, taalName, cycles });
    syncTrackUi();
    render();
  });

  el("grid").addEventListener("click", onGridClick);

  el("play").addEventListener("click", async () => {
    if (transportPlaying) await pausePlayback();
    else await startPlayback();
  });
  el("stop").addEventListener("click", stopPlayback);
  setupSeekBarDrag();

  el("undo").addEventListener("click", async () => {
    song = await invoke("undo");
    syncTrackUi();
    render();
  });
  el("redo").addEventListener("click", async () => {
    song = await invoke("redo");
    syncTrackUi();
    render();
  });

  el("new-song").addEventListener("click", async () => {
    await stopPlayback(); // otherwise the old song keeps sounding underneath the new one
    song = newSong();
    selectedTrack = 0;
    trackUi = [];
    await pushSong();
    render();
  });

  el("save-song").addEventListener("click", async () => {
    try {
      const path = await invoke("plugin:dialog|save", {
        options: {
          defaultPath: `${song.title}.song.json`,
          filters: [{ name: "Sargam Song", extensions: ["json"] }],
        },
      });
      if (!path) return;
      await invoke("save_song", { path });
      showToast(`Saved to ${path}`, "ok");
    } catch (err) {
      showToast(`Save failed: ${err}`, "err");
    }
  });

  el("load-song").addEventListener("click", async () => {
    try {
      const path = await invoke("plugin:dialog|open", {
        options: {
          multiple: false,
          filters: [{ name: "Sargam Song", extensions: ["json"] }],
        },
      });
      if (!path) return;
      await stopPlayback(); // otherwise the old song keeps sounding underneath the loaded one
      song = await invoke("load_song", { path });
      selectedTrack = 0;
      trackUi = [];
      syncTrackUi();
      render();
      showToast(`Opened ${path}`, "ok");
    } catch (err) {
      showToast(`Open failed: ${err}`, "err");
    }
  });

  el("export-midi").addEventListener("click", async () => {
    try {
      const path = await invoke("plugin:dialog|save", {
        options: {
          defaultPath: `${song.title}.mid`,
          filters: [{ name: "Standard MIDI File", extensions: ["mid"] }],
        },
      });
      if (!path) return;
      await invoke("export_midi_file", { path });
      showToast(`Exported to ${path}`, "ok");
    } catch (err) {
      showToast(`MIDI export failed: ${err}`, "err");
    }
  });

  // — export dialog —
  el("open-export").addEventListener("click", () => {
    el("export-name").value = song.title || "composition";
    el("export-backdrop").classList.remove("hidden");
  });
  el("export-cancel").addEventListener("click", () => el("export-backdrop").classList.add("hidden"));
  el("export-backdrop").addEventListener("click", (e) => {
    if (e.target.id === "export-backdrop") el("export-backdrop").classList.add("hidden");
  });
  el("export-do").addEventListener("click", async () => {
    try {
      const path = await invoke("plugin:dialog|save", {
        options: {
          defaultPath: `${el("export-name").value || "composition"}.wav`,
          filters: [{ name: "WAV Audio", extensions: ["wav"] }],
        },
      });
      if (!path) return;
      await invoke("export_wav", { path });
      let message = `Exported to ${path}`;
      if (el("export-stems").checked) {
        const stemsDir = path.replace(/\.wav$/i, "") + "_stems";
        await invoke("export_stems", { dir: stemsDir });
        message += ` (stems in ${stemsDir})`;
      }
      el("export-backdrop").classList.add("hidden");
      showToast(message, "ok");
    } catch (err) {
      showToast(`Export failed: ${err}`, "err");
    }
  });
}

async function main() {
  instruments = await invoke("list_instruments");
  taals = await invoke("list_taals");
  populateTaalSelect();
  renderInstrumentSidebar();
  renderTaalPills();
  await setupEvents();
  setupOnboardingEvents();
  await pullSong();
  requestAnimationFrame(tickTransport);

  if (!localStorage.getItem("sargam-launched")) {
    startOnboarding();
  }
}

main();
