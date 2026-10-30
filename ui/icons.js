// Hand-drawn SVG illustrations that replace the emoji used as UI. Every shape
// takes its colors from the Organic design tokens in style.css (never a hex
// literal), so the art follows the theme when a dark palette lands.
// Loaded before app.js; exposes instrumentArt(), art(), emptyStateHtml() and
// hydrateArt() as globals (the frontend has no module system).

const ART_COLORS = {
  line: "var(--color-neutral-800)",
  wood: "var(--color-accent-300)",
  woodDark: "var(--color-accent-600)",
  skin: "var(--color-accent-100)",
  sage: "var(--color-accent-2-300)",
  sageDark: "var(--color-accent-2-600)",
  paper: "var(--color-neutral-100)",
};

// 64x64 canvas, 2.5px round strokes: one shared drawing language.
const artSvg = (body, size = 64) =>
  `<svg class="art" viewBox="0 0 64 64" width="${size}" height="${size}" fill="none" stroke="${ART_COLORS.line}" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${body}</svg>`;

const ART = {
  // Madal: barrel drum on its side, laced heads, black tuning paste on the right head.
  Madal: `
    <path d="M14 22 C24 18 40 18 50 22 L50 42 C40 46 24 46 14 42 Z" fill="${ART_COLORS.wood}"/>
    <ellipse cx="14" cy="32" rx="5" ry="10" fill="${ART_COLORS.skin}"/>
    <ellipse cx="50" cy="32" rx="6.5" ry="10" fill="${ART_COLORS.skin}"/>
    <circle cx="50" cy="32" r="3.2" fill="${ART_COLORS.line}" stroke="none"/>
    <path d="M19 21 L25 44 M27 19.5 L33 45 M35 19.5 L41 45 M43 20 L47 43" stroke="${ART_COLORS.woodDark}" stroke-width="2"/>`,

  // Tungna: long-necked plucked lute with a round resonator and two strings.
  Tungna: `
    <ellipse cx="22" cy="44" rx="12" ry="11" fill="${ART_COLORS.wood}" transform="rotate(-35 22 44)"/>
    <circle cx="20" cy="46" r="3.2" fill="${ART_COLORS.line}" stroke="none"/>
    <path d="M29 36 L52 10" stroke-width="5" stroke="${ART_COLORS.woodDark}"/>
    <path d="M29 36 L52 10" stroke-width="2" stroke="${ART_COLORS.skin}"/>
    <path d="M24 42 L53 12 M26 45 L55 15" stroke-width="1.2"/>
    <circle cx="53" cy="9" r="2.3" fill="${ART_COLORS.sage}"/><circle cx="57" cy="14" r="2.3" fill="${ART_COLORS.sage}"/>`,

  // Bansuri: bamboo flute at an angle, node rings and finger holes.
  Bansuri: `
    <path d="M10 50 L50 10" stroke-width="9" stroke="${ART_COLORS.line}"/>
    <path d="M10 50 L50 10" stroke-width="5.2" stroke="${ART_COLORS.sage}"/>
    <path d="M20 40 L24 44 M37 23 L41 27" stroke="${ART_COLORS.sageDark}" stroke-width="2"/>
    <circle cx="27.5" cy="32.5" r="1.6" fill="${ART_COLORS.line}" stroke="none"/>
    <circle cx="31.5" cy="28.5" r="1.6" fill="${ART_COLORS.line}" stroke="none"/>
    <circle cx="35.5" cy="24.5" r="1.6" fill="${ART_COLORS.line}" stroke="none"/>
    <circle cx="15.5" cy="44.5" r="1.6" fill="${ART_COLORS.line}" stroke="none"/>
    <path d="M46 6 C50 4 56 6 58 12" stroke="${ART_COLORS.woodDark}" stroke-width="2"/>`,

  // Damphu: hand-held frame drum, a wide shallow rim with a single skin.
  Damphu: `
    <rect x="27" y="44" width="10" height="16" rx="4" fill="${ART_COLORS.woodDark}"/>
    <circle cx="32" cy="27" r="22" fill="${ART_COLORS.wood}"/>
    <circle cx="32" cy="27" r="17.5" fill="${ART_COLORS.skin}"/>
    <circle cx="32" cy="27" r="9.5" stroke="${ART_COLORS.woodDark}" stroke-width="1.8"/>
    <circle cx="32" cy="27" r="2.2" fill="${ART_COLORS.woodDark}" stroke="none"/>`,

  // Harmonium: boxed reed organ with folded bellows and a keyboard.
  Harmonium: `
    <rect x="8" y="14" width="48" height="14" rx="3" fill="${ART_COLORS.sage}"/>
    <path d="M14 14 V28 M22 14 V28 M30 14 V28 M38 14 V28 M46 14 V28" stroke="${ART_COLORS.sageDark}" stroke-width="1.6"/>
    <rect x="6" y="28" width="52" height="24" rx="4" fill="${ART_COLORS.wood}"/>
    <rect x="10" y="34" width="44" height="14" rx="2" fill="${ART_COLORS.paper}"/>
    <path d="M16 34 V48 M22 34 V48 M28 34 V48 M34 34 V48 M40 34 V48 M46 34 V48" stroke-width="1.5"/>
    <path d="M14 34 v7 M20 34 v7 M32 34 v7 M38 34 v7 M44 34 v7" stroke-width="3.2" stroke="${ART_COLORS.line}"/>`,

  // Feature illustrations (onboarding).
  compose: `
    <rect x="8" y="12" width="48" height="10" rx="5" fill="${ART_COLORS.wood}"/>
    <rect x="8" y="27" width="48" height="10" rx="5" fill="${ART_COLORS.sage}"/>
    <rect x="8" y="42" width="48" height="10" rx="5" fill="${ART_COLORS.skin}"/>
    <circle cx="20" cy="17" r="2" fill="${ART_COLORS.line}" stroke="none"/><circle cx="38" cy="17" r="2" fill="${ART_COLORS.line}" stroke="none"/>
    <circle cx="28" cy="32" r="2" fill="${ART_COLORS.line}" stroke="none"/><circle cx="46" cy="32" r="2" fill="${ART_COLORS.line}" stroke="none"/>
    <circle cx="16" cy="47" r="2" fill="${ART_COLORS.line}" stroke="none"/><circle cx="34" cy="47" r="2" fill="${ART_COLORS.line}" stroke="none"/>`,
  game: `
    <path d="M12 26 C12 18 18 16 24 16 H40 C46 16 52 18 52 26 L55 42 C56 49 48 50 45 45 L41 40 H23 L19 45 C16 50 8 49 9 42 Z" fill="${ART_COLORS.sage}"/>
    <path d="M22 24 V34 M17 29 H27" stroke-width="3"/>
    <circle cx="40" cy="27" r="2.4" fill="${ART_COLORS.line}" stroke="none"/><circle cx="46" cy="31" r="2.4" fill="${ART_COLORS.line}" stroke="none"/>`,
  folk: `
    <path d="M10 24 C18 20 30 20 38 24 L38 42 C30 46 18 46 10 42 Z" fill="${ART_COLORS.wood}"/>
    <ellipse cx="10" cy="33" rx="4" ry="9" fill="${ART_COLORS.skin}"/><ellipse cx="38" cy="33" rx="5" ry="9" fill="${ART_COLORS.skin}"/>
    <path d="M44 14 V34 M44 14 L56 11 V31" stroke-width="3"/>
    <ellipse cx="40" cy="36" rx="5" ry="4" fill="${ART_COLORS.line}" stroke="none"/><ellipse cx="52" cy="33" rx="5" ry="4" fill="${ART_COLORS.line}" stroke="none"/>`,

  // Empty-state illustrations.
  emptyProjects: `
    <path d="M6 20 C6 16 8 14 12 14 H24 L30 20 H52 C56 20 58 22 58 26 V46 C58 50 56 52 52 52 H12 C8 52 6 50 6 46 Z" fill="${ART_COLORS.wood}"/>
    <path d="M6 28 H58" stroke-width="2"/>
    <path d="M30 46 V34 L42 31 V43" stroke-width="2.5"/>
    <ellipse cx="27" cy="46" rx="4" ry="3" fill="${ART_COLORS.line}" stroke="none"/><ellipse cx="39" cy="43" rx="4" ry="3" fill="${ART_COLORS.line}" stroke="none"/>`,
  emptyWishlist: `
    <path d="M32 54 C14 42 8 33 8 24 C8 17 13 12 20 12 C25 12 29 15 32 19 C35 15 39 12 44 12 C51 12 56 17 56 24 C56 33 50 42 32 54 Z" fill="${ART_COLORS.skin}" stroke-dasharray="1 5"/>
    <path d="M28 40 V28 L38 26 V37" stroke-width="2.2"/>
    <ellipse cx="25.5" cy="40" rx="3.2" ry="2.5" fill="${ART_COLORS.line}" stroke="none"/><ellipse cx="35.5" cy="37" rx="3.2" ry="2.5" fill="${ART_COLORS.line}" stroke="none"/>`,
  emptyMixer: `
    <path d="M14 12 V52 M32 12 V52 M50 12 V52" stroke="${ART_COLORS.woodDark}" stroke-width="3"/>
    <rect x="9" y="38" width="10" height="7" rx="3.5" fill="${ART_COLORS.wood}"/>
    <rect x="27" y="20" width="10" height="7" rx="3.5" fill="${ART_COLORS.sage}"/>
    <rect x="45" y="30" width="10" height="7" rx="3.5" fill="${ART_COLORS.skin}"/>`,
  emptyTracks: `
    <rect x="6" y="14" width="52" height="10" rx="5" fill="${ART_COLORS.skin}" stroke-dasharray="2 5"/>
    <rect x="6" y="29" width="52" height="10" rx="5" fill="${ART_COLORS.skin}" stroke-dasharray="2 5"/>
    <circle cx="32" cy="50" r="7" fill="${ART_COLORS.wood}"/>
    <path d="M32 46.5 V53.5 M28.5 50 H35.5" stroke-width="2.5"/>`,
  emptySearch: `
    <circle cx="27" cy="27" r="16" fill="${ART_COLORS.skin}"/>
    <path d="M39 39 L55 55" stroke-width="5" stroke="${ART_COLORS.woodDark}"/>
    <path d="M20 31 C24 25 30 25 34 31" stroke-width="2.2"/>
    <circle cx="21.5" cy="23" r="1.6" fill="${ART_COLORS.line}" stroke="none"/><circle cx="32.5" cy="23" r="1.6" fill="${ART_COLORS.line}" stroke="none"/>`,
  note: `
    <path d="M24 46 V14 L46 9 V41" stroke-width="3"/>
    <ellipse cx="18" cy="46" rx="7" ry="5.5" fill="${ART_COLORS.wood}"/><ellipse cx="40" cy="41" rx="7" ry="5.5" fill="${ART_COLORS.sage}"/>`,
};

