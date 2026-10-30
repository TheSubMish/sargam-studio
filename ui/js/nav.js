
// ---------------------------------------------------------------- routing --
//
// Only one persistent surface (the compose UI in #app) plus two overlays:
// Projects is a full-page overlay (own header, "← Back" returns to compose);
// Settings is a slide-out drawer over whatever's currently showing — both
// match "Sargam Main.dc.html", which has no separate nav row at all.

// #app and every .page are flex siblings inside #shell — only one of them
// may be un-hidden at a time, or they stack on top of each other (this is
// what made the Upload page look broken when opened from Projects: only
// #app was ever hidden, never #page-projects). switchPage() enforces that
// exclusivity centrally instead of each show*Page() hand-rolling it.
const PAGE_IDS = ["app", "page-projects", "page-explore", "page-wishlist", "page-upload", "page-earnings"];
function switchPage(pageId) {
  PAGE_IDS.forEach((id) => el(id).classList.toggle("hidden", id !== pageId));
  updateBottomNavActive();
}

function goToCompose() {
  currentPage = "compose";
  switchPage("app");
}

// Highlight the bottom-nav item matching the visible page. Settings is a
// drawer (an action, not a page) so it never stays "active"; Upload is only
// reachable from Projects, so it lights Projects.
function updateBottomNavActive() {
  const active = currentPage === "upload" ? "projects" : currentPage;
  document.querySelectorAll(".bottom-nav-btn").forEach((b) =>
    b.classList.toggle("active", b.dataset.nav === active)
  );
}

function showProjectsPage() {
  currentPage = "projects";
  switchPage("page-projects");
  loadProjects();
}

function hideProjectsPage() {
  currentPage = "compose";
  switchPage("app");
}

function showExplorePage() {
  currentPage = "explore";
  switchPage("page-explore");
  loadExploreTracks();
}
function hideExplorePage() {
  currentPage = "compose";
  switchPage("app");
}

function showWishlistPage() {
  currentPage = "wishlist";
  switchPage("page-wishlist");
  loadWishlistTracks();
}
function hideWishlistPage() {
  currentPage = "compose";
  switchPage("app");
}

function showUploadPage() {
  currentPage = "upload";
  switchPage("page-upload");
}
function hideUploadPage() {
  // Upload is only ever entered from a Projects card, so "back" returns
  // there rather than to the composer.
  currentPage = "projects";
  switchPage("page-projects");
  loadProjects();
}

function showEarningsPage() {
  // Payouts are a money feature — unavailable on mobile builds.
  if (!paymentsEnabled) {
    showToast("Earnings & payouts are managed on desktop / the website", "err");
    return;
  }
  currentPage = "earnings";
  switchPage("page-earnings");
  loadEarnings();
}
function hideEarningsPage() {
  currentPage = "compose";
  switchPage("app");
}

/// Explore/Wishlist/Upload/Earnings all require being signed in (product
/// decision — other people's tracks and your own sales aren't visible to a
/// logged-out session). Prompts the existing auth modal instead of proceeding
/// when there's no session yet.
function requireAuth(next) {
  if (!authStatus.isAuthenticated) {
    showToast("Log in to do that", "err");
    openAuthModal();
    return;
  }
  next();
}

function openSettingsDrawer() {
  renderDrawerContent();
  el("settings-drawer").classList.add("open");
}

function closeSettingsDrawer() {
  el("settings-drawer").classList.remove("open");
}

function setupNav() {
  el("open-projects").addEventListener("click", showProjectsPage);
  el("projects-back").addEventListener("click", hideProjectsPage);
  el("open-settings-drawer").addEventListener("click", openSettingsDrawer);
  el("close-settings-drawer").addEventListener("click", closeSettingsDrawer);

  el("open-explore").addEventListener("click", () => requireAuth(showExplorePage));
  el("open-wishlist").addEventListener("click", () => requireAuth(showWishlistPage));
  el("explore-back").addEventListener("click", hideExplorePage);
  el("wishlist-back").addEventListener("click", hideWishlistPage);
  el("upload-back").addEventListener("click", hideUploadPage);
  el("earnings-back").addEventListener("click", hideEarningsPage);

  // Delete-account confirmation modal.
  el("delete-account-cancel").addEventListener("click", closeDeleteAccountModal);
  el("delete-account-confirm").addEventListener("click", confirmDeleteAccount);
  el("delete-account-backdrop").addEventListener("click", (e) => {
    if (e.target.id === "delete-account-backdrop") closeDeleteAccountModal();
  });

  // Bottom navigation (mobile) — same destinations as the header icons.
  document.querySelectorAll(".bottom-nav-btn").forEach((btn) => {
    btn.addEventListener("click", () => {
      switch (btn.dataset.nav) {
        case "compose": goToCompose(); break;
        case "projects": showProjectsPage(); break;
        case "explore": requireAuth(showExplorePage); break;
        case "wishlist": requireAuth(showWishlistPage); break;
        case "settings": openSettingsDrawer(); break;
      }
    });
  });
  updateBottomNavActive();
}
