//! The marketplace: uploading a finished project, browsing/wishlisting other
//! people's, buying paid ones through Stripe, and creators checking their
//! earnings. Everything here talks to Supabase's REST API (PostgREST for the
//! database, its Storage API for files, and its Edge Functions for anything
//! that needs a secret key) the same way `auth.rs` talks to GoTrue — plain
//! `reqwest` calls carrying the signed-in user's own bearer token. No
//! service_role key or payment secret key ever appears in this binary; those
//! live only inside the Edge Functions under `supabase/functions/`, set as
//! Supabase secrets by the user directly (see `supabase/migrations/
//! 0001_marketplace.sql` and the Edge Function sources for the server side of
//! this feature).
//!
//! Schema recap (full detail in the migration file): `tracks` (one row per
//! uploaded project), `purchases` (written only by the Stripe Edge
//! Functions — no client insert policy, so a client can never fabricate a
//! completed purchase), `wishlists`, `payout_requests`, and a
//! `creator_earnings` view computed from `purchases` rather than a ledger
//! that could drift out of sync.

use std::path::PathBuf;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::AppHandle;

use composer_core::engine::render_song;
use composer_core::model::Song;

use crate::auth;

const PREVIEW_SECONDS: f32 = 30.0;
const PREVIEW_SAMPLE_RATE: f32 = 44_100.0;
// The actual 20%/80% commission split only lives in
// supabase/functions/stripe-create-payment-intent (the only place real money
// changes hands) — app.js's Upload flow shows the same 80% figure as a
// preview, but that's cosmetic and never the source of truth.

/// A small fixed palette so uploaded tracks get some visual variety in
/// Explore without inventing a whole thumbnail/artwork upload flow the user
/// never asked for — picked from the same set already used in the imported
/// mockups' sample data.
const PALETTE: &[(&str, &str, &str)] = &[
    ("🎵", "#667eea", "#764ba2"),
    ("🎸", "#a1c4fd", "#c2e9fb"),
    ("🥁", "#fa709a", "#fee140"),
    ("🎻", "#a8edea", "#fed6e3"),
    ("🎤", "#30cfd0", "#330867"),
    ("✨", "#f093fb", "#f5576c"),
];

fn pick_palette(seed: &str) -> (&'static str, &'static str, &'static str) {
    let idx = seed.bytes().map(|b| b as usize).sum::<usize>() % PALETTE.len();
    PALETTE[idx]
}

fn auth_headers(access_token: &str, anon_key: &str) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "apikey",
        HeaderValue::from_str(anon_key).map_err(|e| e.to_string())?,
    );
    headers.insert(
        "Authorization",
        HeaderValue::from_str(&format!("Bearer {access_token}")).map_err(|e| e.to_string())?,
    );
    Ok(headers)
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PublicConfig {
    pub supabase_url: String,
    pub supabase_anon_key: String,
    pub stripe_publishable_key: String,
    /// Whether in-app paid purchases and seller payouts are available. Off on
    /// Android/iOS: Google Play (and the App Store) require digital in-app
    /// goods to go through their own billing, which Sargam's Stripe flow does
    /// not use — so the mobile build hides all buy/sell-for-money/payout UI and
    /// ships marketplace browsing + free downloads only. Desktop keeps them.
    pub payments_enabled: bool,
}

