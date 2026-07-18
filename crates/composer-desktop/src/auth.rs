//! Real Supabase Auth (GoTrue) — genuine signup/login/logout against
//! Supabase's hosted auth service over HTTPS. This replaces an earlier
//! local-only sha256 password check now that a real backend exists; see
//! CLAUDE.md's Supabase section for the credential-handling reasoning this
//! depends on.
//!
//! `SUPABASE_URL`/`SUPABASE_PUBLISHABLE_KEY` are baked in at build time from
//! a git-ignored `.env` (see `build.rs`) via `option_env!` — never hardcoded,
//! and this file never sees the database password or a service_role key;
//! those must never enter this app at all (they'd bypass Supabase's Row
//! Level Security, unlike the publishable key, which is meant to be public).

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

const SUPABASE_URL: Option<&str> = option_env!("SUPABASE_URL");
const SUPABASE_PUBLISHABLE_KEY: Option<&str> = option_env!("SUPABASE_PUBLISHABLE_KEY");

pub(crate) fn config() -> Result<(&'static str, &'static str), String> {
    match (SUPABASE_URL, SUPABASE_PUBLISHABLE_KEY) {
        (Some(url), Some(key)) if !url.is_empty() && !key.is_empty() => Ok((url, key)),
        _ => Err(
            "Supabase isn't configured — copy .env.example to .env at the repo root, fill in \
             SUPABASE_URL and SUPABASE_PUBLISHABLE_KEY, and rebuild."
                .into(),
        ),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct StoredSession {
    pub(crate) access_token: String,
    refresh_token: String,
    /// Unix epoch seconds.
    expires_at: i64,
    pub(crate) user_id: String,
    pub(crate) email: String,
    pub(crate) name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct AuthState {
    session: Option<StoredSession>,
}

/// Public status the frontend sees — never includes tokens.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus {
    pub has_account: bool,
    pub is_authenticated: bool,
    pub name: String,
    pub email: String,
}

impl From<&AuthState> for AuthStatus {
    fn from(s: &AuthState) -> Self {
        Self {
            has_account: s.session.is_some(),
            is_authenticated: s.session.is_some(),
            name: s.session.as_ref().map(|a| a.name.clone()).unwrap_or_default(),
            email: s.session.as_ref().map(|a| a.email.clone()).unwrap_or_default(),
        }
    }
}

fn auth_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("auth.json"))
}

