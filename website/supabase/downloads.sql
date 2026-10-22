-- Download counter for doctorocket.com (Supabase project "dr-rocket", zfacelxxebqfxwhkwrrd).
-- Fresh-install base schema. Existing installations should apply only the new migration.

-- One row per download started from doctorocket.com. No IPs or user agents are kept.
create table public.downloads (
  id bigint generated always as identity primary key,
  at timestamptz not null default now(),
  os text not null check (os in ('mac', 'windows', 'linux')),
  version text not null,
  country text
);

alter table public.downloads enable row level security;
revoke all on public.downloads from anon, authenticated;

-- Apply migrations/20261019_harden_download_counter.sql after creating this base schema.
-- No anonymous write RPC is installed by this file.

-- Totals for reading in the dashboard.
create view public.download_totals with (security_invoker = true) as
select os, version, count(*) as downloads, max(at) as latest
from public.downloads
group by os, version
order by downloads desc;

revoke all on public.download_totals from anon, authenticated;