// The emoji the marketplace backend stores per track -> matching illustration.
const EMOJI_ART = { "🥁": "Madal", "🪕": "Tungna", "🪶": "Bansuri", "🎵": "note", "🎼": "emptyProjects" };

function art(key, size = 64) {
  return ART[key] ? artSvg(ART[key], size) : "";
}

// Illustration for a built-in instrument; unknown/sampled instruments get a note.
function instrumentArt(name, size = 64) {
  return art(ART[name] ? name : "note", size);
}

// Marketplace tracks carry an emoji from the server; show art when we know it,
// otherwise fall back to the emoji itself so nothing renders blank.
function emojiArt(emoji, size = 64) {
  const key = EMOJI_ART[emoji];
  return key ? art(key, size) : emoji ?? "";
}

// Shared markup for every "nothing here yet" state.
function emptyStateHtml(artKey, title, hint = "", size = 72) {
  return `<div class="empty-art">${art(artKey, size)}</div>
    <h3 class="empty-title">${title}</h3>${hint ? `<p class="empty-hint">${hint}</p>` : ""}`;
}

// Fill static markup: <div data-art="compose" data-size="40"></div>
function hydrateArt(root = document) {
  root.querySelectorAll("[data-art]").forEach((node) => {
    node.innerHTML = art(node.dataset.art, Number(node.dataset.size) || 64);
  });
}


