-- Apply to the existing database before deploying the new website.
begin;

-- Remove the old anonymous entry point; revoking only anon would leave PUBLIC grants.
drop function if exists public.record_download(text, text, text);

create table if not exists public.download_claims (
  day date not null,
  client_hash text not null check (client_hash ~ '^[0-9a-f]{64}$'),
  os text not null check (os in ('mac', 'windows', 'linux')),
  primary key (day, client_hash, os)
);
alter table public.download_claims enable row level security;
revoke all on public.download_claims from public, anon, authenticated;

create or replace function public.record_download(p_os text, p_version text, p_country text, p_client_hash text)
returns void
language plpgsql
security definer
set search_path = ''
as $$
declare
  today date := (now() at time zone 'UTC')::date;
begin
  if p_client_hash is null or p_client_hash !~ '^[0-9a-f]{64}$' then
    raise exception 'Invalid client hash';
  end if;
  -- The primary key makes simultaneous requests across serverless instances atomic.
  insert into public.download_claims (day, client_hash, os)
  values (today, p_client_hash, p_os)
  on conflict do nothing;
  if not found then return; end if;

  insert into public.downloads (os, version, country)
  values (p_os, left(p_version, 32), case when p_country ~ '^[A-Z]{2}$' then p_country else null end);
  delete from public.download_claims where day < today - 2;
end;
$$;

revoke all on function public.record_download(text, text, text, text) from public, anon, authenticated;
grant execute on function public.record_download(text, text, text, text) to service_role;
commit;
