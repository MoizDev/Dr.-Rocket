# Dr. Rocket

## Website (website/, live at doctorocket.com)

Next.js site deployed to the Vercel project `dr-rocket` (scope `moizs-projects-49365057`).
The site hosts the downloads itself: `scripts/fetch-downloads.sh` pulls the assets of the
public GitHub releases into `public/downloads/` (gitignored), with no login needed. Every Vercel
build runs it, so a push to `master` deploys production and other branches get previews. When
GitHub won't serve a file (while the repo is private), it copies the live site's file of the same
version instead, checked against the SHA-256 in its `version.json`; so until the repo is public,
a new release goes out with a deploy from this Mac. A build fails rather than go out without the
macOS or Windows files.

To deploy from this Mac instead (it uses the `gh` login when there is one):

```sh
cd website
vercel build --prod            # runs fetch-downloads.sh, then next build
vercel deploy --prebuilt --prod
```

Release notes live in `release-notes/<version>.md` (a little Markdown: headings, paragraphs,
lists, bold, code, links). The app embeds them all and shows them in its What's New tab on the
first launch after an update (`src/ui/whats_new.rs`); use the same file for the GitHub release
(`gh release create ... --notes-file release-notes/<version>.md`).

For a new app release, attach the files to the GitHub release (macOS DMG via
`DEVELOPER_ID="Developer ID Application: Moiz Hashmi (BJ2UZLM855)" NOTARY_PROFILE=drrocket
tools/bundle.sh --universal --dmg`, which signs and notarizes it; Windows and Linux via the
Release workflow), sign them for automatic updates (see App updates), then deploy as above. The
build takes its version from GitHub: the newest release with the macOS and Windows files
attached. It writes that into `website/release.json` (commit the change), and the badge,
downloads and share image all follow it.

The download files keep the same URLs across releases, so after replacing one run
`vercel cache purge --type cdn --yes` (in `website/`), or the CDN keeps serving the old file.

After deploying, check that https://doctorocket.com loads and that each
`/downloads/DrRocket-{mac.dmg,windows-x64.zip,linux-x64.tar.gz}` returns 200 with the
versioned file name in `Content-Disposition`.

## App updates

The app (`src/update.rs`) reads https://doctorocket.com/version.json every few hours: the
version plus each download's URL and SHA-256, generated at build time from the files in
`public/downloads/` (`updateFeed` in `website/lib/release.ts`). A newer version is downloaded in
the background, checked, and installed when the app quits or the user clicks Restart to Update.
So a release reaches existing users as soon as the site is deployed and the CDN purged; a stale
file fails the checksum and is never installed.

Updates must also be signed with the release key (`docs/signed-updates.md`). For each release,
fetch the new files (`sh website/scripts/fetch-downloads.sh`), run
`node tools/sign-updates.mjs ~/.drrocket-release/release-key.pem` from the repo root, then commit
and push `website/update-signatures.json` (and `website/release.json`) so the deployed
`version.json` carries the signatures. Until then the app sees the new version but won't install it. Check
that every platform in https://doctorocket.com/version.json has a `signature`. The private key
never goes in Git or on the website.

Testing: `DRROCKET_UPDATE_FEED=file:///path/feed.json` points the app at a test feed (it also
enables updates under automation and in development builds, and keeps the download inside
`DRROCKET_CONFIG_DIR`). On macOS, install a `tools/bundle.sh` app in a scratch folder and serve a
DMG built with a higher version. Windows and Linux: the manual `update-test.yml` workflow.

## Download counts

Download links go to `website/app/download/[os]/route.ts`, which records the download in
Supabase (project `dr-rocket`, `zfacelxxebqfxwhkwrrd`; schema in `website/supabase/downloads.sql`)
after responding, then redirects to `/downloads/<file>`. Bots and link previews aren't counted.
The Vercel project has `SUPABASE_URL` and `SUPABASE_SERVICE_ROLE_KEY` (production) for it; the
`record_download` function only accepts the service role.

To read the numbers: `select * from public.download_totals;` (or the Table Editor in the
Supabase dashboard).

## Windows testing

There's no Windows machine here; use the manual GitHub workflows:

- `windows-smoke.yml`: launches the released zip (`source=release`, `tag=...`) or a fresh
  build (`source=build`) with Racket installed and reports whether it stays up, plus crash
  reports, the renderer and Settings screenshots (artifact). `simulate_gpu_crash=true` tests
  the renderer fallback (`src/startup.rs`).
- `windows-bench.yml`: frame-rate runs. GitHub's runners have no GPU and present frames very
  slowly (even the empty welcome screen runs at about 4 fps), so their numbers don't reflect
  real machines; use them only to compare settings against each other.

Benchmark and automation scripts must never open repo files they edit: `tools/bench/` works on
a copy in `target/bench/`.

Indentation is a port of DrRacket's (`src/editor/sexp.rs`, `indent_for`). `tests/indent/*.rkt`
are indented by DrRacket itself and the `reindent_all_matches_drracket` test must reproduce
them; after adding a case, regenerate them with `racket tools/drracket-indent.rkt tests/indent/*.rkt`.
