//! Scheduled marketplace publisher.
//!
//! Generates folk compositions with `composer_core::generate`, renders a
//! 30-second preview, and publishes each one to the Sargam marketplace on
//! Supabase exactly the way the desktop app's Upload flow does (same storage
//! paths, same `tracks` row) — as a dedicated "Sargam Studio" bot account.
//!
//! It signs in with the PUBLIC publishable key and the bot's own email +
//! password, so Row Level Security governs everything it can do. It never
//! needs (and must never be given) the service_role key.
//!
//! Environment (see scheduler/README.md):
//!   SUPABASE_URL, SUPABASE_PUBLISHABLE_KEY, SARGAM_BOT_EMAIL, SARGAM_BOT_PASSWORD
//!
//! Usage: sargam-publisher [--count N] [--max-per-day N] [--seed S] [--dry-run [DIR]]

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use composer_core::engine::{render_song, write_wav};
use composer_core::generate::{generate, GeneratedSong};
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::Client;
use serde_json::{json, Value};

const PREVIEW_SECONDS: f32 = 30.0;
const SAMPLE_RATE: f32 = 44_100.0;
const DISPLAY_NAME: &str = "Sargam Studio";

/// Warm Organic-theme gradients (terracotta / sage / cream), so generated
/// tracks look at home next to the app's own palette.
const PALETTE: &[(&str, &str)] = &[
    ("#ffc6a5", "#ccdbb2"),
    ("#f6a06b", "#e1eecc"),
    ("#ffe1d0", "#aebf92"),
    ("#d67f48", "#f0fae1"),
    ("#ccdbb2", "#ffc6a5"),
    ("#8fa073", "#ffe1d0"),
];

