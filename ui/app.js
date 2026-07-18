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

let currentPage = "compose"; // "compose" | "projects"
let settings = null; // AppSettings from Rust; loaded once at startup
let projects = [];
let projectSort = "recent";
let projectSearch = "";
/// Path of the currently open project file, if it was saved/opened/created
/// via a real path (not a fresh unsaved song) — lets autosave and Projects
/// know where "here" is without a second source of truth.
let currentSongPath = null;
let autoSaveTimer = null;

// Marketplace: Explore/Wishlist/Upload/Earnings/Payment — see the "marketplace"
// section below for the bulk of this feature's logic.
const MARKETPLACE_GENRES = ["Orchestral", "Electronic", "Hip-hop", "Lo-fi", "Ambient", "Rock", "Jazz", "Folk"];
let exploreFilters = { price: "all", genre: null, sort: "recent", search: "", view: "grid" };
let exploreTracksCache = [];
let exploreWishlistIds = new Set();
let selectedExploreTrack = null;
let wishlistTracksCache = [];
let uploadState = { step: 1, projectPath: null, title: "", description: "", genre: "", pricing: "free", priceCents: 0 };
let stripe = null;
let stripeElements = null;
let paymentTrack = null;
let paymentPurchaseId = null;

const el = (id) => document.getElementById(id);

function debounce(fn, ms) {
  let timer;
  return (...args) => {
    clearTimeout(timer);
    timer = setTimeout(() => fn(...args), ms);
  };
}

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

const ZOOM_PRESETS = {
  compact: { w: 18, h: 16 },
  normal: { w: 24, h: 20 },
  expanded: { w: 32, h: 26 },
};
let CELL_W = 24;
let CELL_H = 20;
function applyTimelineZoom(zoom) {
  const preset = ZOOM_PRESETS[zoom] || ZOOM_PRESETS.normal;
  CELL_W = preset.w;
  CELL_H = preset.h;
}
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

// ---------------------------------------------------------------- routing --
//
// Only one persistent surface (the compose UI in #app) plus two overlays:
// Projects is a full-page overlay (own header, "← Back" returns to compose);
// Settings is a slide-out drawer over whatever's currently showing — both
// match "Sargam Main.dc.html", which has no separate nav row at all.

function showProjectsPage() {
  currentPage = "projects";
  el("app").classList.add("hidden");
  el("page-projects").classList.remove("hidden");
  loadProjects();
}

function hideProjectsPage() {
  currentPage = "compose";
  el("page-projects").classList.add("hidden");
  el("app").classList.remove("hidden");
}

function showExplorePage() {
  currentPage = "explore";
  el("app").classList.add("hidden");
  el("page-explore").classList.remove("hidden");
  loadExploreTracks();
}
function hideExplorePage() {
  currentPage = "compose";
  el("page-explore").classList.add("hidden");
  el("app").classList.remove("hidden");
}

function showWishlistPage() {
  currentPage = "wishlist";
  el("app").classList.add("hidden");
  el("page-wishlist").classList.remove("hidden");
  loadWishlistTracks();
}
function hideWishlistPage() {
  currentPage = "compose";
  el("page-wishlist").classList.add("hidden");
  el("app").classList.remove("hidden");
}

function showUploadPage() {
  currentPage = "upload";
  el("app").classList.add("hidden");
  el("page-upload").classList.remove("hidden");
}
function hideUploadPage() {
  currentPage = "compose";
  el("page-upload").classList.add("hidden");
  el("app").classList.remove("hidden");
}

function showEarningsPage() {
  currentPage = "earnings";
  el("app").classList.add("hidden");
  el("page-earnings").classList.remove("hidden");
  loadEarnings();
}
function hideEarningsPage() {
  currentPage = "compose";
  el("page-earnings").classList.add("hidden");
  el("app").classList.remove("hidden");
}

/// Explore/Wishlist/Upload/Earnings all require being signed in (product
/// decision — other people's tracks and your own sales aren't visible to a
/// logged-out session). Prompts the existing auth modal instead of proceeding
/// when there's no session yet.
function requireAuth(next) {
  if (!authStatus.isAuthenticated) {
    showToast("Log in to do that", "err");
    openAuthModal();
    return;
  }
  next();
}

function openSettingsDrawer() {
  renderDrawerContent();
  el("settings-drawer").classList.add("open");
}

function closeSettingsDrawer() {
  el("settings-drawer").classList.remove("open");
}

function setupNav() {
  el("open-projects").addEventListener("click", showProjectsPage);
  el("projects-back").addEventListener("click", hideProjectsPage);
  el("open-settings-drawer").addEventListener("click", openSettingsDrawer);
  el("close-settings-drawer").addEventListener("click", closeSettingsDrawer);

  el("open-explore").addEventListener("click", () => requireAuth(showExplorePage));
  el("open-wishlist").addEventListener("click", () => requireAuth(showWishlistPage));
  el("explore-back").addEventListener("click", hideExplorePage);
  el("wishlist-back").addEventListener("click", hideWishlistPage);
  el("upload-back").addEventListener("click", hideUploadPage);
  el("earnings-back").addEventListener("click", hideEarningsPage);
}

// ------------------------------------------------------------------ auth ---
//
// Real Supabase Auth: auth.rs calls Supabase's GoTrue REST API over HTTPS
// to sign up/log in/log out. Projects (.song.json files) are unaffected and
// stay local — this only covers the account/sign-in layer.

