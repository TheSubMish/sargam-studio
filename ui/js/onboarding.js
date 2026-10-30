
// ------------------------------------------------------------- onboarding --

function renderOnboardInstrumentGrid() {
  const grid = el("ob-instrument-grid");
  grid.innerHTML = "";
  ONBOARD_INSTRUMENTS.forEach((inst) => {
    const card = document.createElement("button");
    card.className = "ob-instrument-card" + (inst.selected ? " selected" : "");
    card.innerHTML = `
      <div class="inst-art">${instrumentArt(inst.name, 56)}</div>
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
