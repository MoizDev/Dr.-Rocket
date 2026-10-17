# Dr. Rocket

## Website (website/, live at doctorocket.com)

Next.js site deployed to the Vercel project `dr-rocket` (scope `moizs-projects-49365057`).
The GitHub repo is private, so the site hosts the downloads itself: `scripts/fetch-downloads.sh`
pulls the release assets into `public/downloads/` (gitignored) using the local `gh` login.

Deploy by building locally and uploading the build, so the binaries go up with it and Vercel
never needs a GitHub token:

```sh
cd website
vercel build --prod            # runs fetch-downloads.sh, then next build
vercel deploy --prebuilt --prod
```

Don't use a plain `vercel deploy` or git-triggered builds: they can't fetch the downloads.

For a new app release, first attach the files to the GitHub release (macOS DMG via
`tools/bundle.sh --universal --dmg`, Windows and Linux via the Release workflow), then bump
`version` in `website/release.json` and deploy as above.

After deploying, check that https://doctorocket.com loads and that each
`/downloads/DrRocket-{mac.dmg,windows-x64.zip,linux-x64.tar.gz}` returns 200 with the
versioned file name in `Content-Disposition`.

## Download counts

Download links go to `website/app/download/[os]/route.ts`, which records the download in
Supabase (project `dr-rocket`, `zfacelxxebqfxwhkwrrd`; schema in `website/supabase/downloads.sql`)
after responding, then redirects to `/downloads/<file>`. Bots and link previews aren't counted.
The Vercel project has `SUPABASE_URL` and `SUPABASE_PUBLISHABLE_KEY` (production) for it.

To read the numbers: `select * from public.download_totals;` (or the Table Editor in the
Supabase dashboard).