let authStatus = { hasAccount: false, isAuthenticated: false, name: "", email: "" };

function authInitials() {
  return authStatus.name
    .split(/\s+/)
    .slice(0, 2)
    .map((w) => w[0]?.toUpperCase() ?? "")
    .join("") || "?";
}

async function refreshAuthStatus() {
  authStatus = await invoke("get_auth_status");
}

function openAuthModal() {
  closeSettingsDrawer(); // the login modal shouldn't sit on top of an open drawer
  el("auth-error").classList.add("hidden");
  el("auth-login-email").value = "";
  el("auth-login-password").value = "";
  el("auth-signup-name").value = "";
  el("auth-signup-email").value = "";
  el("auth-signup-password").value = "";
  const startOnSignup = !authStatus.hasAccount;
  setAuthMode(startOnSignup ? "signup" : "login");
  el("auth-backdrop").classList.remove("hidden");
}

function setAuthMode(mode) {
  el("auth-title").textContent = mode === "login" ? "Log in" : "Sign up";
  el("auth-login-fields").classList.toggle("hidden", mode !== "login");
  el("auth-signup-fields").classList.toggle("hidden", mode !== "signup");
  el("auth-error").classList.add("hidden");
}

function showAuthError(message) {
  const box = el("auth-error");
  box.textContent = message;
  box.classList.remove("hidden");
}

function setupAuthModal() {
  el("auth-cancel").addEventListener("click", () => el("auth-backdrop").classList.add("hidden"));
  el("auth-backdrop").addEventListener("click", (e) => {
    if (e.target.id === "auth-backdrop") el("auth-backdrop").classList.add("hidden");
  });
  el("auth-to-signup").addEventListener("click", () => setAuthMode("signup"));
  el("auth-to-login").addEventListener("click", () => setAuthMode("login"));

  el("auth-do-login").addEventListener("click", async () => {
    try {
      authStatus = await invoke("login", {
        email: el("auth-login-email").value,
        password: el("auth-login-password").value,
      });
      el("auth-backdrop").classList.add("hidden");
      renderDrawerContent();
      showToast(`Logged in as ${authStatus.name}`, "ok");
    } catch (err) {
      showAuthError(String(err));
    }
  });

  el("auth-do-signup").addEventListener("click", async () => {
    try {
      authStatus = await invoke("signup", {
        name: el("auth-signup-name").value,
        email: el("auth-signup-email").value,
        password: el("auth-signup-password").value,
      });
      el("auth-backdrop").classList.add("hidden");
      renderDrawerContent();
      showToast(`Account created — welcome, ${authStatus.name}`, "ok");
    } catch (err) {
      showAuthError(String(err));
    }
  });
}

async function doLogout() {
  if (!confirm("Log out?")) return;
  authStatus = await invoke("logout");
  renderDrawerContent();
}

// -------------------------------------------------------------- projects ---

function formatRelativeTime(epochSecs) {
  const diff = Date.now() / 1000 - epochSecs;
  if (diff < 60) return "just now";
  if (diff < 3600) return `${Math.floor(diff / 60)} min ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} hour${diff < 7200 ? "" : "s"} ago`;
  if (diff < 172800) return "yesterday";
  if (diff < 604800) return `${Math.floor(diff / 86400)} days ago`;
  return new Date(epochSecs * 1000).toLocaleDateString();
}

async function loadProjects() {
  try {
    projects = await invoke("list_projects", { dir: settings.projectsDir });
  } catch (err) {
    projects = [];
    showToast(`Couldn't read projects folder: ${err}`, "err");
  }
  renderProjects();
}

function renderProjects() {
  const grid = el("project-grid");
  const empty = el("empty-projects");

  if (projects.length === 0) {
    grid.innerHTML = "";
    empty.classList.remove("hidden");
    return;
  }
  empty.classList.add("hidden");

  let list = projects.filter((p) =>
    p.title.toLowerCase().includes(projectSearch.toLowerCase())
  );
  if (projectSort === "name") {
    list = list.slice().sort((a, b) => a.title.localeCompare(b.title));
  } else if (projectSort === "duration") {
    list = list.slice().sort((a, b) => b.durationSecs - a.durationSecs);
  } else {
    list = list.slice().sort((a, b) => b.modifiedEpochSecs - a.modifiedEpochSecs);
  }

  grid.innerHTML = "";
  list.forEach((p) => {
    const card = document.createElement("div");
    card.className = "project-card";
    card.innerHTML = `
      <div class="project-card-art">🎵</div>
      <h3 class="project-card-title">${p.title}</h3>
      <div class="project-card-meta">
        <div>${p.trackCount} track${p.trackCount === 1 ? "" : "s"}</div>
        <div>${formatTime(p.durationSecs)}</div>
        <div>${formatRelativeTime(p.modifiedEpochSecs)}</div>
      </div>
      <button class="btn btn-ghost project-card-upload" style="width:100%;margin-bottom:6px;border-radius:6px;padding:6px;font-size:12px">Upload</button>
      <button class="project-card-remove" title="Delete">×</button>`;
    card.addEventListener("click", async () => {
      try {
        await stopPlayback();
        song = await invoke("load_song", { path: p.path });
        currentSongPath = p.path;
        selectedTrack = 0;
        trackUi = [];
        syncTrackUi();
        render();
        hideProjectsPage();
      } catch (err) {
        showToast(`Couldn't open project: ${err}`, "err");
      }
    });
    card.querySelector(".project-card-remove").addEventListener("click", async (e) => {
      e.stopPropagation();
      if (!confirm(`Delete "${p.title}"? This can't be undone.`)) return;
      try {
        await invoke("delete_project", { path: p.path });
        await loadProjects();
        showToast(`Deleted ${p.title}`, "ok");
      } catch (err) {
        showToast(`Delete failed: ${err}`, "err");
      }
    });
    card.querySelector(".project-card-upload").addEventListener("click", (e) => {
      e.stopPropagation();
      requireAuth(() => openUploadPage(p));
    });
    grid.appendChild(card);
  });
}

