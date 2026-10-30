
// -------------------------------------------------------------- marketplace --
//
// Explore/Wishlist/Upload/Earnings/Payment. Talks to the Rust commands in
// marketplace.rs, which in turn talk to Supabase (PostgREST + Storage + two
// Edge Functions for Stripe) — see CLAUDE.md's Supabase section and
// supabase/migrations/0001_marketplace.sql for the server side of this.

function trackPriceTag(track) {
  return track.priceCents > 0
    ? `<span class="tag price">$${(track.priceCents / 100).toFixed(2)}</span>`
    : `<span class="tag price">Free</span>`;
}

function trackCardHtml(track) {
  const saved = exploreWishlistIds.has(track.id);
  return `
    <div class="track-card" data-id="${track.id}">
      <div class="track-thumb" style="background:linear-gradient(135deg, ${track.gradientStart} 0%, ${track.gradientEnd} 100%)">${emojiArt(track.emoji, 56)}</div>
      <div class="track-body">
        <div class="track-title">${track.title}</div>
        <div class="track-creator">${track.creatorName}</div>
        <div class="track-meta"><span class="tag">${track.genre || "—"}</span>${trackPriceTag(track)}</div>
        <div class="track-stats">${track.ratingCount > 0 ? icon("star", 13) + " " + track.ratingAverage.toFixed(1) : "No ratings yet"} · ${track.downloadsCount} DL</div>
        <button class="track-action wishlist-toggle" data-id="${track.id}">${saved ? iconLabel("heartFilled", "Saved", 14) : iconLabel("heart", "Save", 14)}</button>
      </div>
    </div>`;
}

async function handleWishlistToggle(trackId) {
  try {
    const saved = await invoke("toggle_wishlist", { trackId });
    if (saved) exploreWishlistIds.add(trackId);
    else exploreWishlistIds.delete(trackId);
    return saved;
  } catch (err) {
    showToast(`Couldn't update wishlist: ${err}`, "err");
    return exploreWishlistIds.has(trackId);
  }
}

// ---- Explore ----

function renderExploreGenreList() {
  const box = el("explore-genre-list");
  box.innerHTML = MARKETPLACE_GENRES.map(
    (g) => `<button class="filter-btn ${exploreFilters.genre === g ? "active" : ""}" data-genre="${g}">${g}</button>`
  ).join("");
  box.querySelectorAll("button").forEach((btn) => {
    btn.addEventListener("click", () => {
      exploreFilters.genre = exploreFilters.genre === btn.dataset.genre ? null : btn.dataset.genre;
      renderExploreGenreList();
      loadExploreTracks();
    });
  });
}

function renderExploreTracks() {
  const grid = el("explore-tracks");
  grid.classList.toggle("list-view", exploreFilters.view === "list");
  // On mobile (payments off) never surface paid tracks — there's no compliant
  // way to buy them in-app.
  const tracks = paymentsEnabled
    ? exploreTracksCache
    : exploreTracksCache.filter((t) => !t.priceCents);
  if (tracks.length === 0) {
    grid.innerHTML = `<div class="empty-state-inline">${emptyStateHtml("emptySearch", "Nothing matches yet", "Try clearing a filter or searching for something else.")}</div>`;
    return;
  }
  grid.innerHTML = tracks.map(trackCardHtml).join("");
  grid.querySelectorAll(".track-card").forEach((card) => {
    card.addEventListener("click", (e) => {
      if (e.target.closest(".wishlist-toggle")) return;
      openTrackDetail(exploreTracksCache.find((t) => t.id === card.dataset.id));
    });
  });
  grid.querySelectorAll(".wishlist-toggle").forEach((btn) => {
    btn.addEventListener("click", async (e) => {
      e.stopPropagation();
      await handleWishlistToggle(btn.dataset.id);
      renderExploreTracks();
    });
  });
}

