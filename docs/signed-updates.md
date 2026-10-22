# Signing releases

Automatic installation requires an Ed25519 key pinned into the app at build time. Builds
without a key, unsigned feeds, and legacy cached updates offer manual downloads only.
Existing installations made before this change must receive this version before they can
verify signatures; changing the website alone cannot secure an old updater.

1. Generate an Ed25519 key outside this repository, for example with
   `openssl genpkey -algorithm ED25519 -out /secure/location/release-key.pem`.
   Keep the private key off the website and out of Git. Restrict access to the file.
2. Export its raw 32-byte public key in base64 as `DRROCKET_UPDATE_PUBLIC_KEY` when building
   every platform. For GitHub Actions, set the repository variable of that name. The signing
   script prints this public value, which can also be obtained from Node's JWK `x` field.
   Local `tools/bundle.sh` builds inherit the environment variable.
3. Build and upload the release archives. Fetch them with `website/scripts/fetch-downloads.sh`.
4. Run `node tools/sign-updates.mjs /secure/location/release-key.pem` from the repo root.
   Review and commit `website/update-signatures.json`, then build/deploy the website normally.
   The private key is only read locally by this script; the website receives signatures.
5. Preserve the exact archive bytes. The website attaches a signature only when its version,
   URL and checksum match. Replaced archives need fresh signatures and the usual CDN purge.

The signature covers a domain separator, version, platform, lowercase SHA-256 and URL,
with newline separators and a final newline. The app requires HTTPS archive URLs. A test
feed may still be read through `DRROCKET_UPDATE_FEED=file://...`, but it cannot bypass the
pinned key or permit unsigned/local archive URLs.

The app verifies before downloading and rechecks the retained archive and signature on
startup and immediately before extraction/install. It never runs downloaded code to ask
for a version, and it preserves macOS quarantine attributes. Rotating the signing key
requires a release built with the new public key, delivered while the old key is trusted.