/// The frontend's one source for the values it needs to talk to Supabase/
/// Stripe directly (loading `stripe.js` and mounting Payment Element) —
/// these are all meant to be public client-side values (see the Supabase/
/// Stripe credential-tier reasoning in `auth.rs` and CLAUDE.md), just kept
/// in one place instead of duplicated into static JS.
#[tauri::command]
pub fn get_public_config() -> PublicConfig {
    let (url, anon_key) = auth::config().unwrap_or(("", ""));
    PublicConfig {
        supabase_url: url.to_string(),
        supabase_anon_key: anon_key.to_string(),
        stripe_publishable_key: option_env!("STRIPE_PUBLISHABLE_KEY")
            .unwrap_or("")
            .to_string(),
        // Desktop only — mobile app stores forbid non-store billing for
        // digital goods (see the field docs).
        payments_enabled: cfg!(not(any(target_os = "android", target_os = "ios"))),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: String,
    /// The seller's profile/user id — the frontend needs it to block a creator.
    #[serde(default)]
    pub creator_id: String,
    pub title: String,
    pub description: String,
    pub genre: String,
    pub price_cents: i64,
    pub emoji: String,
    pub gradient_start: String,
    pub gradient_end: String,
    pub rating_count: i64,
    #[serde(default)]
    pub rating_average: f32,
    pub downloads_count: i64,
    #[serde(default)]
    pub creator_name: String,
    #[serde(default)]
    pub preview_url: String,
}

/// Raw shape PostgREST hands back (`profiles` embedded as a nested object,
/// `rating_sum`/`rating_count` not yet averaged, no preview URL built yet) —
/// converted to the frontend-facing `Track` in `into_track` below.
#[derive(Debug, Clone, Deserialize)]
struct RawTrack {
    id: String,
    creator_id: String,
    title: String,
    description: String,
    genre: String,
    price_cents: i64,
    emoji: String,
    gradient_start: String,
    gradient_end: String,
    rating_sum: i64,
    rating_count: i64,
    downloads_count: i64,
    preview_path: String,
    #[serde(default)]
    profiles: Option<ProfileEmbed>,
}

#[derive(Debug, Clone, Deserialize)]
struct ProfileEmbed {
    display_name: String,
}

fn into_track(raw: RawTrack, supabase_url: &str) -> Track {
    Track {
        rating_average: if raw.rating_count > 0 {
            raw.rating_sum as f32 / raw.rating_count as f32
        } else {
            0.0
        },
        creator_name: raw
            .profiles
            .map(|p| p.display_name)
            .unwrap_or_else(|| "Unknown".to_string()),
        preview_url: format!(
            "{supabase_url}/storage/v1/object/public/track-previews/{}",
            raw.preview_path
        ),
        id: raw.id,
        creator_id: raw.creator_id,
        title: raw.title,
        description: raw.description,
        genre: raw.genre,
        price_cents: raw.price_cents,
        emoji: raw.emoji,
        gradient_start: raw.gradient_start,
        gradient_end: raw.gradient_end,
        rating_count: raw.rating_count,
        downloads_count: raw.downloads_count,
    }
}

/// Creates (or refreshes the display name on) this user's `profiles` row.
/// Needed before their first upload — `tracks.creator_id` has a foreign key
/// into `profiles`, not straight into `auth.users`, specifically so
/// PostgREST can embed the creator's name in one request (see the migration
/// file's comment on that column).
async fn ensure_profile(app: &AppHandle) -> Result<auth::StoredSession, String> {
    let session = auth::require_session(app).await?;
    let (url, anon_key) = auth::config()?;
    let headers = auth_headers(&session.access_token, anon_key)?;
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{url}/rest/v1/profiles"))
        .headers(headers)
        .header("Prefer", "resolution=merge-duplicates")
        .json(&json!({ "id": session.user_id, "display_name": session.name }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!(
            "Couldn't set up your creator profile: {}",
            resp.text().await.unwrap_or_default()
        ));
    }
    Ok(session)
}

#[tauri::command]
pub async fn explore_tracks(
    app: AppHandle,
    search: Option<String>,
    genre: Option<String>,
    price_filter: Option<String>,
    sort: String,
) -> Result<Vec<Track>, String> {
    let (url, anon_key) = auth::config()?;
    // Browsing doesn't strictly need to be signed in for Supabase's RLS
    // (live tracks are publicly readable), but the app requires login before
    // showing this page at all per the product decision — reuse whatever
    // session exists if there is one, anon key alone otherwise.
    let bearer = auth::require_session(&app)
        .await
        .map(|s| s.access_token)
        .unwrap_or_else(|_| anon_key.to_string());
    let headers = auth_headers(&bearer, anon_key)?;
    let blocked = blocked_creator_ids(&app).await;

    let mut request_url =
        reqwest::Url::parse(&format!("{url}/rest/v1/tracks")).map_err(|e| e.to_string())?;
    {
        let mut q = request_url.query_pairs_mut();
        q.append_pair("select", "*,profiles(display_name)");
        q.append_pair("status", "eq.live");
        // Hide tracks from creators this user has blocked.
        if !blocked.is_empty() {
            q.append_pair("creator_id", &format!("not.in.({})", blocked.join(",")));
        }
        if let Some(g) = genre.filter(|g| !g.is_empty()) {
            q.append_pair("genre", &format!("eq.{g}"));
        }
        match price_filter.as_deref() {
            Some("free") => {
                q.append_pair("price_cents", "eq.0");
            }
            Some("paid") => {
                q.append_pair("price_cents", "gt.0");
            }
            _ => {}
        }
        if let Some(term) = search.filter(|s| !s.is_empty()) {
            q.append_pair(
                "or",
                &format!("(title.ilike.*{term}*,description.ilike.*{term}*)"),
            );
        }
        q.append_pair("order", "created_at.desc");
    }

    let client = reqwest::Client::new();
    let resp = client
        .get(request_url)
        .headers(headers)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!(
            "Couldn't load the marketplace: {}",
            resp.text().await.unwrap_or_default()
        ));
    }
    let mut raw: Vec<RawTrack> = resp.json().await.map_err(|e| e.to_string())?;

    match sort.as_str() {
        "popular" => raw.sort_by(|a, b| b.downloads_count.cmp(&a.downloads_count)),
        "rating" => raw.sort_by(|a, b| {
            let avg = |t: &RawTrack| {
                if t.rating_count > 0 {
                    t.rating_sum as f32 / t.rating_count as f32
                } else {
                    0.0
                }
            };
            avg(b)
                .partial_cmp(&avg(a))
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        _ => {} // already ordered by created_at.desc from PostgREST
    }

    Ok(raw.into_iter().map(|t| into_track(t, url)).collect())
}

#[tauri::command]
pub async fn list_wishlist(app: AppHandle) -> Result<Vec<Track>, String> {
    let session = auth::require_session(&app).await?;
    let (url, anon_key) = auth::config()?;
    let headers = auth_headers(&session.access_token, anon_key)?;

    let mut request_url =
        reqwest::Url::parse(&format!("{url}/rest/v1/wishlists")).map_err(|e| e.to_string())?;
    request_url
        .query_pairs_mut()
        .append_pair("select", "tracks(*,profiles(display_name))")
        .append_pair("buyer_id", &format!("eq.{}", session.user_id));

    let client = reqwest::Client::new();
    let resp = client
        .get(request_url)
        .headers(headers)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!(
            "Couldn't load your wishlist: {}",
            resp.text().await.unwrap_or_default()
        ));
    }

    #[derive(Deserialize)]
    struct WishlistRow {
        tracks: RawTrack,
    }
    let rows: Vec<WishlistRow> = resp.json().await.map_err(|e| e.to_string())?;
    let blocked = blocked_creator_ids(&app).await;
    Ok(rows
        .into_iter()
        .map(|r| into_track(r.tracks, url))
        .filter(|t| !blocked.contains(&t.creator_id))
        .collect())
}

