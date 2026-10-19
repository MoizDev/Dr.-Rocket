-- Run on a test database after the migration; changes are rolled back.
begin;
do $$
declare before_count bigint;
begin
  if has_function_privilege('anon', 'public.record_download(text,text,text,text)', 'EXECUTE')
     or has_function_privilege('authenticated', 'public.record_download(text,text,text,text)', 'EXECUTE') then
    raise exception 'Public role can execute counter RPC';
  end if;
  if to_regprocedure('public.record_download(text,text,text)') is not null then
    raise exception 'Legacy anonymous RPC still exists';
  end if;
  delete from public.download_claims where client_hash = repeat('a', 64);
  select count(*) into before_count from public.downloads;
  perform public.record_download('mac', 'security-check', 'CA', repeat('a', 64));
  perform public.record_download('mac', 'security-check', 'CA', repeat('a', 64));
  if (select count(*) from public.downloads) <> before_count + 1 then
    raise exception 'Repeated downloads were not deduplicated';
  end if;
end;
$$;
rollback;
