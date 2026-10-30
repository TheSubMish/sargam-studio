
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

  el("pattern-length").addEventListener("change", (e) => setPatternBeats(parseInt(e.target.value, 10)));
  el("length-up").addEventListener("click", () => setPatternBeats(currentPatternBeats() + BEATS_PER_BAR));
  el("length-down").addEventListener("click", () => setPatternBeats(currentPatternBeats() - BEATS_PER_BAR));
  el("extend-grid").addEventListener("click", async () => {
    await setPatternBeats(currentPatternBeats() + BEATS_PER_BAR);
    // Bring the new bar into view so the button visibly did something.
    const scroller = el("grid-scroll");
    scroller.scrollTo({ left: scroller.scrollWidth, behavior: "smooth" });
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

function setDetailWishlistLabel(saved) {
  el("track-detail-wishlist").innerHTML = saved
    ? iconLabel("heartFilled", "Remove from wishlist")
    : iconLabel("heart", "Add to wishlist");
}

async function main() {
  settings = await invoke("get_settings");
  applyTimelineZoom(settings.timelineZoom);
  // Platform payment capability (desktop yes, mobile no). Default true if the
  // field is absent (older backend) so desktop never loses its marketplace.
  try {
    const pc = await invoke("get_public_config");
    paymentsEnabled = pc.paymentsEnabled !== false;
  } catch (_) { /* keep default */ }
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
  applyPaymentGating();
  await setupEvents();
  setupOnboardingEvents();
  await pullSong();
  requestAnimationFrame(tickTransport);

  if (!localStorage.getItem("sargam-launched")) {
    startOnboarding();
  }
}

main();