struct Args {
    count: u32,
    max_per_day: u32,
    seed: Option<u64>,
    dry_run: Option<PathBuf>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        count: 1,
        max_per_day: 3,
        seed: None,
        dry_run: None,
    };
    let mut it = std::env::args().skip(1).peekable();
    while let Some(a) = it.next() {
        let mut num = |name: &str| -> Result<u64, String> {
            it.next()
                .and_then(|v| v.parse().ok())
                .ok_or(format!("{name} needs a number"))
        };
        match a.as_str() {
            "--count" => args.count = num("--count")? as u32,
            "--max-per-day" => args.max_per_day = num("--max-per-day")? as u32,
            "--seed" => args.seed = Some(num("--seed")?),
            "--dry-run" => {
                let dir = match it.peek() {
                    Some(next) if !next.starts_with("--") => PathBuf::from(it.next().unwrap()),
                    _ => PathBuf::from("publisher-dry-run"),
                };
                args.dry_run = Some(dir);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(args)
}

fn env(name: &str) -> Result<String, String> {
    std::env::var(name)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .ok_or(format!("{name} is not set"))
}

fn now_nanos() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// Midnight UTC today as an ISO timestamp, for the daily-cap query.
fn utc_midnight_iso() -> String {
    let days = (now_nanos() / 1_000_000_000 / 86_400) as i64;
    // Howard Hinnant's civil-from-days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T00:00:00Z")
}

fn headers(token: &str, anon_key: &str) -> Result<HeaderMap, String> {
    let mut h = HeaderMap::new();
    h.insert(
        "apikey",
        HeaderValue::from_str(anon_key).map_err(|e| e.to_string())?,
    );
    h.insert(
        "Authorization",
        HeaderValue::from_str(&format!("Bearer {token}")).map_err(|e| e.to_string())?,
    );
    Ok(h)
}

async fn check(resp: reqwest::Response, what: &str) -> Result<reqwest::Response, String> {
    if resp.status().is_success() {
        Ok(resp)
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        Err(format!("{what} failed ({status}): {body}"))
    }
}

struct Session {
    token: String,
    user_id: String,
}

async fn login(client: &Client, url: &str, anon: &str) -> Result<Session, String> {
    let resp = client
        .post(format!("{url}/auth/v1/token?grant_type=password"))
        .header("apikey", anon)
        .json(
            &json!({ "email": env("SARGAM_BOT_EMAIL")?, "password": env("SARGAM_BOT_PASSWORD")? }),
        )
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let body: Value = check(resp, "Bot login")
        .await?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    Ok(Session {
        token: body["access_token"]
            .as_str()
            .ok_or("login returned no access_token")?
            .into(),
        user_id: body["user"]["id"]
            .as_str()
            .ok_or("login returned no user id")?
            .into(),
    })
}

fn preview_wav_bytes(full: &[f32]) -> Result<Vec<u8>, String> {
    let len = ((PREVIEW_SECONDS * SAMPLE_RATE) as usize * 2).min(full.len());
    let tmp = std::env::temp_dir().join(format!("sargam-preview-{}.wav", uuid::Uuid::new_v4()));
    write_wav(&tmp, &full[..len], 2, SAMPLE_RATE as u32).map_err(|e| e.to_string())?;
    let bytes = std::fs::read(&tmp).map_err(|e| e.to_string());
    let _ = std::fs::remove_file(&tmp);
    bytes
}

/// Pick a seed whose title the bot hasn't already published.
async fn fresh_song(
    client: &Client,
    url: &str,
    h: &HeaderMap,
    user_id: &str,
    mut seed: u64,
) -> Result<(u64, GeneratedSong), String> {
    for _ in 0..20 {
        let g = generate(seed);
        let mut q =
            reqwest::Url::parse(&format!("{url}/rest/v1/tracks")).map_err(|e| e.to_string())?;
        q.query_pairs_mut()
            .append_pair("select", "id")
            .append_pair("creator_id", &format!("eq.{user_id}"))
            .append_pair("title", &format!("eq.{}", g.song.title));
        let resp = client
            .get(q)
            .headers(h.clone())
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let rows: Vec<Value> = check(resp, "Duplicate-title check")
            .await?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        if rows.is_empty() {
            return Ok((seed, g));
        }
        seed = seed.wrapping_add(0x9E37_79B9);
    }
    Err("couldn't find an unused title after 20 tries".into())
}

async fn publish(
    client: &Client,
    url: &str,
    anon: &str,
    session: &Session,
    seed: u64,
    g: &GeneratedSong,
) -> Result<String, String> {
    let full = render_song(&g.song, SAMPLE_RATE).map_err(|e| e.to_string())?;
    let preview = preview_wav_bytes(&full)?;
    let project = g.song.to_json().into_bytes();

    let track_id = uuid::Uuid::new_v4().to_string();
    let preview_path = format!("{}/{track_id}-preview.wav", session.user_id);
    let project_path = format!("{}/{track_id}.song.json", session.user_id);
    let h = headers(&session.token, anon)?;

    let resp = client
        .post(format!(
            "{url}/storage/v1/object/track-previews/{preview_path}"
        ))
        .headers(h.clone())
        .header("Content-Type", "audio/wav")
        .body(preview)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    check(resp, "Preview upload").await?;

    let resp = client
        .post(format!(
            "{url}/storage/v1/object/track-files/{project_path}"
        ))
        .headers(h.clone())
        .header("Content-Type", "application/json")
        .body(project)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if let Err(e) = check(resp, "Project upload").await {
        remove_object(client, url, &h, "track-previews", &preview_path).await;
        return Err(e);
    }

    let (g1, g2) = PALETTE[(seed % PALETTE.len() as u64) as usize];
    let resp = client
        .post(format!("{url}/rest/v1/tracks"))
        .headers(h.clone())
        .json(&json!({
            "id": track_id,
            "creator_id": session.user_id,
            "title": g.song.title,
            "description": g.description,
            "genre": g.genre,
            "price_cents": 0,
            "preview_path": preview_path,
            "project_path": project_path,
            "emoji": g.emoji,
            "gradient_start": g1,
            "gradient_end": g2,
        }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if let Err(e) = check(resp, "Listing the track").await {
        // Don't leave orphaned files behind a track that never got listed.
        remove_object(client, url, &h, "track-previews", &preview_path).await;
        remove_object(client, url, &h, "track-files", &project_path).await;
        return Err(e);
    }
    Ok(track_id)
}

async fn remove_object(client: &Client, url: &str, h: &HeaderMap, bucket: &str, path: &str) {
    let _ = client
        .delete(format!("{url}/storage/v1/object/{bucket}/{path}"))
        .headers(h.clone())
        .send()
        .await;
}

fn dry_run(dir: &Path, seed: u64) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let g = generate(seed);
    let stem = g.song.title.replace(' ', "_");
    g.song
        .save(&dir.join(format!("{stem}.song.json")))
        .map_err(|e| e.to_string())?;
    let audio = render_song(&g.song, SAMPLE_RATE).map_err(|e| e.to_string())?;
    write_wav(
        &dir.join(format!("{stem}.wav")),
        &audio,
        2,
        SAMPLE_RATE as u32,
    )
    .map_err(|e| e.to_string())?;
    println!(
        "dry run: {:?} ({:.0}s, genre {}) -> {}",
        g.song.title,
        g.song.duration_secs(),
        g.genre,
        dir.display()
    );
    println!("  {}", g.description);
    Ok(())
}

async fn run() -> Result<u32, String> {
    let args = parse_args()?;
    let mut seed = args.seed.unwrap_or_else(now_nanos);

    if let Some(dir) = &args.dry_run {
        for _ in 0..args.count {
            dry_run(dir, seed)?;
            seed = seed.wrapping_add(1);
        }
        return Ok(0);
    }

    let url = env("SUPABASE_URL")?;
    let url = url.trim_end_matches('/');
    let anon = env("SUPABASE_PUBLISHABLE_KEY")?;
    let client = Client::new();

    let session = login(&client, url, &anon).await?;
    let h = headers(&session.token, &anon)?;

    let resp = client
        .post(format!("{url}/rest/v1/profiles"))
        .headers(h.clone())
        .header("Prefer", "resolution=merge-duplicates")
        .json(&json!({ "id": session.user_id, "display_name": DISPLAY_NAME }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    check(resp, "Profile setup").await?;

    // Daily cap, so a misfiring timer can never flood Explore.
    let mut q = reqwest::Url::parse(&format!("{url}/rest/v1/tracks")).map_err(|e| e.to_string())?;
    q.query_pairs_mut()
        .append_pair("select", "id")
        .append_pair("creator_id", &format!("eq.{}", session.user_id))
        .append_pair("created_at", &format!("gte.{}", utc_midnight_iso()));
    let resp = client
        .get(q)
        .headers(h.clone())
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let today: Vec<Value> = check(resp, "Daily-cap check")
        .await?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let room = args.max_per_day.saturating_sub(today.len() as u32);
    let to_make = args.count.min(room);
    if to_make < args.count {
        println!(
            "daily cap: {} already published today (max {}), making {to_make} of {}",
            today.len(),
            args.max_per_day,
            args.count
        );
    }

    let mut failures = 0;
    for _ in 0..to_make {
        let (used_seed, g) = fresh_song(&client, url, &h, &session.user_id, seed).await?;
        match publish(&client, url, &anon, &session, used_seed, &g).await {
            Ok(id) => println!("published {:?} ({}) as {id}", g.song.title, g.genre),
            Err(e) => {
                eprintln!("FAILED {:?}: {e}", g.song.title);
                failures += 1;
            }
        }
        seed = used_seed.wrapping_add(1);
    }
    Ok(failures)
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    match run().await {
        Ok(0) => {}
        Ok(n) => {
            eprintln!("{n} track(s) failed to publish");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
