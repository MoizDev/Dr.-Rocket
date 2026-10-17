#!/bin/sh
# Fetch the release binaries into public/downloads under stable names.
#
# The GitHub repo is private, so the site serves the files itself. Version, repo and file names
# all come from release.json. Uses the gh CLI when it is logged in, otherwise the GitHub API
# with $GITHUB_TOKEN (for CI or a Vercel build). An asset missing from the release (say, a
# Linux build that hasn't been attached yet) is skipped with a warning, not an error.
#
# Usage: scripts/fetch-downloads.sh            (from anywhere)
set -eu

here="$(cd "$(dirname "$0")/.." && pwd)"
out="$here/public/downloads"
manifest="$here/release.json"

field() { node -e "const r=require(process.argv[1]); const v=process.argv[2].split('.').reduce((o,k)=>o[k],r); console.log(String(v).replaceAll('{version}', r.version))" "$manifest" "$1"; }

version="$(field version)"
repo="$(field repo)"
tag="v$version"
mkdir -p "$out"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

use_gh=0
if command -v gh >/dev/null 2>&1 && gh auth token >/dev/null 2>&1; then use_gh=1; fi
if [ "$use_gh" = 0 ] && [ -z "${GITHUB_TOKEN:-}" ]; then
  echo "fetch-downloads: need a logged-in gh CLI or GITHUB_TOKEN to read $repo" >&2
  exit 1
fi

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
  if fetch "$asset" "$out/$file"; then
    echo "  $file  <- $asset ($(du -h "$out/$file" | cut -f1 | tr -d ' '))"
  else
    echo "  $file  skipped: $asset is not on release $tag yet" >&2
    [ "$os" = linux ] || status=1
  fi
done
exit $status
