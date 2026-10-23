#!/bin/sh
# Fetch the release binaries into public/downloads under stable names.
#
# The GitHub repo is private, so the site serves the files itself. The version is the newest
# GitHub release (prereleases included, drafts not) that has the macOS and Windows downloads
# attached; it is written into release.json, which the pages read. Repo and file names come
# from release.json. Uses the gh CLI when it is logged in, otherwise the GitHub API
# with $GITHUB_TOKEN (for CI or a Vercel build). An asset missing from the release (say, a
# Linux build that hasn't been attached yet) is skipped with a warning, not an error. A Vercel
# preview build with neither skips all the files; any other build without them fails.
#
# Usage: scripts/fetch-downloads.sh            (from anywhere)
set -eu

here="$(cd "$(dirname "$0")/.." && pwd)"
out="$here/public/downloads"
manifest="$here/release.json"

field() { node -e "const r=require(process.argv[1]); const v=process.argv[2].split('.').reduce((o,k)=>o[k],r); console.log(String(v).replaceAll('{version}', r.version))" "$manifest" "$1"; }

repo="$(field repo)"
mkdir -p "$out"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

use_gh=0
if command -v gh >/dev/null 2>&1 && gh auth token >/dev/null 2>&1; then use_gh=1; fi
if [ "$use_gh" = 0 ] && [ -z "${GITHUB_TOKEN:-}" ]; then
  # Vercel's preview builds (from git pushes) can't read GitHub: they build the page without the
  # files, so only the previews' download buttons don't work. Production always needs them.
  if [ "${VERCEL_ENV:-}" = preview ]; then
    echo "fetch-downloads: no GitHub access on this preview build; skipping the downloads, so its download buttons won't work" >&2
    exit 0
  fi
  echo "fetch-downloads: need a logged-in gh CLI or GITHUB_TOKEN to read $repo" >&2
  exit 1
fi

# The newest release with the macOS and Windows files attached becomes the site's version.
if [ "$use_gh" = 1 ]; then
  gh api "repos/$repo/releases?per_page=20" >"$tmp/releases.json"
else
  curl -fsSL -H "Authorization: Bearer $GITHUB_TOKEN" "https://api.github.com/repos/$repo/releases?per_page=20" >"$tmp/releases.json"
fi
node -e '
  const fs = require("fs");
  const [manifest, list] = process.argv.slice(1);
  const r = JSON.parse(fs.readFileSync(manifest, "utf8"));
  const releases = JSON.parse(fs.readFileSync(list, "utf8")).filter((x) => !x.draft);
  // Semver order (GitHub lists 1.0.0-beta.9 before 1.0.0-beta.10): numbers compare as numbers,
  // and a release comes after its prereleases.
  const key = (tag) => {
    const [core, pre] = tag.replace(/^v/, "").split("+")[0].split(/-(.*)/s);
    return [core.split(".").map(Number), pre ? pre.split(".") : []];
  };
  const part = (a, b) => {
    const [na, nb] = [/^\d+$/.test(a), /^\d+$/.test(b)];
    if (na && nb) return Number(a) - Number(b);
    if (na !== nb) return na ? -1 : 1;
    return a < b ? -1 : a > b ? 1 : 0;
  };
  const cmp = (x, y) => {
    const [[ca, pa], [cb, pb]] = [key(x.tag_name), key(y.tag_name)];
    for (let i = 0; i < 3; i++) if (ca[i] !== cb[i]) return ca[i] - cb[i];
    if (!pa.length || !pb.length) return pb.length - pa.length;
    for (let i = 0; i < Math.min(pa.length, pb.length); i++) {
      const c = part(pa[i], pb[i]);
      if (c) return c;
    }
    return pa.length - pb.length;
  };
  const ready = releases
    .filter((x) => {
      const v = x.tag_name.replace(/^v/, "");
      const names = new Set(x.assets.map((a) => a.name));
      return ["mac", "windows"].every((os) => names.has(r.downloads[os].asset.replaceAll("{version}", v)));
    })
    .sort(cmp)
    .pop();
  if (!ready) { console.error("fetch-downloads: no release has the macOS and Windows files yet"); process.exit(1); }
  const v = ready.tag_name.replace(/^v/, "");
  if (v !== r.version) {
    console.log(`  version ${r.version} -> ${v} (newest release on GitHub)`);
    r.version = v;
    fs.writeFileSync(manifest, JSON.stringify(r, null, 2) + "\n");
  }
' "$manifest" "$tmp/releases.json"
version="$(field version)"
tag="v$version"
echo "  release $tag"

fetch() { # fetch ASSET DEST -> 0 when downloaded
  asset="$1"; dest="$2"
  if [ "$use_gh" = 1 ]; then
    gh release download "$tag" -R "$repo" -p "$asset" -D "$tmp" --clobber >/dev/null 2>&1 || return 1
    mv "$tmp/$asset" "$dest"
  else
    api="https://api.github.com/repos/$repo/releases/tags/$tag"
    id="$(curl -fsSL -H "Authorization: Bearer $GITHUB_TOKEN" "$api" |
      node -e "let s='';process.stdin.on('data',d=>s+=d).on('end',()=>{const a=JSON.parse(s).assets.find(a=>a.name===process.argv[1]);console.log(a?a.id:'')})" "$asset")"
    [ -n "$id" ] || return 1
    curl -fsSL -H "Authorization: Bearer $GITHUB_TOKEN" -H "Accept: application/octet-stream" \
      "https://api.github.com/repos/$repo/releases/assets/$id" -o "$tmp/$asset" || return 1
    mv "$tmp/$asset" "$dest"
  fi
}

status=0
for os in mac windows linux; do
  asset="$(field "downloads.$os.asset")"
  file="$(field "downloads.$os.file")"
  # GitHub downloads occasionally fail mid-transfer: retry before giving up.
  if fetch "$asset" "$out/$file" || { sleep 3; fetch "$asset" "$out/$file"; } || { sleep 10; fetch "$asset" "$out/$file"; }; then
    echo "  $file  <- $asset ($(du -h "$out/$file" | cut -f1 | tr -d ' '))"
  else
    echo "  $file  skipped: $asset is not on release $tag yet" >&2
    [ "$os" = linux ] || status=1
  fi
done
exit $status