/// Returns the new saved state (`true` = now on the wishlist).
#[tauri::command]
pub async fn toggle_wishlist(app: AppHandle, track_id: String) -> Result<bool, String> {
    let session = auth::require_session(&app).await?;
    let (url, anon_key) = auth::config()?;
    let headers = auth_headers(&session.access_token, anon_key)?;
    let client = reqwest::Client::new();

    let mut check_url =
        reqwest::Url::parse(&format!("{url}/rest/v1/wishlists")).map_err(|e| e.to_string())?;
    check_url
        .query_pairs_mut()
        .append_pair("select", "track_id")
        .append_pair("buyer_id", &format!("eq.{}", session.user_id))
        .append_pair("track_id", &format!("eq.{track_id}"));
    let existing: Vec<Value> = client
        .get(check_url)
        .headers(headers.clone())
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    if existing.is_empty() {
        let resp = client
            .post(format!("{url}/rest/v1/wishlists"))
            .headers(headers)
            .json(&json!({ "buyer_id": session.user_id, "track_id": track_id }))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!(
                "Couldn't save to wishlist: {}",
                resp.text().await.unwrap_or_default()
            ));
        }
        Ok(true)
    } else {
        let mut del_url =
            reqwest::Url::parse(&format!("{url}/rest/v1/wishlists")).map_err(|e| e.to_string())?;
        del_url
            .query_pairs_mut()
            .append_pair("buyer_id", &format!("eq.{}", session.user_id))
            .append_pair("track_id", &format!("eq.{track_id}"));
        let resp = client
            .delete(del_url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!(
                "Couldn't remove from wishlist: {}",
                resp.text().await.unwrap_or_default()
            ));
        }
        Ok(false)
    }
}