async function createNewProject() {
  const title = prompt("Project name", "Untitled");
  if (title === null) return;
  try {
    const path = await invoke("new_project", { dir: settings.projectsDir, title });
    song = await invoke("load_song", { path });
    currentSongPath = path;
    selectedTrack = 0;
    trackUi = [];
    syncTrackUi();
    render();
    hideProjectsPage();
    showToast(`Created ${title}`, "ok");
  } catch (err) {
    showToast(`Couldn't create project: ${err}`, "err");
  }
}

function setupProjectsPage() {
  el("new-project-btn").addEventListener("click", createNewProject);
  el("empty-new-project").addEventListener("click", createNewProject);
  document.querySelectorAll(".sort-btn").forEach((btn) => {
    btn.addEventListener("click", () => {
      projectSort = btn.dataset.sort;
      document.querySelectorAll(".sort-btn").forEach((b) => b.classList.toggle("active", b === btn));
      renderProjects();
    });
  });
  el("project-search").addEventListener("input", (e) => {
    projectSearch = e.target.value;
    renderProjects();
  });
}

// --------------------------------------------------------------- settings --

async function saveSettings() {
  await invoke("set_settings", { settings });
}

/// Flat, un-tabbed sections (Profile → Workspace → Audio → Export → About) —
/// matches "Sargam Main.dc.html"'s drawer exactly, no sidebar tab nav. Only
/// controls with something real behind them are here (see main.rs); the
/// language/theme pickers keep their non-English/non-light options visibly
/// disabled ("coming soon"), the same honesty pattern the source design uses
/// for its own "Dark (coming soon)" theme option, rather than silently doing
/// nothing.
function drawerContentHtml() {
  const s = settings;
  const profileHtml = authStatus.isAuthenticated
    ? `<div class="profile-card">
         <div class="profile-avatar">${authInitials()}</div>
         <div class="profile-info">
           <div class="profile-name">${authStatus.name}</div>
           <div class="profile-email">${authStatus.email}</div>
         </div>
         <button class="icon-btn" id="drawer-earnings" title="Earnings">💰</button>
         <button class="icon-btn" id="drawer-logout" title="Log out">⌄</button>
       </div>`
    : `<div class="profile-login-prompt">
         <p class="profile-hint" style="margin-bottom:8px">Not logged in. Sign in to sync your account — your projects stay on this device either way.</p>
         <button class="btn btn-primary btn-block" id="drawer-login" style="border-radius:999px">Log in / Sign up</button>
       </div>`;

  return `
    <div class="settings-section">${profileHtml}</div>

    <div class="settings-section">
      <h3>Workspace</h3>
      <div class="settings-item">
        <label>Language</label>
        <select id="set-language">
          <option value="en">English</option>
          <option value="ne" disabled>नेपाली (coming soon)</option>
        </select>
      </div>
      <div class="settings-item">
        <label>Theme</label>
        <select id="set-theme">
          <option value="light">Light</option>
          <option value="dark" disabled>Dark (soon)</option>
        </select>
      </div>
      <div class="settings-item">
        <div><div class="settings-item-label">Timeline zoom</div><div class="settings-item-note">Grid cell size in the note editor</div></div>
        <select id="set-zoom">
          <option value="compact">Compact</option>
          <option value="normal">Normal</option>
          <option value="expanded">Expanded</option>
        </select>
      </div>
      <label class="settings-check-label"><input type="checkbox" id="set-autosave" /> Auto-save every 30 seconds</label>
    </div>

    <div class="settings-section">
      <h3>Audio Quality</h3>
      <div class="settings-item">
        <label>Sample rate</label>
        <select id="set-samplerate">
          <option value="44100">44.1 kHz</option>
          <option value="48000">48 kHz</option>
          <option value="96000">96 kHz</option>
        </select>
      </div>
      <div class="settings-item">
        <label>Bit depth</label>
        <select id="set-bitdepth">
          <option value="16">16-bit</option>
          <option value="24">24-bit</option>
          <option value="32">32-bit float</option>
        </select>
      </div>
      <div class="settings-item">
        <div><div class="settings-item-label">Buffer size</div><div class="settings-item-note">Lower = less latency, more risk of crackle</div></div>
        <select id="set-buffersize">
          <option value="128">128 samples</option>
          <option value="256">256 samples</option>
          <option value="512">512 samples</option>
          <option value="1024">1024 samples</option>
        </select>
      </div>
    </div>

    <div class="settings-section">
      <h3>Export Defaults</h3>
      <div class="settings-item">
        <label>Format</label>
        <select disabled title="MP3/FLAC coming soon — no encoder yet">
          <option>WAV</option>
          <option disabled>MP3 (soon)</option>
          <option disabled>FLAC (soon)</option>
        </select>
      </div>
      <div class="settings-item">
        <div class="settings-item-label">Project folder</div>
      </div>
      <div class="settings-item">
        <span style="font-size:12px;opacity:0.75;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;flex:1" title="${s.projectsDir}">${s.projectsDir}</span>
        <button class="btn btn-secondary" id="set-projects-dir" style="border-radius:999px;padding:5px 12px;font-size:12px">Browse…</button>
      </div>
      <label class="settings-check-label"><input type="checkbox" id="set-stems-default" /> Export stems by default</label>
      <p class="settings-item-note" style="margin-top:8px">Exports are always peak-normalized.</p>
    </div>

    <div class="settings-section">
      <h3>About</h3>
      <p style="font-size:12px;opacity:0.7;line-height:1.6;margin:0"><strong>Sargam v1.0</strong><br>लोक बाजा स्टुडियो · Compose Nepali folk music for games and projects</p>
    </div>`;
}