function setExploreView(v) {
  exploreFilters.view = v;
  el("explore-view-grid").classList.toggle("active", v === "grid");
  el("explore-view-list").classList.toggle("active", v === "list");
  renderExploreTracks();
}

function setExplorePrice(p) {
  exploreFilters.price = p;
  ["all", "free", "paid"].forEach((k) => el(`explore-price-${k}`).classList.toggle("active", k === p));
  loadExploreTracks();
}

async function loadExploreTracks() {
  try {
    const [tracks, wishlist] = await Promise.all([
      invoke("explore_tracks", {
        search: exploreFilters.search || null,
        genre: exploreFilters.genre || null,
        priceFilter: exploreFilters.price,
        sort: exploreFilters.sort,
      }),
      invoke("list_wishlist"),
    ]);
    exploreTracksCache = tracks;
    exploreWishlistIds = new Set(wishlist.map((t) => t.id));
    renderExploreTracks();
  } catch (err) {
    showToast(`Couldn't load Explore: ${err}`, "err");
  }
}

function setupExplorePage() {
  renderExploreGenreList();
  el("explore-view-grid").addEventListener("click", () => setExploreView("grid"));
  el("explore-view-list").addEventListener("click", () => setExploreView("list"));
  el("explore-price-all").addEventListener("click", () => setExplorePrice("all"));
  el("explore-price-free").addEventListener("click", () => setExplorePrice("free"));
  el("explore-price-paid").addEventListener("click", () => setExplorePrice("paid"));
  el("explore-sort").addEventListener("change", (e) => {
    exploreFilters.sort = e.target.value;
    loadExploreTracks();
  });
  el("explore-search").addEventListener(
    "input",
    debounce((e) => {
      exploreFilters.search = e.target.value;
      loadExploreTracks();
    }, 300)
  );
  el("explore-reset").addEventListener("click", () => {
    exploreFilters = { price: "all", genre: null, sort: "recent", search: "", view: exploreFilters.view };
    el("explore-search").value = "";
    el("explore-sort").value = "recent";
    setExplorePrice("all");
    renderExploreGenreList();
    loadExploreTracks();
  });

  el("track-detail-close").addEventListener("click", closeTrackDetail);
  el("track-detail-backdrop").addEventListener("click", (e) => {
    if (e.target.id === "track-detail-backdrop") closeTrackDetail();
  });
  el("track-detail-buy").addEventListener("click", async () => {
    if (!selectedExploreTrack) return;
    if (selectedExploreTrack.priceCents > 0) {
      const track = selectedExploreTrack;
      closeTrackDetail();
      openPaymentFlow(track);
    } else {
      await downloadTrackToDisk(selectedExploreTrack);
    }
  });
  el("track-detail-wishlist").addEventListener("click", async () => {
    if (!selectedExploreTrack) return;
    await handleWishlistToggle(selectedExploreTrack.id);
    setDetailWishlistLabel(exploreWishlistIds.has(selectedExploreTrack.id));
    renderExploreTracks();
  });

  // Moderation controls (report / block a creator).
  el("track-detail-report").addEventListener("click", () => {
    if (selectedExploreTrack) requireAuth(() => openReportModal(selectedExploreTrack));
  });
  el("track-detail-block").addEventListener("click", () => {
    if (selectedExploreTrack) requireAuth(() => blockSelectedCreator(selectedExploreTrack));
  });
  el("report-cancel").addEventListener("click", closeReportModal);
  el("report-backdrop").addEventListener("click", (e) => {
    if (e.target.id === "report-backdrop") closeReportModal();
  });
  el("report-submit").addEventListener("click", submitReport);
}