// ---- Moderation: report & block ------------------------------------------
//
// User-generated content (uploaded tracks) means the app stores require an
// in-app way to report objectionable content and to block a creator. Reports
// land in `track_reports`; blocks in `blocked_creators`, and blocked creators'
// tracks are filtered out of Explore/Wishlist below. See
// supabase/migrations/0002_moderation.sql for the tables + RLS.

/// The creator ids the signed-in user has blocked. Returns empty (not an
/// error) when signed out or on any failure, so browsing degrades gracefully.
async fn blocked_creator_ids(app: &AppHandle) -> Vec<String> {
    let Ok(session) = auth::require_session(app).await else {
        return Vec::new();
    };
    let Ok((url, anon_key)) = auth::config() else {
        return Vec::new();
    };
    let Ok(headers) = auth_headers(&session.access_token, anon_key) else {
        return Vec::new();
    };
    let Ok(mut req) = reqwest::Url::parse(&format!("{url}/rest/v1/blocked_creators")) else {
        return Vec::new();
    };
    req.query_pairs_mut()
        .append_pair("select", "creator_id")
        .append_pair("user_id", &format!("eq.{}", session.user_id));
    let client = reqwest::Client::new();
    let Ok(resp) = client.get(req).headers(headers).send().await else {
        return Vec::new();
    };
    #[derive(Deserialize)]
    struct Row {
        creator_id: String,
    }
    let rows: Vec<Row> = resp.json().await.unwrap_or_default();
    rows.into_iter().map(|r| r.creator_id).collect()
}

/// Report a track for violating the content policy.
#[tauri::command]
pub async fn report_track(
    app: AppHandle,
    track_id: String,
    reason: String,
    details: Option<String>,
) -> Result<(), String> {
    let session = auth::require_session(&app).await?;
    let (url, anon_key) = auth::config()?;
    let headers = auth_headers(&session.access_token, anon_key)?;
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{url}/rest/v1/track_reports"))
        .headers(headers)
        .json(&json!({
            "track_id": track_id,
            "reporter_id": session.user_id,
            "reason": reason,
            "details": details.unwrap_or_default(),
        }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!(
            "Couldn't submit the report: {}",
            resp.text().await.unwrap_or_default()
        ));
    }
    Ok(())
}

/// Block a creator so their tracks stop appearing for this user.
#[tauri::command]
pub async fn block_creator(app: AppHandle, creator_id: String) -> Result<(), String> {
    let session = auth::require_session(&app).await?;
    let (url, anon_key) = auth::config()?;
    let headers = auth_headers(&session.access_token, anon_key)?;
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{url}/rest/v1/blocked_creators"))
        .headers(headers)
        // Blocking twice is a no-op, not an error.
        .header("Prefer", "resolution=merge-duplicates")
        .json(&json!({ "user_id": session.user_id, "creator_id": creator_id }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!(
            "Couldn't block this creator: {}",
            resp.text().await.unwrap_or_default()
        ));
    }
    Ok(())
}