function renderDrawerContent() {
  if (!settings) return;
  el("drawer-content").innerHTML = drawerContentHtml();

  if (authStatus.isAuthenticated) {
    el("drawer-logout").addEventListener("click", doLogout);
    el("drawer-earnings").addEventListener("click", () => {
      closeSettingsDrawer();
      requireAuth(showEarningsPage);
    });
  } else {
    el("drawer-login").addEventListener("click", openAuthModal);
  }

  el("set-language").value = settings.language;
  el("set-language").addEventListener("change", async (e) => {
    settings.language = e.target.value;
    await saveSettings();
  });
  el("set-theme").value = settings.theme;
  el("set-theme").addEventListener("change", async (e) => {
    settings.theme = e.target.value;
    await saveSettings();
  });
  el("set-zoom").value = settings.timelineZoom;
  el("set-zoom").addEventListener("change", async (e) => {
    settings.timelineZoom = e.target.value;
    applyTimelineZoom(settings.timelineZoom);
    renderGrid();
    await saveSettings();
  });
  el("set-autosave").checked = settings.autoSave;
  el("set-autosave").addEventListener("change", async (e) => {
    settings.autoSave = e.target.checked;
    setupAutoSave();
    await saveSettings();
  });

  el("set-samplerate").value = String(settings.sampleRate);
  el("set-samplerate").addEventListener("change", async (e) => {
    settings.sampleRate = +e.target.value;
    await saveSettings();
  });
  el("set-bitdepth").value = settings.bitDepth;
  el("set-bitdepth").addEventListener("change", async (e) => {
    settings.bitDepth = e.target.value;
    await saveSettings();
  });
  el("set-buffersize").value = String(settings.bufferSize);
  el("set-buffersize").addEventListener("change", async (e) => {
    settings.bufferSize = +e.target.value;
    await invoke("set_buffer_size", { frames: settings.bufferSize });
    await saveSettings();
  });

  el("set-stems-default").checked = settings.exportStemsDefault;
  el("set-stems-default").addEventListener("change", async (e) => {
    settings.exportStemsDefault = e.target.checked;
    await saveSettings();
  });
  el("set-projects-dir").addEventListener("click", async () => {
    const dir = await invoke("plugin:dialog|open", {
      options: { directory: true, defaultPath: settings.projectsDir },
    });
    if (dir) {
      settings.projectsDir = dir;
      await saveSettings();
      renderDrawerContent();
    }
  });
}

function setupAutoSave() {
  clearInterval(autoSaveTimer);
  if (!settings.autoSave) return;
  autoSaveTimer = setInterval(async () => {
    if (!currentSongPath) return; // nowhere to autosave an unsaved project to
    try {
      await invoke("save_song", { path: currentSongPath });
    } catch {
      // Stay quiet — a failed background autosave shouldn't interrupt composing.
    }
  }, 30_000);
}

// -------------------------------------------------------------- marketplace --
//
// Explore/Wishlist/Upload/Earnings/Payment. Talks to the Rust commands in
// marketplace.rs, which in turn talk to Supabase (PostgREST + Storage + two
// Edge Functions for Stripe) — see CLAUDE.md's Supabase section and
// supabase/migrations/0001_marketplace.sql for the server side of this.

function trackPriceTag(track) {
  return track.priceCents > 0
    ? `<span class="tag price">$${(track.priceCents / 100).toFixed(2)}</span>`
    : `<span class="tag price">Free</span>`;
}

function trackCardHtml(track) {
  const saved = exploreWishlistIds.has(track.id);
  return `
    <div class="track-card" data-id="${track.id}">
      <div class="track-thumb" style="background:linear-gradient(135deg, ${track.gradientStart} 0%, ${track.gradientEnd} 100%)">${track.emoji}</div>
      <div class="track-body">
        <div class="track-title">${track.title}</div>
        <div class="track-creator">${track.creatorName}</div>
        <div class="track-meta"><span class="tag">${track.genre || "—"}</span>${trackPriceTag(track)}</div>
        <div class="track-stats">${track.ratingCount > 0 ? "★ " + track.ratingAverage.toFixed(1) : "No ratings yet"} · ${track.downloadsCount} DL</div>
        <button class="track-action wishlist-toggle" data-id="${track.id}">${saved ? "♥ Saved" : "♡ Save"}</button>
      </div>
    </div>`;
}

async function handleWishlistToggle(trackId) {
  try {
    const saved = await invoke("toggle_wishlist", { trackId });
    if (saved) exploreWishlistIds.add(trackId);
    else exploreWishlistIds.delete(trackId);
    return saved;
  } catch (err) {
    showToast(`Couldn't update wishlist: ${err}`, "err");
    return exploreWishlistIds.has(trackId);
  }
}

