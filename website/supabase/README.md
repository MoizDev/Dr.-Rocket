# Download counter rollout

For the existing database, run `migrations/20261019_harden_download_counter.sql` in the
Supabase SQL editor before deploying the website. For a new database, run `downloads.sql`
first. The migration preserves recorded downloads and removes the anonymous write RPC.

Set server-only `SUPABASE_URL` and `SUPABASE_SERVICE_ROLE_KEY` in the website deployment.
Use the legacy service-role JWT key (it is sent as both apikey and Bearer authorization).
Never prefix it with NEXT_PUBLIC_. Remove the old SUPABASE_PUBLISHABLE_KEY setting.
Without the server key or a trusted Vercel client address, download redirects still work
but no counter write is attempted. Do not restore anonymous execution as a fallback.

Counts are now deduplicated per UTC day, client address and operating system, enforced
atomically in Postgres across deployments. This counts download starts, not completed
transfers. Users behind the same NAT can be undercounted. Distributed bots using distinct
addresses still require perimeter rate limiting if they become a problem.

Raw IPs and user agents are not stored. A keyed hash includes the UTC date so it changes
daily; claims older than two days are removed on subsequent accepted downloads. Vercel
must be the public ingress because it overwrites X-Forwarded-For. Self-hosted deployments
skip counting until a trusted-ingress policy is implemented.

Check the migration with `checks.sql` in a transaction after applying it. That check
rolls back its own sample rows. Source: https://vercel.com/docs/headers/request-headers
