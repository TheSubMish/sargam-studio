
// ----------------------------------------------------------------- grid ----

const BEATS_PER_BAR = 4;

// Resize the selected track's sheet. It can't shrink past its last note, so
// shortening never silently deletes notes the user can no longer see.
async function setPatternBeats(beats) {
  if (song.tracks.length === 0) return;
  const pattern = trackPattern(song.tracks[selectedTrack]);
  const lastEnd = pattern.notes.reduce((m, n) => Math.max(m, n.start + n.duration), 0);
  const minBeats = Math.max(1, Math.ceil(lastEnd / TICKS_PER_BEAT));
  let next = Math.min(9999, Math.max(1, Number.isFinite(beats) ? Math.round(beats) : 8));
  if (next < minBeats) {
    next = minBeats;
    showToast(`There are notes up to beat ${minBeats}. Remove them to shorten further.`);
  }
  pattern.length = next * TICKS_PER_BEAT;
  await pushSong();
  renderGrid();
  renderTrackList();
}

function currentPatternBeats() {
  if (song.tracks.length === 0) return 0;
  return Math.round(trackPattern(song.tracks[selectedTrack]).length / TICKS_PER_BEAT);
}

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
    el("editor-track-name").textContent = "No track selected";
    el("editor-empty").innerHTML = emptyStateHtml("emptyTracks", "Your song is quiet", "Choose an instrument to add your first track.", 80);
    el("editor-empty").classList.remove("hidden");
    el("extend-grid").classList.add("hidden");
    taalControls.classList.add("hidden");
    canvas.width = 0;
    canvas.height = 0;
    return;
  }

  el("editor-empty").classList.add("hidden");
  el("extend-grid").classList.remove("hidden");
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
  ctx.font = `600 11px ${getComputedStyle(document.body).fontFamily}`;
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
