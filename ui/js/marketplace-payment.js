
// ---- Payment (Stripe, embedded — no redirect) ----

async function ensureStripeLoaded() {
  if (stripe) return stripe;
  const config = await invoke("get_public_config");
  if (!config.stripePublishableKey) {
    throw new Error("Stripe isn't configured yet — add STRIPE_PUBLISHABLE_KEY to .env and rebuild.");
  }
  stripe = window.Stripe(config.stripePublishableKey);
  return stripe;
}

function showPaymentError(msg) {
  el("payment-error").textContent = msg;
  el("payment-error").classList.remove("hidden");
}

async function openPaymentFlow(track) {
  if (!paymentsEnabled) {
    showToast("Purchases aren't available in this version — visit the website to buy", "err");
    return;
  }
  paymentTrack = track;
  el("payment-stage-method").classList.remove("hidden");
  el("payment-stage-receipt").classList.add("hidden");
  el("payment-error").classList.add("hidden");
  el("payment-total").textContent = `$${(track.priceCents / 100).toFixed(2)}`;
  el("payment-pay").disabled = false;
  el("payment-pay").textContent = "Pay";
  el("payment-card-element").innerHTML = "";
  el("payment-backdrop").classList.remove("hidden");

  try {
    await ensureStripeLoaded();
    const intent = await invoke("start_stripe_purchase", { trackId: track.id });
    paymentPurchaseId = intent.purchaseId;
    stripeElements = stripe.elements({ clientSecret: intent.clientSecret });
    const cardElement = stripeElements.create("payment");
    cardElement.mount("#payment-card-element");
  } catch (err) {
    showPaymentError(String(err.message ?? err));
  }
}

function closePaymentFlow() {
  el("payment-backdrop").classList.add("hidden");
  stripeElements = null;
  paymentTrack = null;
  paymentPurchaseId = null;
}

function setupPaymentFlow() {
  el("payment-cancel").addEventListener("click", closePaymentFlow);
  el("payment-backdrop").addEventListener("click", (e) => {
    if (e.target.id === "payment-backdrop") closePaymentFlow();
  });
  el("payment-pay").addEventListener("click", async () => {
    if (!stripe || !stripeElements || !paymentTrack) return;
    el("payment-pay").disabled = true;
    el("payment-pay").textContent = "Processing…";
    try {
      const { error } = await stripe.confirmPayment({
        elements: stripeElements,
        redirect: "if_required",
        confirmParams: { return_url: window.location.href },
      });
      if (error) throw new Error(error.message);
      const status = await invoke("finalize_purchase", { purchaseId: paymentPurchaseId });
      if (status.status !== "completed") {
        throw new Error("Payment is still processing — check back shortly.");
      }
      el("payment-stage-method").classList.add("hidden");
      el("payment-stage-receipt").classList.remove("hidden");
      el("payment-receipt-title").textContent = `${paymentTrack.title} — you can download it now`;
    } catch (err) {
      showPaymentError(String(err.message ?? err));
      el("payment-pay").disabled = false;
      el("payment-pay").textContent = "Pay";
    }
  });
  el("payment-download").addEventListener("click", async () => {
    if (!paymentTrack) return;
    await downloadTrackToDisk(paymentTrack);
  });
  el("payment-done").addEventListener("click", () => {
    closePaymentFlow();
    loadExploreTracks().catch(() => {});
  });
}