/// Undo a block.
#[tauri::command]
pub async fn unblock_creator(app: AppHandle, creator_id: String) -> Result<(), String> {
    let session = auth::require_session(&app).await?;
    let (url, anon_key) = auth::config()?;
    let headers = auth_headers(&session.access_token, anon_key)?;
    let client = reqwest::Client::new();
    let mut del = reqwest::Url::parse(&format!("{url}/rest/v1/blocked_creators"))
        .map_err(|e| e.to_string())?;
    del.query_pairs_mut()
        .append_pair("user_id", &format!("eq.{}", session.user_id))
        .append_pair("creator_id", &format!("eq.{creator_id}"));
    let resp = client
        .delete(del)
        .headers(headers)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!(
            "Couldn't unblock: {}",
            resp.text().await.unwrap_or_default()
        ));
    }
    Ok(())
}

/// The creator ids this user has blocked (for a management/undo UI).
#[tauri::command]
pub async fn list_blocked_creators(app: AppHandle) -> Result<Vec<String>, String> {
    Ok(blocked_creator_ids(&app).await)
}

/// Renders `song_path`, uploads a 30-second preview + the full project file,
/// and creates the marketplace listing. `sample_rate`/preview-trimming logic
/// lives here rather than in composer-core because it's an app-shell/
/// marketplace concern, not core music logic — the same reasoning
/// `write_wav_with_depth` vs. the disabled MP3 button already documents for
/// what belongs in the engine vs. the desktop shell.
#[tauri::command]
pub async fn upload_track(
    app: AppHandle,
    song_path: PathBuf,
    title: String,
    description: String,
    genre: String,
    price_cents: i64,
) -> Result<Track, String> {
    if title.trim().is_empty() {
        return Err("Give your track a title first.".into());
    }
    if price_cents < 0 {
        return Err("Price can't be negative.".into());
    }

    let session = ensure_profile(&app).await?;
    let (url, anon_key) = auth::config()?;

    let song = Song::load(&song_path).map_err(|e| e.to_string())?;
    let full_audio = render_song(&song, PREVIEW_SAMPLE_RATE).map_err(|e| e.to_string())?;
    let preview_len = ((PREVIEW_SECONDS * PREVIEW_SAMPLE_RATE) as usize * 2).min(full_audio.len());
    let preview_audio = &full_audio[..preview_len];

    let preview_wav_bytes = {
        let tmp = std::env::temp_dir().join(format!("sargam-preview-{}.wav", uuid::Uuid::new_v4()));
        composer_core::engine::write_wav(&tmp, preview_audio, 2, PREVIEW_SAMPLE_RATE as u32)
            .map_err(|e| e.to_string())?;
        let bytes = std::fs::read(&tmp).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&tmp);
        bytes
    };
    let project_bytes = song.to_json().into_bytes();

    let track_id = uuid::Uuid::new_v4().to_string();
    let preview_path = format!("{}/{track_id}-preview.wav", session.user_id);
    let project_path = format!("{}/{track_id}.song.json", session.user_id);

    let client = reqwest::Client::new();
    let storage_headers = auth_headers(&session.access_token, anon_key)?;

    let preview_resp = client
        .post(format!(
            "{url}/storage/v1/object/track-previews/{preview_path}"
        ))
        .headers(storage_headers.clone())
        .header("Content-Type", "audio/wav")
        .body(preview_wav_bytes)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !preview_resp.status().is_success() {
        return Err(format!(
            "Couldn't upload the preview: {}",
            preview_resp.text().await.unwrap_or_default()
        ));
    }

    let project_resp = client
        .post(format!(
            "{url}/storage/v1/object/track-files/{project_path}"
        ))
        .headers(storage_headers)
        .header("Content-Type", "application/json")
        .body(project_bytes)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !project_resp.status().is_success() {
        return Err(format!(
            "Couldn't upload the project file: {}",
            project_resp.text().await.unwrap_or_default()
        ));
    }

    let (emoji, gradient_start, gradient_end) = pick_palette(&track_id);
    let row_headers = auth_headers(&session.access_token, anon_key)?;
    let insert_resp = client
        .post(format!("{url}/rest/v1/tracks"))
        .headers(row_headers)
        .header("Prefer", "return=representation")
        .json(&json!({
            "id": track_id,
            "creator_id": session.user_id,
            "title": title,
            "description": description,
            "genre": genre,
            "price_cents": price_cents,
            "preview_path": preview_path,
            "project_path": project_path,
            "emoji": emoji,
            "gradient_start": gradient_start,
            "gradient_end": gradient_end,
        }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !insert_resp.status().is_success() {
        return Err(format!(
            "Couldn't list the track: {}",
            insert_resp.text().await.unwrap_or_default()
        ));
    }
    let mut rows: Vec<RawTrack> = insert_resp.json().await.map_err(|e| e.to_string())?;
    let raw = rows
        .pop()
        .ok_or("Upload succeeded but no track was returned")?;
    Ok(into_track(raw, url))
}

/// `Ok(())` on success; the storage RLS policy itself is what turns an
/// unpurchased paid track into a rejection, which we translate into a
/// message that points the user at buying it instead of a raw 403/permission
/// error.
#[tauri::command]
pub async fn download_track(
    app: AppHandle,
    track_id: String,
    dest_path: PathBuf,
) -> Result<(), String> {
    let session = auth::require_session(&app).await?;
    let (url, anon_key) = auth::config()?;
    let headers = auth_headers(&session.access_token, anon_key)?;
    let client = reqwest::Client::new();

    let mut track_url =
        reqwest::Url::parse(&format!("{url}/rest/v1/tracks")).map_err(|e| e.to_string())?;
    track_url
        .query_pairs_mut()
        .append_pair("select", "project_path")
        .append_pair("id", &format!("eq.{track_id}"));
    let rows: Vec<Value> = client
        .get(track_url)
        .headers(headers.clone())
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let project_path = rows
        .first()
        .and_then(|r| r.get("project_path"))
        .and_then(|p| p.as_str())
        .ok_or("Track not found")?;

    let resp = client
        .get(format!(
            "{url}/storage/v1/object/track-files/{project_path}"
        ))
        .headers(headers)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.status() == reqwest::StatusCode::FORBIDDEN
        || resp.status() == reqwest::StatusCode::UNAUTHORIZED
    {
        return Err("Purchase this track first to download it.".into());
    }
    if !resp.status().is_success() {
        return Err(format!(
            "Download failed: {}",
            resp.text().await.unwrap_or_default()
        ));
    }
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    std::fs::write(&dest_path, bytes).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StripeIntent {
    pub client_secret: String,
    pub purchase_id: String,
}

#[tauri::command]
pub async fn start_stripe_purchase(
    app: AppHandle,
    track_id: String,
) -> Result<StripeIntent, String> {
    let session = auth::require_session(&app).await?;
    let (url, anon_key) = auth::config()?;
    let headers = auth_headers(&session.access_token, anon_key)?;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{url}/functions/v1/stripe-create-payment-intent"))
        .headers(headers)
        .json(&json!({ "track_id": track_id }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let body: Value = resp.json().await.map_err(|e| e.to_string())?;
    if let Some(err) = body.get("error").and_then(|e| e.as_str()) {
        return Err(err.to_string());
    }
    Ok(StripeIntent {
        client_secret: body
            .get("client_secret")
            .and_then(|v| v.as_str())
            .ok_or("Stripe didn't return a client secret")?
            .to_string(),
        purchase_id: body
            .get("purchase_id")
            .and_then(|v| v.as_str())
            .ok_or("Stripe didn't return a purchase id")?
            .to_string(),
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseStatus {
    pub status: String,
}

/// Called right after `stripe.confirmCardPayment()` resolves client-side.
/// The webhook usually lands within a second, so this polls the `purchases`
/// row briefly rather than trusting the client-side confirmation alone —
/// Stripe's own guidance is that the webhook, not the client SDK result, is
/// the source of truth for whether a payment actually completed.
#[tauri::command]
pub async fn finalize_purchase(
    app: AppHandle,
    purchase_id: String,
) -> Result<PurchaseStatus, String> {
    let session = auth::require_session(&app).await?;
    let (url, anon_key) = auth::config()?;
    let headers = auth_headers(&session.access_token, anon_key)?;
    let client = reqwest::Client::new();

    for _ in 0..10 {
        let mut req_url =
            reqwest::Url::parse(&format!("{url}/rest/v1/purchases")).map_err(|e| e.to_string())?;
        req_url
            .query_pairs_mut()
            .append_pair("select", "status")
            .append_pair("id", &format!("eq.{purchase_id}"));
        let rows: Vec<Value> = client
            .get(req_url)
            .headers(headers.clone())
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        if let Some(status) = rows
            .first()
            .and_then(|r| r.get("status"))
            .and_then(|s| s.as_str())
        {
            if status != "pending" {
                return Ok(PurchaseStatus {
                    status: status.to_string(),
                });
            }
        }
        // Deliberate short blocking sleep, not tokio::time::sleep — this
        // command is bounded to ~5s total and doesn't warrant pulling tokio
        // in as a direct dependency just for this one poll loop.
        std::thread::sleep(Duration::from_millis(500));
    }
    Ok(PurchaseStatus {
        status: "pending".to_string(),
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackEarnings {
    pub track_id: String,
    pub title: String,
    pub genre: String,
    pub price_cents: i64,
    pub earnings_cents: i64,
    pub sales_count: i64,
    pub downloads_count: i64,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EarningsSummary {
    pub tracks: Vec<TrackEarnings>,
    pub total_earnings_cents: i64,
    pub requested_or_paid_cents: i64,
    pub available_cents: i64,
}

#[tauri::command]
pub async fn list_my_earnings(app: AppHandle) -> Result<EarningsSummary, String> {
    let session = auth::require_session(&app).await?;
    let (url, anon_key) = auth::config()?;
    let headers = auth_headers(&session.access_token, anon_key)?;
    let client = reqwest::Client::new();

    #[derive(Deserialize)]
    struct EarningsRow {
        track_id: String,
        title: String,
        genre: String,
        price_cents: i64,
        earnings_cents: i64,
        sales_count: i64,
        downloads_count: i64,
    }
    let mut earnings_url = reqwest::Url::parse(&format!("{url}/rest/v1/creator_earnings"))
        .map_err(|e| e.to_string())?;
    earnings_url
        .query_pairs_mut()
        .append_pair("creator_id", &format!("eq.{}", session.user_id));
    let rows: Vec<EarningsRow> = client
        .get(earnings_url)
        .headers(headers.clone())
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let total_earnings_cents: i64 = rows.iter().map(|r| r.earnings_cents).sum();

    let mut payout_url = reqwest::Url::parse(&format!("{url}/rest/v1/payout_requests"))
        .map_err(|e| e.to_string())?;
    payout_url
        .query_pairs_mut()
        .append_pair("select", "amount_cents,status")
        .append_pair("creator_id", &format!("eq.{}", session.user_id))
        .append_pair("status", "neq.rejected");
    let payouts: Vec<Value> = client
        .get(payout_url)
        .headers(headers)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let requested_or_paid_cents: i64 = payouts
        .iter()
        .filter_map(|p| p.get("amount_cents").and_then(|a| a.as_i64()))
        .sum();

    Ok(EarningsSummary {
        tracks: rows
            .into_iter()
            .map(|r| TrackEarnings {
                track_id: r.track_id,
                title: r.title,
                genre: r.genre,
                price_cents: r.price_cents,
                earnings_cents: r.earnings_cents,
                sales_count: r.sales_count,
                downloads_count: r.downloads_count,
            })
            .collect(),
        total_earnings_cents,
        requested_or_paid_cents,
        available_cents: (total_earnings_cents - requested_or_paid_cents).max(0),
    })
}

#[tauri::command]
pub async fn request_payout(app: AppHandle, amount_cents: i64) -> Result<(), String> {
    if amount_cents <= 0 {
        return Err("Enter an amount to request.".into());
    }
    let session = auth::require_session(&app).await?;
    let (url, anon_key) = auth::config()?;
    let headers = auth_headers(&session.access_token, anon_key)?;
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{url}/rest/v1/payout_requests"))
        .headers(headers)
        .json(&json!({ "creator_id": session.user_id, "amount_cents": amount_cents }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!(
            "Couldn't submit payout request: {}",
            resp.text().await.unwrap_or_default()
        ));
    }
    Ok(())
}