// ---- Explore ----

function renderExploreGenreList() {
  const box = el("explore-genre-list");
  box.innerHTML = MARKETPLACE_GENRES.map(
    (g) => `<button class="filter-btn ${exploreFilters.genre === g ? "active" : ""}" data-genre="${g}">${g}</button>`
  ).join("");
  box.querySelectorAll("button").forEach((btn) => {
    btn.addEventListener("click", () => {
      exploreFilters.genre = exploreFilters.genre === btn.dataset.genre ? null : btn.dataset.genre;
      renderExploreGenreList();
      loadExploreTracks();
    });
  });
}

function renderExploreTracks() {
  const grid = el("explore-tracks");
  grid.classList.toggle("list-view", exploreFilters.view === "list");
  if (exploreTracksCache.length === 0) {
    grid.innerHTML = `<p style="opacity:0.7">No tracks match these filters.</p>`;
    return;
  }
  grid.innerHTML = exploreTracksCache.map(trackCardHtml).join("");
  grid.querySelectorAll(".track-card").forEach((card) => {
    card.addEventListener("click", (e) => {
      if (e.target.closest(".wishlist-toggle")) return;
      openTrackDetail(exploreTracksCache.find((t) => t.id === card.dataset.id));
    });
  });
  grid.querySelectorAll(".wishlist-toggle").forEach((btn) => {
    btn.addEventListener("click", async (e) => {
      e.stopPropagation();
      await handleWishlistToggle(btn.dataset.id);
      renderExploreTracks();
    });
  });
}

function setExploreView(v) {
  exploreFilters.view = v;
  el("explore-view-grid").classList.toggle("active", v === "grid");
  el("explore-view-list").classList.toggle("active", v === "list");
  renderExploreTracks();
}

function setExplorePrice(p) {
  exploreFilters.price = p;
  ["all", "free", "paid"].forEach((k) => el(`explore-price-${k}`).classList.toggle("active", k === p));
  loadExploreTracks();
}

async function loadExploreTracks() {
  try {
    const [tracks, wishlist] = await Promise.all([
      invoke("explore_tracks", {
        search: exploreFilters.search || null,
        genre: exploreFilters.genre || null,
        priceFilter: exploreFilters.price,
        sort: exploreFilters.sort,
      }),
      invoke("list_wishlist"),
    ]);
    exploreTracksCache = tracks;
    exploreWishlistIds = new Set(wishlist.map((t) => t.id));
    renderExploreTracks();
  } catch (err) {
    showToast(`Couldn't load Explore: ${err}`, "err");
  }
}

function setupExplorePage() {
  renderExploreGenreList();
  el("explore-view-grid").addEventListener("click", () => setExploreView("grid"));
  el("explore-view-list").addEventListener("click", () => setExploreView("list"));
  el("explore-price-all").addEventListener("click", () => setExplorePrice("all"));
  el("explore-price-free").addEventListener("click", () => setExplorePrice("free"));
  el("explore-price-paid").addEventListener("click", () => setExplorePrice("paid"));
  el("explore-sort").addEventListener("change", (e) => {
    exploreFilters.sort = e.target.value;
    loadExploreTracks();
  });
  el("explore-search").addEventListener(
    "input",
    debounce((e) => {
      exploreFilters.search = e.target.value;
      loadExploreTracks();
    }, 300)
  );
  el("explore-reset").addEventListener("click", () => {
    exploreFilters = { price: "all", genre: null, sort: "recent", search: "", view: exploreFilters.view };
    el("explore-search").value = "";
    el("explore-sort").value = "recent";
    setExplorePrice("all");
    renderExploreGenreList();
    loadExploreTracks();
  });

  el("track-detail-close").addEventListener("click", closeTrackDetail);
  el("track-detail-backdrop").addEventListener("click", (e) => {
    if (e.target.id === "track-detail-backdrop") closeTrackDetail();
  });
  el("track-detail-buy").addEventListener("click", async () => {
    if (!selectedExploreTrack) return;
    if (selectedExploreTrack.priceCents > 0) {
      const track = selectedExploreTrack;
      closeTrackDetail();
      openPaymentFlow(track);
    } else {
      await downloadTrackToDisk(selectedExploreTrack);
    }
  });
  el("track-detail-wishlist").addEventListener("click", async () => {
    if (!selectedExploreTrack) return;
    await handleWishlistToggle(selectedExploreTrack.id);
    el("track-detail-wishlist").textContent = exploreWishlistIds.has(selectedExploreTrack.id)
      ? "♥ Remove from wishlist"
      : "♡ Add to wishlist";
    renderExploreTracks();
  });
}

function openTrackDetail(track) {
  if (!track) return;
  selectedExploreTrack = track;
  el("track-detail-hero").style.background = `linear-gradient(135deg, ${track.gradientStart} 0%, ${track.gradientEnd} 100%)`;
  el("track-detail-hero").textContent = track.emoji;
  el("track-detail-title").textContent = track.title;
  el("track-detail-creator").textContent = `by ${track.creatorName}`;
  el("track-detail-genre").textContent = track.genre || "—";
  el("track-detail-rating").textContent = track.ratingCount > 0 ? `★ ${track.ratingAverage.toFixed(1)}` : "No ratings yet";
  el("track-detail-description").textContent = track.description || "";
  el("track-detail-audio").src = track.previewUrl;
  el("track-detail-buy").textContent =
    track.priceCents > 0 ? `Buy for $${(track.priceCents / 100).toFixed(2)}` : "Download Free";
  el("track-detail-wishlist").textContent = exploreWishlistIds.has(track.id)
    ? "♥ Remove from wishlist"
    : "♡ Add to wishlist";
  el("track-detail-backdrop").classList.remove("hidden");
}

