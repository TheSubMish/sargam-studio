# Sargam Studio: scheduled composer

Three times a day, a systemd timer on this machine runs `sargam-publisher`,
which composes a new folk track with `composer_core::generate`, renders a
30-second preview, and publishes it to the marketplace on Supabase. Everyone
who opens **Explore** sees the growing list; each track is free to download and
opens as a normal editable song in Sargam.

## How it publishes (and why it is safe)

It signs in as a **dedicated bot account** with the public publishable key, so
Row Level Security applies to it like any user. It writes the same three things
the desktop Upload flow writes: a preview in `track-previews`, the project JSON
in `track-files`, and a row in `tracks`, all under the bot's own user id. It
never uses, and must never be given, the `service_role` key. A built-in cap of 3
tracks per UTC day stops a misfiring timer from flooding Explore.

## One-time setup

1. **Create the bot user.** Supabase dashboard, Authentication, Users, Add user,
   with an email and a strong password, and tick **Auto Confirm User** (the
   publisher can't click a confirmation email).
2. **Install:** `./scheduler/install.sh` (builds, copies the binary to
   `~/.local/bin`, installs the units, creates `~/.config/sargam-publisher.env`).
3. **Fill in `~/.config/sargam-publisher.env`** with the `SUPABASE_URL` and
   `SUPABASE_PUBLISHABLE_KEY` from the app's `.env`, plus the bot email and password.
4. **Test once:** `systemctl --user start sargam-publisher.service`, then
   `journalctl --user -u sargam-publisher -n 20`. You should see `published "..."`.
   Open Explore in the app to confirm it appears.
5. **Turn it on:** `./scheduler/install.sh --enable`.

The timer only runs while this computer is on and online. `Persistent=true`
makes a missed slot run once when the machine wakes. To keep it running while
you are logged out: `loginctl enable-linger $USER`.

## Everyday use

- Preview without publishing: `sargam-publisher --count 3 --dry-run ./out`
  (writes `.song.json` and `.wav` files; no network).
- Reproduce a song: `--seed <number>`. The seed is in every track's description.
- See upcoming runs: `systemctl --user list-timers sargam-publisher.timer`
- Pause: `systemctl --user disable --now sargam-publisher.timer`
- Remove a track: set its `status` to `removed` in the dashboard's Table Editor.

## Notes

- Tracks are labelled in their description as auto-composed by the rule-based
  generator, so listeners are not misled about how they were made.
- Explore in the app requires login. That is an existing product decision, not
  something the publisher changes.
- Songs are about 30 to 36 seconds (a 16-cycle form). Lengthen `FORM_CYCLES` in
  `crates/composer-core/src/generate.rs` for longer pieces.