fn load(app: &AppHandle) -> AuthState {
    let Ok(path) = auth_path(app) else {
        return AuthState::default();
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

fn save(app: &AppHandle, state: &AuthState) -> Result<(), String> {
    let path = auth_path(app)?;
    let json = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// --- Supabase GoTrue wire types (only the fields we actually use) ---

#[derive(Deserialize)]
struct GotrueUser {
    id: String,
    email: Option<String>,
    #[serde(default)]
    user_metadata: serde_json::Value,
}

#[derive(Deserialize)]
struct GotrueSession {
    access_token: String,
    refresh_token: String,
    expires_in: i64,
    user: GotrueUser,
}

#[derive(Deserialize)]
struct GotrueError {
    #[serde(alias = "error_description", alias = "msg")]
    message: Option<String>,
}

fn extract_name(user: &GotrueUser) -> String {
    user.user_metadata
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

fn session_from_gotrue(session: GotrueSession) -> StoredSession {
    let name = extract_name(&session.user);
    StoredSession {
        access_token: session.access_token,
        refresh_token: session.refresh_token,
        expires_at: now_secs() + session.expires_in,
        user_id: session.user.id,
        email: session.user.email.unwrap_or_default(),
        name,
    }
}

async fn gotrue_error_message(resp: reqwest::Response) -> String {
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    serde_json::from_str::<GotrueError>(&text)
        .ok()
        .and_then(|e| e.message)
        .unwrap_or_else(|| format!("Supabase returned {status}"))
}

async fn parse_session_response(resp: reqwest::Response) -> Result<GotrueSession, String> {
    if resp.status().is_success() {
        let text = resp.text().await.map_err(|e| e.to_string())?;
        serde_json::from_str::<GotrueSession>(&text)
            .map_err(|e| format!("Unexpected response from Supabase: {e}"))
    } else {
        Err(gotrue_error_message(resp).await)
    }
}

/// Refresh a stored session if it's expired (or about to be); on an actual
/// rejection from Supabase (not a network hiccup) the local session is
/// cleared so the UI correctly falls back to "log in again" rather than
/// silently staying "authenticated" with a dead token.
async fn refresh_if_needed(app: &AppHandle, state: &mut AuthState) {
    let Some(session) = &state.session else { return };
    if session.expires_at > now_secs() + 30 {
        return; // still valid for a while
    }
    let Ok((url, key)) = config() else { return };
    let client = reqwest::Client::new();
    let Ok(resp) = client
        .post(format!("{url}/auth/v1/token?grant_type=refresh_token"))
        .header("apikey", key)
        .json(&serde_json::json!({ "refresh_token": session.refresh_token }))
        .send()
        .await
    else {
        return; // network hiccup — keep the (possibly stale) session rather than logging out
    };
    match parse_session_response(resp).await {
        Ok(new_session) => {
            state.session = Some(session_from_gotrue(new_session));
            let _ = save(app, state);
        }
        Err(_) => {
            // Supabase actually rejected the refresh token (expired/revoked).
            state.session = None;
            let _ = save(app, state);
        }
    }
}

#[tauri::command]
pub async fn get_auth_status(app: AppHandle) -> AuthStatus {
    let mut state = load(&app);
    refresh_if_needed(&app, &mut state).await;
    AuthStatus::from(&state)
}

/// Shared by every marketplace command that needs to act as the signed-in
/// user (upload, wishlist, purchase, earnings, ...) — refreshes an expiring
/// session the same way `get_auth_status` does, then hands back the token
/// marketplace.rs attaches as `Authorization: Bearer` on Supabase REST calls.
pub(crate) async fn require_session(app: &AppHandle) -> Result<StoredSession, String> {
    let mut state = load(app);
    refresh_if_needed(app, &mut state).await;
    state.session.ok_or_else(|| "Log in to do this.".to_string())
}

#[tauri::command]
pub async fn signup(
    app: AppHandle,
    name: String,
    email: String,
    password: String,
) -> Result<AuthStatus, String> {
    let (url, key) = config()?;
    let (name, email) = (name.trim().to_string(), email.trim().to_string());
    if name.is_empty() || email.is_empty() || password.is_empty() {
        return Err("Name, email, and password are all required.".into());
    }
    if password.len() < 6 {
        return Err("Password must be at least 6 characters.".into());
    }

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{url}/auth/v1/signup"))
        .header("apikey", key)
        .json(&serde_json::json!({
            "email": email,
            "password": password,
            "data": { "name": name },
        }))
        .send()
        .await
        .map_err(|e| format!("Couldn't reach Supabase: {e}"))?;

    if !resp.status().is_success() {
        return Err(gotrue_error_message(resp).await);
    }

    let text = resp.text().await.map_err(|e| e.to_string())?;
    // With "Confirm email" on (Supabase's default), signup succeeds but
    // returns no session — the account exists but can't log in until the
    // user clicks the confirmation link Supabase just emailed them.
    match serde_json::from_str::<GotrueSession>(&text) {
        Ok(session) => {
            let state = AuthState {
                session: Some(session_from_gotrue(session)),
            };
            save(&app, &state)?;
            Ok(AuthStatus::from(&state))
        }
        Err(_) => Err("Account created — check your email to confirm it, then log in.".into()),
    }
}

#[tauri::command]
pub async fn login(app: AppHandle, email: String, password: String) -> Result<AuthStatus, String> {
    let (url, key) = config()?;
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{url}/auth/v1/token?grant_type=password"))
        .header("apikey", key)
        .json(&serde_json::json!({ "email": email.trim(), "password": password }))
        .send()
        .await
        .map_err(|e| format!("Couldn't reach Supabase: {e}"))?;

    let session = parse_session_response(resp).await?;
    let state = AuthState {
        session: Some(session_from_gotrue(session)),
    };
    save(&app, &state)?;
    Ok(AuthStatus::from(&state))
}

#[tauri::command]
pub async fn logout(app: AppHandle) -> Result<AuthStatus, String> {
    let state = load(&app);
    if let (Ok((url, key)), Some(session)) = (config(), &state.session) {
        let client = reqwest::Client::new();
        // Best-effort: invalidate the refresh token server-side. Logout
        // proceeds locally either way — it should still work while offline.
        let _ = client
            .post(format!("{url}/auth/v1/logout"))
            .header("apikey", key)
            .bearer_auth(&session.access_token)
            .send()
            .await;
    }
    let cleared = AuthState::default();
    save(&app, &cleared)?;
    Ok(AuthStatus::from(&cleared))
}
