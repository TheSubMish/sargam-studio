// Creates a Stripe PaymentIntent for a single track purchase and records a
// `pending` row in `purchases`. Runs with the service_role key (only
// available inside Edge Functions, never shipped to the desktop client) so it
// can write `purchases` even though clients have no insert policy on that
// table — see supabase/migrations/0001_marketplace.sql for why.
//
// Talks to Stripe via plain fetch + its REST API rather than pulling in the
// Stripe SDK — same "hand-roll the REST call" choice composer-desktop's
// auth.rs already made for Supabase's own Auth API, so there's one fewer
// dependency to trust.
import { createClient } from "jsr:@supabase/supabase-js@2";
import { corsHeaders } from "../_shared/cors.ts";

const STRIPE_SECRET_KEY = Deno.env.get("STRIPE_SECRET_KEY");
const COMMISSION_BPS = 2000; // 20% platform commission, 80% to the creator.

Deno.serve(async (req) => {
  if (req.method === "OPTIONS") return new Response(null, { headers: corsHeaders });

  try {
    if (!STRIPE_SECRET_KEY) {
      throw new Error("STRIPE_SECRET_KEY is not set — run `supabase secrets set STRIPE_SECRET_KEY=sk_test_...`");
    }

    const authHeader = req.headers.get("Authorization");
    if (!authHeader) throw new Error("Missing Authorization header");

    // Identify the buyer from their own JWT — never trust a client-supplied
    // buyer id, since anyone could otherwise attribute a purchase to someone
    // else's account.
    const userClient = createClient(
      Deno.env.get("SUPABASE_URL")!,
      Deno.env.get("SUPABASE_ANON_KEY")!,
      { global: { headers: { Authorization: authHeader } } },
    );
    const { data: userData, error: userErr } = await userClient.auth.getUser();
    if (userErr || !userData.user) throw new Error("Not authenticated");
    const buyerId = userData.user.id;

    const { track_id } = await req.json();
    if (!track_id) throw new Error("track_id is required");

    const admin = createClient(
      Deno.env.get("SUPABASE_URL")!,
      Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!,
    );

    const { data: track, error: trackErr } = await admin
      .from("tracks")
      .select("id, price_cents, status")
      .eq("id", track_id)
      .single();
    if (trackErr || !track) throw new Error("Track not found");
    if (track.status !== "live") throw new Error("This track is no longer available");
    if (track.price_cents <= 0) throw new Error("This track is free — no payment needed");

    const amountCents = track.price_cents;
    const platformFeeCents = Math.round((amountCents * COMMISSION_BPS) / 10000);
    const creatorEarningsCents = amountCents - platformFeeCents;

    const { data: purchase, error: purchaseErr } = await admin
      .from("purchases")
      .insert({
        track_id,
        buyer_id: buyerId,
        amount_cents: amountCents,
        platform_fee_cents: platformFeeCents,
        creator_earnings_cents: creatorEarningsCents,
        provider: "stripe",
        status: "pending",
      })
      .select()
      .single();
    if (purchaseErr || !purchase) throw new Error(`Couldn't record purchase: ${purchaseErr?.message}`);

    const stripeResp = await fetch("https://api.stripe.com/v1/payment_intents", {
      method: "POST",
      headers: {
        Authorization: `Bearer ${STRIPE_SECRET_KEY}`,
        "Content-Type": "application/x-www-form-urlencoded",
      },
      body: new URLSearchParams({
        amount: String(amountCents),
        currency: "usd",
        "metadata[purchase_id]": purchase.id,
        "metadata[track_id]": track_id,
      }),
    });
    const stripeData = await stripeResp.json();
    if (!stripeResp.ok) {
      await admin.from("purchases").update({ status: "failed" }).eq("id", purchase.id);
      throw new Error(stripeData.error?.message ?? "Stripe rejected the payment intent");
    }

    await admin
      .from("purchases")
      .update({ provider_reference: stripeData.id })
      .eq("id", purchase.id);

    return new Response(
      JSON.stringify({ client_secret: stripeData.client_secret, purchase_id: purchase.id }),
      { headers: { ...corsHeaders, "Content-Type": "application/json" } },
    );
  } catch (err) {
    return new Response(JSON.stringify({ error: String(err.message ?? err) }), {
      status: 400,
      headers: { ...corsHeaders, "Content-Type": "application/json" },
    });
  }
});
