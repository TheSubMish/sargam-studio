#!/usr/bin/env bash
# Deploy a signed .aab to Google Play from this machine, using a service
# account key. Self-contained: needs only bash + openssl + curl + python3
# (no fastlane / pip / gcloud).
#
# Usage:
#   scripts/deploy-play.sh [options]
#     --build [N]     build the AAB first (cargo tauri android build --aab);
#                     N sets ANDROID_VERSION_CODE (must exceed the last upload)
#     --aab PATH      AAB to upload (default: the built universal release AAB)
#     --track NAME    internal | alpha | beta | production   (default: internal)
#     --status NAME   completed | draft | inProgress          (default: completed)
#     --key PATH      service account json (default: auto-detected in repo root)
#
# One-time setup (in your Google / Play accounts — can't be scripted):
#   - Create the service account, grant it "Release" access in Play Console
#     (Setup -> API access -> Users & permissions).
#   - Enable the "Google Play Android Developer API" in the Cloud project.
#   - Upload the FIRST bundle manually once (enrols Play App Signing).
#
# The same service account key doubles as the PLAY_SERVICE_ACCOUNT_JSON secret
# for the GitHub Actions pipeline (.github/workflows/android-release.yml).

set -euo pipefail

PKG="com.submish.sargam"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
AAB="$ROOT/crates/composer-desktop/gen/android/app/build/outputs/bundle/universalRelease/app-universal-release.aab"
TRACK="internal"; STATUS="completed"; KEY=""; DO_BUILD=""; VCODE=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --build)  DO_BUILD=1; [[ "${2:-}" =~ ^[0-9]+$ ]] && { VCODE="$2"; shift; }; shift;;
    --aab)    AAB="$2"; shift 2;;
    --track)  TRACK="$2"; shift 2;;
    --status) STATUS="$2"; shift 2;;
    --key)    KEY="$2"; shift 2;;
    -h|--help) sed -n '2,30p' "$0" | sed 's/^# \?//'; exit 0;;
    *) echo "unknown arg: $1" >&2; exit 1;;
  esac
done

# --- optionally build the AAB ---
if [[ -n "$DO_BUILD" ]]; then
  echo "→ building signed AAB${VCODE:+ (versionCode $VCODE)}…"
  ( cd "$ROOT/crates/composer-desktop" \
    && ANDROID_VERSION_CODE="$VCODE" cargo tauri android build --aab )
fi

# --- locate the service account key ---
if [[ -z "$KEY" ]]; then
  KEY=$(grep -rlI '"type": *"service_account"' "$ROOT"/*.json 2>/dev/null | head -1 || true)
fi
[[ -n "$KEY" && -f "$KEY" ]] || { echo "✗ service account key not found (pass --key PATH)"; exit 1; }
[[ -f "$AAB" ]] || { echo "✗ AAB not found: $AAB  (use --build, or --aab PATH)"; exit 1; }

echo "→ key:   $KEY"
echo "→ aab:   $AAB ($(du -h "$AAB" | cut -f1))"
echo "→ target: $PKG  track=$TRACK  status=$STATUS"

b64url() { openssl base64 -A | tr '+/' '-_' | tr -d '='; }
# extract a field, or print the API error and exit
jget() { python3 -c "import sys,json
d=json.load(sys.stdin)
if isinstance(d,dict) and d.get('error'): sys.exit('✗ API error: '+json.dumps(d['error']))
print(d$1)"; }

# --- 1. mint an OAuth2 access token (JWT bearer) ---
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
python3 -c "import json;open('$TMP/pk.pem','w').write(json.load(open('$KEY'))['private_key'])"
CLIENT_EMAIL=$(python3 -c "import json;print(json.load(open('$KEY'))['client_email'])")
NOW=$(date +%s); EXP=$((NOW+3600))
HEADER=$(printf '{"alg":"RS256","typ":"JWT"}' | b64url)
CLAIM=$(printf '{"iss":"%s","scope":"https://www.googleapis.com/auth/androidpublisher","aud":"https://oauth2.googleapis.com/token","iat":%s,"exp":%s}' \
  "$CLIENT_EMAIL" "$NOW" "$EXP" | b64url)
SIG=$(printf '%s' "$HEADER.$CLAIM" | openssl dgst -sha256 -sign "$TMP/pk.pem" -binary | b64url)
TOKEN=$(curl -s -X POST https://oauth2.googleapis.com/token \
  -d grant_type=urn:ietf:params:oauth:grant-type:jwt-bearer \
  --data-urlencode "assertion=$HEADER.$CLAIM.$SIG" | jget "['access_token']")
echo "✓ authenticated as $CLIENT_EMAIL"

AUTH=(-H "Authorization: Bearer $TOKEN")
API="https://androidpublisher.googleapis.com/androidpublisher/v3/applications/$PKG"
UP="https://androidpublisher.googleapis.com/upload/androidpublisher/v3/applications/$PKG"

# --- 2. create an edit ---
EDIT=$(curl -s -X POST "$API/edits" "${AUTH[@]}" -H "Content-Type: application/json" | jget "['id']")
echo "✓ edit $EDIT"

# --- 3. upload the bundle ---
echo "→ uploading bundle (can take a minute)…"
VC=$(curl -s -X POST "$UP/edits/$EDIT/bundles?uploadType=media" "${AUTH[@]}" \
  -H "Content-Type: application/octet-stream" --data-binary @"$AAB" | jget "['versionCode']")
echo "✓ uploaded versionCode $VC"

# --- 4. assign to track ---
curl -s -X PUT "$API/edits/$EDIT/tracks/$TRACK" "${AUTH[@]}" -H "Content-Type: application/json" \
  -d "{\"releases\":[{\"versionCodes\":[\"$VC\"],\"status\":\"$STATUS\"}]}" | jget "['track']" >/dev/null
echo "✓ assigned to '$TRACK'"

# --- 5. commit ---
curl -s -X POST "$API/edits/$EDIT:commit" "${AUTH[@]}" | jget "['id']" >/dev/null
echo "✓ committed — versionCode $VC is now on the '$TRACK' track. Check Play Console."
