
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
      <div class="project-card-art">${art("note", 64)}</div>
      <h3 class="project-card-title">${p.title}</h3>
      <div class="project-card-meta">
        <div>${p.trackCount} track${p.trackCount === 1 ? "" : "s"}</div>
        <div>${formatTime(p.durationSecs)}</div>
        <div>${formatRelativeTime(p.modifiedEpochSecs)}</div>
      </div>
      <button class="btn btn-ghost project-card-upload" style="width:100%;margin-bottom:6px;border-radius:6px;padding:6px;font-size:12px">Upload</button>
      <button class="project-card-remove" title="Delete" aria-label="Delete project">${icon("close", 14)}</button>`;
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
  el("new-project-fab").addEventListener("click", createNewProject);
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
