
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

/// Grid cell under a pointer event, or null if outside the note area.
function gridCellAt(e, track) {
  const { rows, cols } = gridDimensions(track);
  const rect = e.currentTarget.getBoundingClientRect();
  const x = e.clientX - rect.left;
  const y = e.clientY - rect.top;
  if (x < LEFT_MARGIN) return null;
  const col = Math.floor((x - LEFT_MARGIN) / CELL_W);
  const row = Math.floor(y / CELL_H);
  if (col < 0 || col >= cols || row < 0 || row >= rows) return null;
  return { col, row };
}

function findNoteIdx(pattern, tick, trigger) {
  const key = JSON.stringify(trigger);
  return pattern.notes.findIndex((n) => n.start === tick && JSON.stringify(n.trigger) === key);
}

/// Sets one cell to filled/empty in the local song (no push). Returns whether it changed.
function setGridCell(track, pattern, cell, fill) {
  const tick = cell.col * STEP_TICKS;
  const trigger = rowTrigger(track, cell.row);
  const idx = findNoteIdx(pattern, tick, trigger);
  if (fill && idx < 0) {
    pattern.notes.push({ start: tick, duration: STEP_TICKS, trigger, velocity: 0.8, ornament: "Plain" });
    return true;
  }
  if (!fill && idx >= 0) {
    pattern.notes.splice(idx, 1);
    return true;
  }
  return false;
}

/// Click toggles a cell; click-drag paints (or erases) every cell passed over,
/// deciding which from whether the first cell was empty. The song is pushed
/// once on release. Touch keeps tap-to-toggle only, so swiping still scrolls
/// the grid.
function setupGridPainting() {
  const canvas = el("grid");
  let paint = null; // { track, pattern, fill, visited, changed } while a mouse/pen drag is active
  let touchCell = null;

  async function commit(pattern, changed) {
    if (!changed) return;
    pattern.notes.sort((a, b) => a.start - b.start);
    await pushSong();
    renderGrid();
    renderTrackList();
  }

  canvas.addEventListener("pointerdown", (e) => {
    if (song.tracks.length === 0 || (e.pointerType === "mouse" && e.button !== 0)) return;
    const track = song.tracks[selectedTrack];
    const cell = gridCellAt(e, track);
    if (!cell) return;
    if (e.pointerType === "touch") {
      touchCell = cell;
      return;
    }
    const { pattern } = gridDimensions(track);
    const fill = findNoteIdx(pattern, cell.col * STEP_TICKS, rowTrigger(track, cell.row)) < 0;
    paint = { track, pattern, fill, visited: new Set([`${cell.col},${cell.row}`]), changed: false };
    paint.changed = setGridCell(track, pattern, cell, fill);
    canvas.setPointerCapture(e.pointerId);
    renderGrid();
  });

  canvas.addEventListener("pointermove", (e) => {
    if (!paint) return;
    const cell = gridCellAt(e, paint.track);
    if (!cell) return;
    const key = `${cell.col},${cell.row}`;
    if (paint.visited.has(key)) return;
    paint.visited.add(key);
    if (setGridCell(paint.track, paint.pattern, cell, paint.fill)) {
      paint.changed = true;
      renderGrid();
    }
  });

  canvas.addEventListener("pointerup", async () => {
    if (paint) {
      const { pattern, changed } = paint;
      paint = null;
      await commit(pattern, changed);
    } else if (touchCell) {
      const cell = touchCell;
      touchCell = null;
      const track = song.tracks[selectedTrack];
      const { pattern } = gridDimensions(track);
      const fill = findNoteIdx(pattern, cell.col * STEP_TICKS, rowTrigger(track, cell.row)) < 0;
      await commit(pattern, setGridCell(track, pattern, cell, fill));
    }
  });

  canvas.addEventListener("pointercancel", () => {
    // Interrupted drag: keep cells already painted; drop a pending touch tap (it was a scroll).
    if (paint) {
      const { pattern, changed } = paint;
      paint = null;
      commit(pattern, changed);
    }
    touchCell = null;
  });
}