function openTrackDetail(track) {
  if (!track) return;
  selectedExploreTrack = track;
  el("track-detail-hero").style.background = `linear-gradient(135deg, ${track.gradientStart} 0%, ${track.gradientEnd} 100%)`;
  el("track-detail-hero").innerHTML = emojiArt(track.emoji, 96);
  el("track-detail-title").textContent = track.title;
  el("track-detail-creator").textContent = `by ${track.creatorName}`;
  el("track-detail-genre").textContent = track.genre || "—";
  el("track-detail-rating").innerHTML = track.ratingCount > 0 ? `${icon("star", 14)} ${track.ratingAverage.toFixed(1)}` : "No ratings yet";
  el("track-detail-description").textContent = track.description || "";
  el("track-detail-audio").src = track.previewUrl;
  el("track-detail-buy").textContent =
    track.priceCents > 0 ? `Buy for $${(track.priceCents / 100).toFixed(2)}` : "Download Free";
  setDetailWishlistLabel(exploreWishlistIds.has(track.id));
  el("track-detail-backdrop").classList.remove("hidden");
}

function closeTrackDetail() {
  const audio = el("track-detail-audio");
  audio.pause();
  audio.src = "";
  el("track-detail-backdrop").classList.add("hidden");
  selectedExploreTrack = null;
}

// ---- Moderation: report & block ----

let reportingTrack = null;

function openReportModal(track) {
  reportingTrack = track;
  el("report-track-name").textContent = `Reporting "${track.title}" by ${track.creatorName}`;
  el("report-reason").value = "Copyright infringement";
  el("report-details").value = "";
  el("report-error").classList.add("hidden");
  el("report-backdrop").classList.remove("hidden");
}

function closeReportModal() {
  el("report-backdrop").classList.add("hidden");
  reportingTrack = null;
}

async function submitReport() {
  if (!reportingTrack) return;
  const btn = el("report-submit");
  btn.disabled = true;
  try {
    await invoke("report_track", {
      trackId: reportingTrack.id,
      reason: el("report-reason").value,
      details: el("report-details").value || null,
    });
    closeReportModal();
    showToast("Thanks — we'll review this track", "ok");
  } catch (err) {
    el("report-error").textContent = String(err);
    el("report-error").classList.remove("hidden");
  } finally {
    btn.disabled = false;
  }
}

async function blockSelectedCreator(track) {
  try {
    await invoke("block_creator", { creatorId: track.creatorId });
    closeTrackDetail();
    showToast(`Blocked ${track.creatorName} — their tracks are hidden`, "ok");
    // Re-fetch so the blocked creator's tracks disappear immediately.
    if (currentPage === "explore") loadExploreTracks();
    else if (currentPage === "wishlist") loadWishlistTracks();
  } catch (err) {
    showToast(`Couldn't block: ${err}`, "err");
  }
}

// ---- Account deletion (app-store requirement) ----

function openDeleteAccountModal() {
  el("delete-account-error").classList.add("hidden");
  el("delete-account-backdrop").classList.remove("hidden");
}

function closeDeleteAccountModal() {
  el("delete-account-backdrop").classList.add("hidden");
}

async function confirmDeleteAccount() {
  const btn = el("delete-account-confirm");
  btn.disabled = true;
  btn.textContent = "Deleting…";
  try {
    authStatus = await invoke("delete_account");
    closeDeleteAccountModal();
    renderDrawerContent();
    showToast("Your account has been deleted", "ok");
    // Reflect the now-signed-out state everywhere.
    if (currentPage !== "compose") goToCompose();
  } catch (err) {
    el("delete-account-error").textContent = String(err);
    el("delete-account-error").classList.remove("hidden");
  } finally {
    btn.disabled = false;
    btn.textContent = "Delete my account";
  }
}

async function downloadTrackToDisk(track) {
  try {
    const destPath = await invoke("plugin:dialog|save", {
      options: {
        defaultPath: `${track.title}.song.json`,
        filters: [{ name: "Sargam Project", extensions: ["json"] }],
      },
    });
    if (!destPath) return;
    await invoke("download_track", { trackId: track.id, destPath });
    showToast(`Downloaded ${track.title}`, "ok");
  } catch (err) {
    showToast(`Download failed: ${err}`, "err");
  }
}