function closeTrackDetail() {
  const audio = el("track-detail-audio");
  audio.pause();
  audio.src = "";
  el("track-detail-backdrop").classList.add("hidden");
  selectedExploreTrack = null;
}

async function downloadTrackToDisk(track) {
  try {
    const destPath = await invoke("plugin:dialog|save", {
      options: {
        defaultPath: `${track.title}.song.json`,
        filters: [{ name: "Sargam Project", extensions: ["json"] }],
      },
    });
    if (!destPath) return;
    await invoke("download_track", { trackId: track.id, destPath });
    showToast(`Downloaded ${track.title}`, "ok");
  } catch (err) {
    showToast(`Download failed: ${err}`, "err");
  }
}

// ---- Wishlist ----

async function loadWishlistTracks() {
  try {
    wishlistTracksCache = await invoke("list_wishlist");
    exploreWishlistIds = new Set(wishlistTracksCache.map((t) => t.id));
    renderWishlistTracks();
  } catch (err) {
    showToast(`Couldn't load wishlist: ${err}`, "err");
  }
}

function renderWishlistTracks() {
  el("wishlist-count").textContent = `${wishlistTracksCache.length} track${wishlistTracksCache.length === 1 ? "" : "s"} saved`;
  const grid = el("wishlist-tracks");
  const empty = el("wishlist-empty");
  if (wishlistTracksCache.length === 0) {
    grid.innerHTML = "";
    empty.classList.remove("hidden");
    return;
  }
  empty.classList.add("hidden");
  grid.innerHTML = wishlistTracksCache
    .map(
      (track) => `
    <div class="track-card" data-id="${track.id}">
      <div class="track-thumb" style="background:linear-gradient(135deg, ${track.gradientStart} 0%, ${track.gradientEnd} 100%)">${track.emoji}</div>
      <div class="track-body">
        <div class="track-title">${track.title}</div>
        <div class="track-creator">${track.creatorName}</div>
        <div style="font-size:13px;font-weight:700;color:var(--color-accent);margin-bottom:8px">${
          track.priceCents > 0 ? "$" + (track.priceCents / 100).toFixed(2) : "Free"
        }</div>
        <div style="display:flex;gap:4px">
          <button class="track-action wishlist-buy" data-id="${track.id}" style="flex:1">${
            track.priceCents > 0 ? "Buy" : "Download"
          }</button>
          <button class="btn btn-ghost wishlist-remove" data-id="${track.id}" style="padding:6px 8px">×</button>
        </div>
      </div>
    </div>`
    )
    .join("");
  grid.querySelectorAll(".track-card").forEach((card) => {
    card.addEventListener("click", (e) => {
      if (e.target.closest("button")) return;
      openTrackDetail(wishlistTracksCache.find((t) => t.id === card.dataset.id));
    });
  });
  grid.querySelectorAll(".wishlist-buy").forEach((btn) => {
    btn.addEventListener("click", async (e) => {
      e.stopPropagation();
      const track = wishlistTracksCache.find((t) => t.id === btn.dataset.id);
      if (track.priceCents > 0) openPaymentFlow(track);
      else await downloadTrackToDisk(track);
    });
  });
  grid.querySelectorAll(".wishlist-remove").forEach((btn) => {
    btn.addEventListener("click", async (e) => {
      e.stopPropagation();
      await handleWishlistToggle(btn.dataset.id);
      await loadWishlistTracks();
    });
  });
}

function setupWishlistPage() {
  el("wishlist-explore-btn").addEventListener("click", () => {
    hideWishlistPage();
    requireAuth(showExplorePage);
  });
}

// ---- Upload ----

async function openUploadPage(project) {
  uploadState = {
    step: 1,
    projectPath: project.path,
    title: project.title,
    description: "",
    genre: "",
    pricing: "free",
    priceCents: 0,
  };
  showUploadPage();
  await populateUploadProjectSelect();
  el("upload-project-select").value = project.path;
  el("upload-title").value = project.title;
  el("upload-description").value = "";
  el("upload-genre").value = "";
  el("upload-price").value = "";
  setUploadPricing("free");
  renderUploadStep();
}

async function populateUploadProjectSelect() {
  try {
    const list = await invoke("list_projects", { dir: settings.projectsDir });
    el("upload-project-select").innerHTML = list.map((p) => `<option value="${p.path}">${p.title}</option>`).join("");
  } catch (err) {
    showToast(`Couldn't list projects: ${err}`, "err");
  }
}

function renderUploadStep() {
  for (let i = 1; i <= 4; i++) {
    el(`upload-step-${i}`).classList.toggle("hidden", uploadState.step !== i);
    if (i <= 3) {
      const dot = el(`upload-step-dot-${i}`);
      dot.classList.toggle("active", uploadState.step === i);
      dot.classList.toggle("complete", uploadState.step > i);
    }
  }
  el("upload-step-dot-4").classList.toggle("active", uploadState.step === 4);

  if (uploadState.step === 3) {
    el("upload-review").innerHTML = `
      <div style="display:flex;justify-content:space-between;margin-bottom:8px"><span style="color:var(--color-accent-700);font-size:13px">Title</span><span style="font-weight:600">${uploadState.title}</span></div>
      <div style="display:flex;justify-content:space-between;margin-bottom:8px"><span style="color:var(--color-accent-700);font-size:13px">Genre</span><span style="font-weight:600">${uploadState.genre || "—"}</span></div>
      <div style="display:flex;justify-content:space-between"><span style="color:var(--color-accent-700);font-size:13px">Price</span><span style="font-weight:600">${
        uploadState.pricing === "free" ? "Free" : "$" + (uploadState.priceCents / 100).toFixed(2)
      }</span></div>`;
  }
}

