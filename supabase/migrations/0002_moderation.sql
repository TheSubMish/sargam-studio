-- UGC moderation: track reports + creator blocks.
--
-- Required because the marketplace hosts user-uploaded tracks: the app stores
-- mandate an in-app way to report objectionable content and to block a
-- creator. Reports are written here for the operator to review; blocks are
-- per-user and cause the blocked creator's tracks to be filtered out of
-- Explore/Wishlist (see composer-desktop's marketplace.rs).
--
-- Run this once in the Supabase dashboard's SQL Editor, after
-- 0001_marketplace.sql. This repo holds no DDL-capable credentials by design
-- (see CLAUDE.md's Supabase section), so it's applied by hand.

-- A user's report of a track. Only the reporter can insert/see their own rows;
-- the operator reviews them with the service_role key in the dashboard.
create table public.track_reports (
  id uuid primary key default gen_random_uuid(),
  track_id uuid not null references public.tracks(id) on delete cascade,
  reporter_id uuid not null references auth.users(id) on delete cascade,
  reason text not null,
  details text not null default '',
  created_at timestamptz not null default now()
);
alter table public.track_reports enable row level security;
create policy "users file their own reports" on public.track_reports
  for insert with check (reporter_id = auth.uid());
create policy "users see their own reports" on public.track_reports
  for select using (reporter_id = auth.uid());

-- A user blocking a creator. creator_id references profiles(id) (the same
-- thing tracks.creator_id points at), so the block filter can compare directly
-- against a track's creator_id.
create table public.blocked_creators (
  user_id uuid not null references auth.users(id) on delete cascade,
  creator_id uuid not null references public.profiles(id) on delete cascade,
  created_at timestamptz not null default now(),
  primary key (user_id, creator_id)
);
alter table public.blocked_creators enable row level security;
create policy "users manage their own blocks" on public.blocked_creators
  for all using (user_id = auth.uid()) with check (user_id = auth.uid());
