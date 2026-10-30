// Sargam desktop UI — plain JS, no build step. Talks to composer-core only
// through the Tauri commands defined in composer-desktop/src/main.rs; the
// frontend holds a local copy of the Song JSON and pushes it back with
// `set_song` after every edit (undo/redo happen server-side on that history).
//
// Visual design imported from the Claude Design "Organic" system (project
// c2fc1636-0a9a-4823-8313-a9adedc9fe63, "Sargam v3.dc.html"); tokens live in
// style.css. Layout adapts that mockup's static clip-grid sketch into a real
// track list + functional piano-roll/taal-grid editor underneath, since the
// mockup's own clip cells were empty no-op placeholders.

// Transport icons are SVG so they render identically on every OS (the ▶/⏸ glyphs didn't).
const PLAY_SVG = '<svg viewBox="0 0 24 24" width="22" height="22" fill="currentColor" aria-hidden="true"><path d="M8 5.5v13a1 1 0 0 0 1.52.85l10-6.5a1 1 0 0 0 0-1.7l-10-6.5A1 1 0 0 0 8 5.5z"/></svg>';
const PAUSE_SVG = '<svg viewBox="0 0 24 24" width="22" height="22" fill="currentColor" aria-hidden="true"><rect x="6" y="5" width="4.5" height="14" rx="1.5"/><rect x="13.5" y="5" width="4.5" height="14" rx="1.5"/></svg>';
const invoke = window.__TAURI__.core.invoke;
const TICKS_PER_BEAT = 480;
const STEP_TICKS = TICKS_PER_BEAT / 4; // 16th-note editing grid

const STROKES = ["Bass", "Open", "Rim", "Slap", "Mute"];
const PERCUSSION_INSTRUMENTS = new Set(["Damphu", "Madal"]);
const INSTRUMENT_NE = {
  Tungna: "टुङ्ना",
  Damphu: "डम्फु",
  Madal: "मादल",
  Harmonium: "हार्मोनियम",
  Bansuri: "बाँसुरी",
  Sarangi: "सारङ्गी",
};
const DEVANAGARI_DIGITS = ["०", "१", "२", "३", "४", "५", "६", "७", "८", "९"];

const PITCH_LOW = 48; // C3
const PITCH_HIGH = 84; // C6
const NOTE_NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
const PENTATONIC = new Set([9, 0, 2, 4, 7]); // A minor pentatonic, relative to A

// Onboarding's "choose your instruments" screen. The source design featured
// Sarangi here, which composer-core doesn't synthesize yet — swapped for
// Tungna (a real, playable instrument) rather than tease something silent.
const ONBOARD_INSTRUMENTS = [
  { name: "Madal", selected: true },
  { name: "Tungna", selected: false },
  { name: "Bansuri", selected: false },
];
const ONBOARD_SCREEN_COUNT = 5;
let onboardScreen = 0;

let song = null;
let instruments = [];
let taals = [];
let selectedTrack = 0;
/// Ephemeral mute/solo UI state, one entry per track index; NOT persisted in
/// the Song model (mute/solo are a mixer-session concept, not composition
/// data). `baseGain` is the "real" volume the slider edits; `track.gain` sent
/// to the backend is always recomputed from base/muted/solo before pushing.
let trackUi = [];
let transportPlaying = false;
let transportBaseSecs = 0;
let transportStartWallMs = 0;
/// True while the user has the seek-bar thumb grabbed — suspends
/// tickTransport's own auto-advance so the drag isn't fighting playback.
let isDraggingSeek = false;

let currentPage = "compose"; // "compose" | "projects"
let settings = null; // AppSettings from Rust; loaded once at startup
let projects = [];
let projectSort = "recent";
let projectSearch = "";
/// Path of the currently open project file, if it was saved/opened/created
/// via a real path (not a fresh unsaved song) — lets autosave and Projects
/// know where "here" is without a second source of truth.
let currentSongPath = null;
let autoSaveTimer = null;

// Marketplace: Explore/Wishlist/Upload/Earnings/Payment — see the "marketplace"
// section below for the bulk of this feature's logic.
const MARKETPLACE_GENRES = ["Orchestral", "Electronic", "Hip-hop", "Lo-fi", "Ambient", "Rock", "Jazz", "Folk"];
let exploreFilters = { price: "all", genre: null, sort: "recent", search: "", view: "grid" };
let exploreTracksCache = [];
let exploreWishlistIds = new Set();
let selectedExploreTrack = null;
let wishlistTracksCache = [];
let uploadState = { step: 1, projectPath: null, title: "", description: "", genre: "", pricing: "free", priceCents: 0 };
let stripe = null;
// Whether in-app paid purchases / seller payouts are available. False on the
// mobile (Android/iOS) build — app-store billing policy forbids Stripe for
// digital goods — so we hide every buy/sell-for-money/payout affordance and
// show only free tracks + free downloads. Set from get_public_config in main().
let paymentsEnabled = true;

// Hide the static monetary UI when payments are disabled (mobile). Dynamic
// bits (paid tracks in listings, the drawer's Earnings link) are gated where
// they're rendered — this covers the fixed elements. Idempotent.
function applyPaymentGating() {
  if (paymentsEnabled) return;
  ["upload-pricing-paid", "upload-view-earnings", "explore-price-paid"].forEach((id) => {
    const e = el(id);
    if (e) e.style.display = "none";
  });
}
let stripeElements = null;
let paymentTrack = null;
let paymentPurchaseId = null;
