// The desktop app calls these functions directly (not from a browser page
// under Supabase's own domain), so a permissive CORS response is fine here —
// there's no cookie/session to leak cross-origin, auth is a bearer token the
// caller attaches explicitly.
export const corsHeaders = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Headers": "authorization, x-client-info, apikey, content-type",
};
