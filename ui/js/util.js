
const el = (id) => document.getElementById(id);

function debounce(fn, ms) {
  let timer;
  return (...args) => {
    clearTimeout(timer);
    timer = setTimeout(() => fn(...args), ms);
  };
}

let toastTimer = null;
function showToast(message, kind) {
  const toast = el("toast");
  toast.textContent = message;
  toast.className = "show " + kind;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => toast.classList.remove("show"), 4000);
}

function cssVar(name) {
  return getComputedStyle(document.body).getPropertyValue(name).trim();
}

function toDevanagari(n) {
  return String(n)
    .split("")
    .map((d) => DEVANAGARI_DIGITS[+d] ?? d)
    .join("");
}

function midiName(midi) {
  const m = Math.round(midi);
  return `${NOTE_NAMES[((m % 12) + 12) % 12]}${Math.floor(m / 12) - 1}`;
}

function isPercussion(instrumentName) {
  return PERCUSSION_INSTRUMENTS.has(instrumentName);
}

function newSong() {
  return {
    title: "Untitled",
    tempo: { changes: [{ tick: 0, bpm: 100.0 }] },
    master_gain: 1.0,
    tracks: [],
  };
}
