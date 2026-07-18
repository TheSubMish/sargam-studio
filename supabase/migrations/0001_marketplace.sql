-- Sargam marketplace schema: tracks (uploaded projects), purchases, wishlists,
-- payout requests, and the storage buckets that hold preview/full audio.
--
-- Run this once in the Supabase dashboard's SQL Editor (project
-- mmdwmazvnggacldnnanq, or whichever project SUPABASE_URL in .env points at).
-- This repo has no DB credentials capable of running DDL by design — see
-- CLAUDE.md's Supabase section — so this file is applied by hand, not by any
-- automated migration runner.

create table public.profiles (
  id uuid primary key references auth.users(id) on delete cascade,
  display_name text not null,
  updated_at timestamptz not null default now()
);
alter table public.profiles enable row level security;
create policy "profiles are publicly readable" on public.profiles for select using (true);
create policy "users insert their own profile" on public.profiles for insert with check (auth.uid() = id);
create policy "users update their own profile" on public.profiles for update using (auth.uid() = id);

create table public.tracks (
  id uuid primary key default gen_random_uuid(),
  -- References profiles(id) (not auth.users directly) so PostgREST can
  -- embed the creator's display name via `tracks?select=*,profiles(display_name)`
  -- — a direct FK is what makes that embedding syntax work.
  creator_id uuid not null references public.profiles(id) on delete cascade,
  title text not null,
  description text not null default '',
  genre text not null default '',
  price_cents integer not null default 0 check (price_cents >= 0),
  preview_path text not null,     -- storage path in track-previews (public bucket)
  project_path text not null,     -- storage path in track-files (private, gated bucket)
  emoji text not null default '🎵',
  gradient_start text not null default '#667eea',
  gradient_end text not null default '#764ba2',
  rating_sum integer not null default 0,
  rating_count integer not null default 0,
  downloads_count integer not null default 0,
  status text not null default 'live' check (status in ('live', 'removed')),
  created_at timestamptz not null default now()
);
alter table public.tracks enable row level security;
create policy "live tracks are publicly readable" on public.tracks
  for select using (status = 'live' or creator_id = auth.uid());
create policy "creators insert their own tracks" on public.tracks
  for insert with check (creator_id = auth.uid());
create policy "creators update their own tracks" on public.tracks
  for update using (creator_id = auth.uid());

create table public.purchases (
  id uuid primary key default gen_random_uuid(),
  track_id uuid not null references public.tracks(id) on delete cascade,
  buyer_id uuid not null references auth.users(id) on delete cascade,
  amount_cents integer not null,
  platform_fee_cents integer not null,
  creator_earnings_cents integer not null,
  provider text not null check (provider in ('stripe', 'esewa', 'khalti')),
  provider_reference text,
  status text not null default 'pending' check (status in ('pending', 'completed', 'failed')),
  created_at timestamptz not null default now(),
  completed_at timestamptz
);
alter table public.purchases enable row level security;
create policy "buyers and creators can see their purchases" on public.purchases
  for select using (
    buyer_id = auth.uid()
    or track_id in (select id from public.tracks where creator_id = auth.uid())
  );
-- Deliberately no client insert/update policy: purchases are only ever
-- written by the Stripe Edge Functions using the service_role key, so a
-- client can never fabricate a "completed" purchase for itself.

create table public.wishlists (
  buyer_id uuid not null references auth.users(id) on delete cascade,
  track_id uuid not null references public.tracks(id) on delete cascade,
  created_at timestamptz not null default now(),
  primary key (buyer_id, track_id)
);
alter table public.wishlists enable row level security;
create policy "users manage their own wishlist" on public.wishlists
  for all using (buyer_id = auth.uid()) with check (buyer_id = auth.uid());

create table public.payout_requests (
  id uuid primary key default gen_random_uuid(),
  creator_id uuid not null references auth.users(id) on delete cascade,
  amount_cents integer not null check (amount_cents > 0),
  status text not null default 'requested' check (status in ('requested', 'paid', 'rejected')),
  requested_at timestamptz not null default now(),
  resolved_at timestamptz
);
alter table public.payout_requests enable row level security;
create policy "creators manage their own payout requests" on public.payout_requests
  for all using (creator_id = auth.uid()) with check (creator_id = auth.uid());

-- Earnings are computed on the fly from purchases rather than kept in a
-- separate ledger that could drift out of sync. security_invoker makes the
-- view respect the underlying tables' RLS, so a creator only ever sees their
-- own rows through it.
create view public.creator_earnings
  with (security_invoker = true) as
select
  t.id as track_id,
  t.creator_id,
  t.title,
  t.genre,
  t.price_cents,
  t.rating_sum,
  t.rating_count,
  t.downloads_count,
  coalesce(sum(p.creator_earnings_cents) filter (where p.status = 'completed'), 0) as earnings_cents,
  count(p.id) filter (where p.status = 'completed') as sales_count
from public.tracks t
left join public.purchases p on p.track_id = t.id
group by t.id;

-- Storage buckets: previews are public (played directly via <audio> in
-- Explore), full project files are private and gated by ownership/purchase.
insert into storage.buckets (id, name, public) values ('track-previews', 'track-previews', true);
insert into storage.buckets (id, name, public) values ('track-files', 'track-files', false);

create policy "track previews are publicly readable" on storage.objects
  for select using (bucket_id = 'track-previews');
create policy "creators upload their own previews" on storage.objects
  for insert with check (
    bucket_id = 'track-previews' and (storage.foldername(name))[1] = auth.uid()::text
  );

-- Path convention (enforced by the upload code, not the database):
--   track-previews/{creator_id}/{track_id}-preview.wav
--   track-files/{creator_id}/{track_id}.song.json
-- which is what lets (storage.foldername(name))[1] = auth.uid()::text work
-- as a "the uploader owns this path" check below.
create policy "purchased or free project files are downloadable" on storage.objects
  for select using (
    bucket_id = 'track-files' and (
      (storage.foldername(name))[1] = auth.uid()::text
      or exists (
        select 1 from public.tracks t
        where t.project_path = storage.objects.name
          and (
            t.price_cents = 0
            or exists (
              select 1 from public.purchases p
              where p.track_id = t.id and p.buyer_id = auth.uid() and p.status = 'completed'
            )
          )
      )
    )
  );
create policy "creators upload their own project files" on storage.objects
  for insert with check (
    bucket_id = 'track-files' and (storage.foldername(name))[1] = auth.uid()::text
  );
