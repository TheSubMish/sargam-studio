
// ---- Wishlist ----

async function loadWishlistTracks() {
  try {
    wishlistTracksCache = await invoke("list_wishlist");
    exploreWishlistIds = new Set(wishlistTracksCache.map((t) => t.id));
    renderWishlistTracks();
  } catch (err) {
    showToast(`Couldn't load wishlist: ${err}`, "err");
  }
}

function renderWishlistTracks() {
  // On mobile (payments off), hide paid saved tracks — they can't be bought here.
  const visible = paymentsEnabled
    ? wishlistTracksCache
    : wishlistTracksCache.filter((t) => !t.priceCents);
  el("wishlist-count").textContent = `${visible.length} track${visible.length === 1 ? "" : "s"} saved`;
  const grid = el("wishlist-tracks");
  const empty = el("wishlist-empty");
  if (visible.length === 0) {
    grid.innerHTML = "";
    // Hide the flex:1 grid entirely so it stops claiming the vertical space —
    // otherwise the empty-state card gets pushed to the bottom instead of
    // centering in the free space (see .empty-state margin:auto).
    grid.classList.add("hidden");
    empty.classList.remove("hidden");
    return;
  }
  empty.classList.add("hidden");
  grid.classList.remove("hidden");
  grid.innerHTML = visible
    .map(
      (track) => `
    <div class="track-card" data-id="${track.id}">
      <div class="track-thumb" style="background:linear-gradient(135deg, ${track.gradientStart} 0%, ${track.gradientEnd} 100%)">${emojiArt(track.emoji, 56)}</div>
      <div class="track-body">
        <div class="track-title">${track.title}</div>
        <div class="track-creator">${track.creatorName}</div>
        <div style="font-size:13px;font-weight:700;color:var(--color-accent);margin-bottom:8px">${
          track.priceCents > 0 ? "$" + (track.priceCents / 100).toFixed(2) : "Free"
        }</div>
        <div style="display:flex;gap:4px">
          <button class="track-action wishlist-buy" data-id="${track.id}" style="flex:1">${
            track.priceCents > 0 ? "Buy" : "Download"
          }</button>
          <button class="btn btn-ghost wishlist-remove" data-id="${track.id}" style="padding:6px 8px" aria-label="Remove from wishlist">${icon("close", 14)}</button>
        </div>
      </div>
    </div>`
    )
    .join("");
  grid.querySelectorAll(".track-card").forEach((card) => {
    card.addEventListener("click", (e) => {
      if (e.target.closest("button")) return;
      openTrackDetail(wishlistTracksCache.find((t) => t.id === card.dataset.id));
    });
  });
  grid.querySelectorAll(".wishlist-buy").forEach((btn) => {
    btn.addEventListener("click", async (e) => {
      e.stopPropagation();
      const track = wishlistTracksCache.find((t) => t.id === btn.dataset.id);
      if (track.priceCents > 0) openPaymentFlow(track);
      else await downloadTrackToDisk(track);
    });
  });
  grid.querySelectorAll(".wishlist-remove").forEach((btn) => {
    btn.addEventListener("click", async (e) => {
      e.stopPropagation();
      await handleWishlistToggle(btn.dataset.id);
      await loadWishlistTracks();
    });
  });
}

function setupWishlistPage() {
  el("wishlist-explore-btn").addEventListener("click", () => {
    hideWishlistPage();
    requireAuth(showExplorePage);
  });
}
