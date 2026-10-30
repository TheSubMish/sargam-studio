
// -------------------------------------------------------------- inspector --

function renderInspector() {
  const panel = el("inspector");
  if (song.tracks.length === 0) {
    panel.innerHTML = `<div class="empty-inspector">${emptyStateHtml("emptyMixer", "No mixer yet", "Add a track and its sound controls appear here.", 64)}</div>`;
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
      <button id="ins-mute" class="btn ${ui.muted ? "btn-accent" : "btn-secondary"}">${ui.muted ? iconLabel("volumeOff", "Muted") : iconLabel("volume", "Audio")}</button>
      <button id="ins-solo" class="btn ${ui.solo ? "btn-accent-2" : "btn-secondary"}">${iconLabel("headphones", "Solo")}</button>
    </div>
    <div class="action-row">
      <button id="ins-loop" class="btn btn-secondary btn-block">Loop region</button>
      <button id="ins-remove" class="btn btn-secondary" aria-label="Remove track" title="Remove track">${icon("close", 16)}</button>
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
