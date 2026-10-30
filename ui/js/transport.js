
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
      el("play").innerHTML = PLAY_SVG;
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
  el("play").innerHTML = PAUSE_SVG;
}

async function pausePlayback() {
  transportBaseSecs = currentDisplayedPositionSecs();
  await invoke("pause");
  transportPlaying = false;
  el("play").classList.remove("playing");
  el("play").innerHTML = PLAY_SVG;
}

async function stopPlayback() {
  await invoke("stop");
  transportPlaying = false;
  transportBaseSecs = 0;
  el("play").classList.remove("playing");
  el("play").innerHTML = PLAY_SVG;
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