// UI icon set: 24x24, stroke follows the surrounding text color (currentColor).
const ICON = {
  check: '<path d="M5 12.5l4.5 4.5L19 7.5"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  minus: '<path d="M5 12h14"/>',
  close: '<path d="M6 6l12 12M18 6L6 18"/>',
  arrowLeft: '<path d="M19 12H5M11 6l-6 6 6 6"/>',
  arrowRight: '<path d="M5 12h14M13 6l6 6-6 6"/>',
  heart: '<path d="M12 20.5C5 15.5 3 12 3 8.8 3 6.2 5 4.5 7.3 4.5c1.9 0 3.4 1 4.7 2.9 1.3-1.9 2.8-2.9 4.7-2.9C19 4.5 21 6.2 21 8.8c0 3.2-2 6.7-9 11.7z"/>',
  heartFilled: '<path fill="currentColor" d="M12 20.5C5 15.5 3 12 3 8.8 3 6.2 5 4.5 7.3 4.5c1.9 0 3.4 1 4.7 2.9 1.3-1.9 2.8-2.9 4.7-2.9C19 4.5 21 6.2 21 8.8c0 3.2-2 6.7-9 11.7z"/>',
  star: '<path fill="currentColor" d="M12 3.5l2.6 5.4 5.9.8-4.3 4.1 1 5.9L12 16.9 6.8 19.7l1-5.9-4.3-4.1 5.9-.8z"/>',
  flag: '<path d="M6 21V4M6 5h11l-2.5 4L17 13H6"/>',
  block: '<circle cx="12" cy="12" r="9"/><path d="M5.6 5.6l12.8 12.8"/>',
  wallet: '<path d="M4 7.5A2.5 2.5 0 0 1 6.5 5H18v3M4 7.5V17a2 2 0 0 0 2 2h13a1 1 0 0 0 1-1V9a1 1 0 0 0-1-1H6.5A2.5 2.5 0 0 1 4 7.5z"/><circle cx="16" cy="13.5" r="1.2" fill="currentColor" stroke="none"/>',
  card: '<rect x="3" y="5.5" width="18" height="13" rx="2.5"/><path d="M3 10h18M7 15h4"/>',
  volume: '<path d="M4 9.5v5h3.5L12 18.5v-13L7.5 9.5z" fill="currentColor"/><path d="M15.5 9a4 4 0 0 1 0 6M18 6.5a7.5 7.5 0 0 1 0 11"/>',
  volumeOff: '<path d="M4 9.5v5h3.5L12 18.5v-13L7.5 9.5z" fill="currentColor"/><path d="M16 9.5l5 5M21 9.5l-5 5"/>',
  headphones: '<path d="M4 15v-3a8 8 0 0 1 16 0v3"/><rect x="3" y="14" width="4.5" height="6.5" rx="2" fill="currentColor"/><rect x="16.5" y="14" width="4.5" height="6.5" rx="2" fill="currentColor"/>',
};

function icon(name, size = 18) {
  return `<svg class="icon" viewBox="0 0 24 24" width="${size}" height="${size}" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${ICON[name] ?? ""}</svg>`;
}

// Icon followed by a text label, e.g. iconLabel("heart", "Save").
function iconLabel(name, text, size = 16) {
  return `${icon(name, size)}<span>${text}</span>`;
}

ART.success = `
  <circle cx="32" cy="32" r="24" fill="${ART_COLORS.sage}"/>
  <path d="M21 33l8 8 15-17" stroke-width="4.5"/>
  <path d="M10 14l3 3M54 12l-3 4M8 44l4-2M55 46l-4-2" stroke="${ART_COLORS.woodDark}" stroke-width="2.2"/>`;

// <span data-icon="check" data-size="16"></span> and <div data-art="..."> both hydrate here.
function hydrateIcons(root = document) {
  root.querySelectorAll("[data-icon]").forEach((node) => {
    node.innerHTML = icon(node.dataset.icon, Number(node.dataset.size) || 18);
  });
}

hydrateArt();
hydrateIcons();
