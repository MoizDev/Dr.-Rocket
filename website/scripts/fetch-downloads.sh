#!/bin/sh
# Downloads are staged and authenticated as a complete release before promotion. All offered
# platforms are required. A failed attempt invalidates old stable names and their provenance.
set -eu
here="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)"
exec node "$here/scripts/fetch-downloads.mjs"
