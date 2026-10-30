
// ---- Earnings ----

async function loadEarnings() {
  try {
    const summary = await invoke("list_my_earnings");
    renderEarnings(summary);
  } catch (err) {
    showToast(`Couldn't load earnings: ${err}`, "err");
  }
}

function renderEarnings(summary) {
  const totalSales = summary.tracks.reduce((s, t) => s + t.salesCount, 0);
  const totalDownloads = summary.tracks.reduce((s, t) => s + t.downloadsCount, 0);
  el("earnings-kpis").innerHTML = `
    <div class="kpi-card"><div class="kpi-label">Total Earnings</div><div class="kpi-value" style="color:var(--color-accent)">$${(
      summary.totalEarningsCents / 100
    ).toFixed(2)}</div></div>
    <div class="kpi-card" style="border-left-color:var(--color-accent-2)"><div class="kpi-label">Sales</div><div class="kpi-value" style="color:var(--color-accent-2)">${totalSales}</div></div>
    <div class="kpi-card" style="border-left-color:#667eea"><div class="kpi-label">Downloads</div><div class="kpi-value" style="color:#667eea">${totalDownloads}</div></div>
    <div class="kpi-card" style="border-left-color:#f093fb"><div class="kpi-label">Tracks</div><div class="kpi-value" style="color:#f093fb">${summary.tracks.length}</div></div>`;

  const list = el("earnings-tracks");
  list.innerHTML =
    summary.tracks.length === 0
      ? `<p style="opacity:0.7">Nothing uploaded yet.</p>`
      : summary.tracks
          .map(
            (t) => `
      <div class="track-item">
        <div style="flex:1">
          <div style="font-weight:700;font-size:14px">${t.title}</div>
          <div style="font-size:12px;color:var(--color-accent-700);margin-bottom:4px">${t.genre || "—"}</div>
          <div style="font-size:12px">${t.salesCount} sales · ${t.downloadsCount} downloads · ${
              t.priceCents > 0 ? "$" + (t.priceCents / 100).toFixed(2) : "Free"
            }</div>
        </div>
        <div style="text-align:right">
          <div style="font-size:11px;color:var(--color-accent-700)">Earnings</div>
          <div style="font-size:18px;font-weight:700;color:var(--color-accent)">$${(t.earningsCents / 100).toFixed(2)}</div>
        </div>
      </div>`
          )
          .join("");

  el("earnings-available").textContent = `$${(summary.availableCents / 100).toFixed(2)}`;
  el("earnings-request-payout").dataset.available = summary.availableCents;
}

function setupEarningsPage() {
  el("earnings-request-payout").addEventListener("click", async () => {
    const available = parseInt(el("earnings-request-payout").dataset.available || "0", 10);
    if (available <= 0) {
      showToast("No available balance to request.", "err");
      return;
    }
    const input = prompt(`Amount to request (max $${(available / 100).toFixed(2)})`, (available / 100).toFixed(2));
    if (input === null) return;
    const amountCents = Math.round(parseFloat(input) * 100);
    if (!amountCents || amountCents <= 0 || amountCents > available) {
      showToast("Enter a valid amount within your available balance.", "err");
      return;
    }
    try {
      await invoke("request_payout", { amountCents });
      showToast("Payout requested.", "ok");
      await loadEarnings();
    } catch (err) {
      showToast(`Payout request failed: ${err}`, "err");
    }
  });
}
