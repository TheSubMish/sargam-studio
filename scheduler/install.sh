#!/usr/bin/env bash
# Build and install the Sargam Studio publisher as a systemd *user* timer.
#   ./scheduler/install.sh            install files, do not start the timer
#   ./scheduler/install.sh --enable   also enable + start the timer
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
repo="$(dirname "$here")"
env_file="$HOME/.config/sargam-publisher.env"
unit_dir="$HOME/.config/systemd/user"

cargo build --release -p sargam-publisher --manifest-path "$repo/Cargo.toml"

mkdir -p "$HOME/.local/bin" "$unit_dir" "$(dirname "$env_file")"
install -m 755 "$repo/target/release/sargam-publisher" "$HOME/.local/bin/sargam-publisher"
install -m 644 "$here/sargam-publisher.service" "$here/sargam-publisher.timer" "$unit_dir/"

if [ ! -e "$env_file" ]; then
  umask 077
  cat > "$env_file" <<'ENV'
# Sargam Studio publisher credentials. Keep this file private (chmod 600).
# URL and publishable key are the same values as the app's .env.
SUPABASE_URL=
SUPABASE_PUBLISHABLE_KEY=
# A dedicated bot user you create in the Supabase dashboard (see README).
SARGAM_BOT_EMAIL=
SARGAM_BOT_PASSWORD=
ENV
  echo "created $env_file: fill it in before enabling the timer"
fi
chmod 600 "$env_file"

systemctl --user daemon-reload

if [ "${1:-}" = "--enable" ]; then
  if grep -qE '^(SUPABASE_URL|SUPABASE_PUBLISHABLE_KEY|SARGAM_BOT_EMAIL|SARGAM_BOT_PASSWORD)=$' "$env_file"; then
    echo "refusing to enable: $env_file still has empty values" >&2
    exit 1
  fi
  systemctl --user enable --now sargam-publisher.timer
  systemctl --user list-timers sargam-publisher.timer
else
  echo "installed. Next: fill in $env_file, test with"
  echo "  systemctl --user start sargam-publisher.service && journalctl --user -u sargam-publisher -n 20"
  echo "then run: $0 --enable"
fi
