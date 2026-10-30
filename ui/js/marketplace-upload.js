
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
  // Mobile can only publish free tracks (no in-app selling).
  if (!paymentsEnabled) kind = "free";
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