function setUploadPricing(kind) {
  uploadState.pricing = kind;
  el("upload-pricing-free").classList.toggle("selected", kind === "free");
  el("upload-pricing-paid").classList.toggle("selected", kind === "paid");
  el("upload-price-group").classList.toggle("hidden", kind !== "paid");
}

function updateUploadEarnPreview() {
  const price = parseFloat(el("upload-price").value) || 0;
  uploadState.priceCents = Math.round(price * 100);
  const earn = ((uploadState.priceCents * 0.8) / 100).toFixed(2);
  el("upload-earn-preview").textContent = `You'll earn: $${earn}`;
}

function setupUploadPage() {
  el("upload-next-1").addEventListener("click", () => {
    uploadState.projectPath = el("upload-project-select").value;
    uploadState.title = el("upload-title").value.trim();
    uploadState.description = el("upload-description").value.trim();
    uploadState.genre = el("upload-genre").value;
    if (!uploadState.title) {
      showToast("Give your track a title", "err");
      return;
    }
    uploadState.step = 2;
    renderUploadStep();
  });
  el("upload-back-2").addEventListener("click", () => {
    uploadState.step = 1;
    renderUploadStep();
  });
  el("upload-pricing-free").addEventListener("click", () => setUploadPricing("free"));
  el("upload-pricing-paid").addEventListener("click", () => setUploadPricing("paid"));
  el("upload-price").addEventListener("input", updateUploadEarnPreview);
  el("upload-next-2").addEventListener("click", () => {
    if (uploadState.pricing === "free") {
      uploadState.priceCents = 0;
    } else if (!uploadState.priceCents || uploadState.priceCents <= 0) {
      showToast("Enter a price", "err");
      return;
    }
    uploadState.step = 3;
    renderUploadStep();
  });
  el("upload-back-3").addEventListener("click", () => {
    uploadState.step = 2;
    renderUploadStep();
  });
  el("upload-do").addEventListener("click", async () => {
    const btn = el("upload-do");
    btn.disabled = true;
    btn.textContent = "Uploading…";
    try {
      const track = await invoke("upload_track", {
        songPath: uploadState.projectPath,
        title: uploadState.title,
        description: uploadState.description,
        genre: uploadState.genre,
        priceCents: uploadState.priceCents,
      });
      uploadState.step = 4;
      renderUploadStep();
      el("upload-success-title").textContent = `${track.title} is now live`;
    } catch (err) {
      showToast(`Upload failed: ${err}`, "err");
    } finally {
      btn.disabled = false;
      btn.textContent = "Upload Now";
    }
  });
  el("upload-view-explore").addEventListener("click", () => {
    hideUploadPage();
    requireAuth(showExplorePage);
  });
  el("upload-view-earnings").addEventListener("click", () => {
    hideUploadPage();
    requireAuth(showEarningsPage);
  });
}

// ---- Earnings ----

async function loadEarnings() {
  try {
    const summary = await invoke("list_my_earnings");
    renderEarnings(summary);
  } catch (err) {
    showToast(`Couldn't load earnings: ${err}`, "err");
  }
}

function renderEarnings(summary) {
  const totalSales = summary.tracks.reduce((s, t) => s + t.salesCount, 0);
  const totalDownloads = summary.tracks.reduce((s, t) => s + t.downloadsCount, 0);
  el("earnings-kpis").innerHTML = `
    <div class="kpi-card"><div class="kpi-label">Total Earnings</div><div class="kpi-value" style="color:var(--color-accent)">$${(
      summary.totalEarningsCents / 100
    ).toFixed(2)}</div></div>
    <div class="kpi-card" style="border-left-color:var(--color-accent-2)"><div class="kpi-label">Sales</div><div class="kpi-value" style="color:var(--color-accent-2)">${totalSales}</div></div>
    <div class="kpi-card" style="border-left-color:#667eea"><div class="kpi-label">Downloads</div><div class="kpi-value" style="color:#667eea">${totalDownloads}</div></div>
    <div class="kpi-card" style="border-left-color:#f093fb"><div class="kpi-label">Tracks</div><div class="kpi-value" style="color:#f093fb">${summary.tracks.length}</div></div>`;

  const list = el("earnings-tracks");
  list.innerHTML =
    summary.tracks.length === 0
      ? `<p style="opacity:0.7">Nothing uploaded yet.</p>`
      : summary.tracks
          .map(
            (t) => `
      <div class="track-item">
        <div style="flex:1">
          <div style="font-weight:700;font-size:14px">${t.title}</div>
          <div style="font-size:12px;color:var(--color-accent-700);margin-bottom:4px">${t.genre || "—"}</div>
          <div style="font-size:12px">${t.salesCount} sales · ${t.downloadsCount} downloads · ${
              t.priceCents > 0 ? "$" + (t.priceCents / 100).toFixed(2) : "Free"
            }</div>
        </div>
        <div style="text-align:right">
          <div style="font-size:11px;color:var(--color-accent-700)">Earnings</div>
          <div style="font-size:18px;font-weight:700;color:var(--color-accent)">$${(t.earningsCents / 100).toFixed(2)}</div>
        </div>
      </div>`
          )
          .join("");

  el("earnings-available").textContent = `$${(summary.availableCents / 100).toFixed(2)}`;
  el("earnings-request-payout").dataset.available = summary.availableCents;
}

