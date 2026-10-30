
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
         <button class="icon-btn" id="drawer-earnings" title="Earnings" aria-label="Earnings">${icon("wallet", 20)}</button>
         <button class="icon-btn" id="drawer-logout" title="Log out" aria-label="Log out">Log out</button>
       </div>
       <button class="danger-link" id="drawer-delete-account">Delete account</button>`
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
    el("drawer-delete-account").addEventListener("click", () => {
      closeSettingsDrawer();
      openDeleteAccountModal();
    });
    const earningsLink = el("drawer-earnings");
    if (earningsLink) {
      // No payouts on mobile → no Earnings entry point.
      if (!paymentsEnabled) earningsLink.style.display = "none";
      earningsLink.addEventListener("click", () => {
        closeSettingsDrawer();
        requireAuth(showEarningsPage);
      });
    }
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
