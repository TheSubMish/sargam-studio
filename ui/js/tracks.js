
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
      <div class="track-art">${instrumentArt(track.instrument, 36)}</div>
      <div class="names">
        <div class="name">${track.name}</div>
        <div class="ne">${INSTRUMENT_NE[track.instrument] ?? track.instrument}</div>
      </div>
      <div class="sparkline">${dots}</div>
      <button class="remove" title="Remove track" aria-label="Remove track">${icon("close", 14)}</button>`;
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
    btn.innerHTML = `${instrumentArt(name, 32)}<span class="inst-label"><span>${name}</span><span class="ne">${INSTRUMENT_NE[name] ?? ""}</span></span>`;
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
