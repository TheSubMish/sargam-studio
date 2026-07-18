// Stripe calls this directly (not an authenticated Supabase user), so it must
// be deployed with `supabase functions deploy stripe-webhook --no-verify-jwt`
// and instead verifies Stripe's own signature below — anyone who could skip
// that check could mark arbitrary purchases "completed" for free.
import { createClient } from "jsr:@supabase/supabase-js@2";

const STRIPE_WEBHOOK_SECRET = Deno.env.get("STRIPE_WEBHOOK_SECRET");

async function verifyStripeSignature(rawBody: string, signatureHeader: string, secret: string) {
  const parts = Object.fromEntries(
    signatureHeader.split(",").map((kv) => kv.split("=") as [string, string]),
  );
  const timestamp = parts["t"];
  const expectedSig = parts["v1"];
  if (!timestamp || !expectedSig) return false;

  const key = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(secret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const signed = `${timestamp}.${rawBody}`;
  const sigBytes = await crypto.subtle.sign("HMAC", key, new TextEncoder().encode(signed));
  const computedSig = Array.from(new Uint8Array(sigBytes))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
  return computedSig === expectedSig;
}

Deno.serve(async (req) => {
  try {
    if (!STRIPE_WEBHOOK_SECRET) {
      throw new Error("STRIPE_WEBHOOK_SECRET is not set — run `supabase secrets set STRIPE_WEBHOOK_SECRET=whsec_...`");
    }
    const signatureHeader = req.headers.get("Stripe-Signature");
    if (!signatureHeader) throw new Error("Missing Stripe-Signature header");

    const rawBody = await req.text();
    const valid = await verifyStripeSignature(rawBody, signatureHeader, STRIPE_WEBHOOK_SECRET);
    if (!valid) throw new Error("Invalid Stripe signature");

    const event = JSON.parse(rawBody);

    if (event.type === "payment_intent.succeeded") {
      const intent = event.data.object;
      const purchaseId = intent.metadata?.purchase_id;
      if (purchaseId) {
        const admin = createClient(
          Deno.env.get("SUPABASE_URL")!,
          Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!,
        );
        const { data: purchase } = await admin
          .from("purchases")
          .update({ status: "completed", completed_at: new Date().toISOString() })
          .eq("id", purchaseId)
          .eq("status", "pending") // idempotent: a retried webhook delivery is a no-op the second time
          .select()
          .single();
        if (purchase) {
          // downloads_count is a display nicety, not the source of truth for
          // earnings (that view is computed straight from `purchases`), so a
          // plain read-then-write here is fine — it can't double-count since
          // the `eq("status", "pending")` guard above already made this
          // block run at most once per purchase.
          const { data: track } = await admin
            .from("tracks")
            .select("downloads_count")
            .eq("id", purchase.track_id)
            .single();
          if (track) {
            await admin
              .from("tracks")
              .update({ downloads_count: track.downloads_count + 1 })
              .eq("id", purchase.track_id);
          }
        }
      }
    }

    return new Response(JSON.stringify({ received: true }), {
      headers: { "Content-Type": "application/json" },
    });
  } catch (err) {
    return new Response(JSON.stringify({ error: String(err.message ?? err) }), {
      status: 400,
      headers: { "Content-Type": "application/json" },
    });
  }
});