function setupEarningsPage() {
  el("earnings-request-payout").addEventListener("click", async () => {
    const available = parseInt(el("earnings-request-payout").dataset.available || "0", 10);
    if (available <= 0) {
      showToast("No available balance to request.", "err");
      return;
    }
    const input = prompt(`Amount to request (max $${(available / 100).toFixed(2)})`, (available / 100).toFixed(2));
    if (input === null) return;
    const amountCents = Math.round(parseFloat(input) * 100);
    if (!amountCents || amountCents <= 0 || amountCents > available) {
      showToast("Enter a valid amount within your available balance.", "err");
      return;
    }
    try {
      await invoke("request_payout", { amountCents });
      showToast("Payout requested.", "ok");
      await loadEarnings();
    } catch (err) {
      showToast(`Payout request failed: ${err}`, "err");
    }
  });
}

// ---- Payment (Stripe, embedded — no redirect) ----

async function ensureStripeLoaded() {
  if (stripe) return stripe;
  const config = await invoke("get_public_config");
  if (!config.stripePublishableKey) {
    throw new Error("Stripe isn't configured yet — add STRIPE_PUBLISHABLE_KEY to .env and rebuild.");
  }
  stripe = window.Stripe(config.stripePublishableKey);
  return stripe;
}

function showPaymentError(msg) {
  el("payment-error").textContent = msg;
  el("payment-error").classList.remove("hidden");
}

async function openPaymentFlow(track) {
  paymentTrack = track;
  el("payment-stage-method").classList.remove("hidden");
  el("payment-stage-receipt").classList.add("hidden");
  el("payment-error").classList.add("hidden");
  el("payment-total").textContent = `$${(track.priceCents / 100).toFixed(2)}`;
  el("payment-pay").disabled = false;
  el("payment-pay").textContent = "Pay";
  el("payment-card-element").innerHTML = "";
  el("payment-backdrop").classList.remove("hidden");

  try {
    await ensureStripeLoaded();
    const intent = await invoke("start_stripe_purchase", { trackId: track.id });
    paymentPurchaseId = intent.purchaseId;
    stripeElements = stripe.elements({ clientSecret: intent.clientSecret });
    const cardElement = stripeElements.create("payment");
    cardElement.mount("#payment-card-element");
  } catch (err) {
    showPaymentError(String(err.message ?? err));
  }
}

function closePaymentFlow() {
  el("payment-backdrop").classList.add("hidden");
  stripeElements = null;
  paymentTrack = null;
  paymentPurchaseId = null;
}

function setupPaymentFlow() {
  el("payment-cancel").addEventListener("click", closePaymentFlow);
  el("payment-backdrop").addEventListener("click", (e) => {
    if (e.target.id === "payment-backdrop") closePaymentFlow();
  });
  el("payment-pay").addEventListener("click", async () => {
    if (!stripe || !stripeElements || !paymentTrack) return;
    el("payment-pay").disabled = true;
    el("payment-pay").textContent = "Processing…";
    try {
      const { error } = await stripe.confirmPayment({
        elements: stripeElements,
        redirect: "if_required",
        confirmParams: { return_url: window.location.href },
      });
      if (error) throw new Error(error.message);
      const status = await invoke("finalize_purchase", { purchaseId: paymentPurchaseId });
      if (status.status !== "completed") {
        throw new Error("Payment is still processing — check back shortly.");
      }
      el("payment-stage-method").classList.add("hidden");
      el("payment-stage-receipt").classList.remove("hidden");
      el("payment-receipt-title").textContent = `${paymentTrack.title} — you can download it now`;
    } catch (err) {
      showPaymentError(String(err.message ?? err));
      el("payment-pay").disabled = false;
      el("payment-pay").textContent = "Pay";
    }
  });
  el("payment-download").addEventListener("click", async () => {
    if (!paymentTrack) return;
    await downloadTrackToDisk(paymentTrack);
  });
  el("payment-done").addEventListener("click", () => {
    closePaymentFlow();
    loadExploreTracks().catch(() => {});
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
    currentSongPath = null;
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
      currentSongPath = path;
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
      currentSongPath = path;
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
    el("export-stems").checked = !!settings?.exportStemsDefault;
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
  settings = await invoke("get_settings");
  applyTimelineZoom(settings.timelineZoom);
  await refreshAuthStatus();
  setupAutoSave();

  instruments = await invoke("list_instruments");
  taals = await invoke("list_taals");
  populateTaalSelect();
  renderInstrumentSidebar();
  renderTaalPills();
  setupNav();
  setupAuthModal();
  setupProjectsPage();
  setupExplorePage();
  setupWishlistPage();
  setupUploadPage();
  setupEarningsPage();
  setupPaymentFlow();
  await setupEvents();
  setupOnboardingEvents();
  await pullSong();
  requestAnimationFrame(tickTransport);

  if (!localStorage.getItem("sargam-launched")) {
    startOnboarding();
  }
}

main();
