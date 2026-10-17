-- Download counter for doctorocket.com (Supabase project "dr-rocket", zfacelxxebqfxwhkwrrd).
-- Applied as migrations "downloads" and "record_download_anon_only"; kept here for reference.

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

-- The website's only way in: record a download. Runs as the owner, so the table itself
-- stays closed to the publishable key.
create function public.record_download(p_os text, p_version text, p_country text)
returns void
language sql
security definer
set search_path = ''
as $$
  insert into public.downloads (os, version, country)
  values (p_os, left(p_version, 32), left(p_country, 2));
$$;

revoke all on function public.record_download(text, text, text) from public;
grant execute on function public.record_download(text, text, text) to anon;

-- Totals for reading in the dashboard.
create view public.download_totals with (security_invoker = true) as
select os, version, count(*) as downloads, max(at) as latest
from public.downloads
group by os, version
order by downloads desc;

revoke all on public.download_totals from anon, authenticated;
