// Permanently deletes the caller's account and all associated data.
//
// Runs with the service_role key (only available inside Edge Functions, never
// shipped to the desktop client) so it can call auth.admin.deleteUser — which
// a client can never do. The user is identified from their own JWT, so a
// caller can only ever delete themselves, never someone else's account.
//
// Deleting the auth user cascades through every table (profiles → tracks →
// purchases → wishlists → payout_requests → track_reports → blocked_creators
// all have `on delete cascade` foreign keys), so this one call removes the
// user's whole footprint. Storage objects don't cascade, so we clear those
// first, best-effort.
//
// Deploy:  supabase functions deploy delete-account
// (SUPABASE_URL, SUPABASE_ANON_KEY, and SUPABASE_SERVICE_ROLE_KEY are provided
//  automatically to deployed functions.)
import { createClient } from "jsr:@supabase/supabase-js@2";
import { corsHeaders } from "../_shared/cors.ts";

Deno.serve(async (req) => {
  if (req.method === "OPTIONS") return new Response(null, { headers: corsHeaders });

  try {
    const authHeader = req.headers.get("Authorization");
    if (!authHeader) throw new Error("Missing Authorization header");

    // Identify the caller from their own token — never trust a client-supplied
    // user id.
    const userClient = createClient(
      Deno.env.get("SUPABASE_URL")!,
      Deno.env.get("SUPABASE_ANON_KEY")!,
      { global: { headers: { Authorization: authHeader } } },
    );
    const { data: userData, error: userErr } = await userClient.auth.getUser();
    if (userErr || !userData.user) throw new Error("Not authenticated");
    const userId = userData.user.id;

    const admin = createClient(
      Deno.env.get("SUPABASE_URL")!,
      Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!,
    );

    // Remove this user's storage objects. Files live under a folder named by
    // the user id: track-previews/{userId}/... and track-files/{userId}/...
    for (const bucket of ["track-previews", "track-files"]) {
      const { data: files } = await admin.storage.from(bucket).list(userId);
      if (files && files.length > 0) {
        await admin.storage
          .from(bucket)
          .remove(files.map((f) => `${userId}/${f.name}`));
      }
    }

    // Delete the auth user — cascades to every dependent table.
    const { error: delErr } = await admin.auth.admin.deleteUser(userId);
    if (delErr) throw delErr;

    return new Response(JSON.stringify({ deleted: true }), {
      headers: { ...corsHeaders, "Content-Type": "application/json" },
    });
  } catch (e) {
    return new Response(
      JSON.stringify({ error: e instanceof Error ? e.message : String(e) }),
      { status: 400, headers: { ...corsHeaders, "Content-Type": "application/json" } },
    );
  }
});
