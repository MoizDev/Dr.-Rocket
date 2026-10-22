# Signing releases

The app installs an update automatically only if the update is signed with the release key.
The public half of that key is `release-key.pub` (base64 of the raw 32-byte Ed25519 key), and
every build includes it. The private half never goes in Git or on the website. It lives on the
release Mac at `~/.drrocket-release/release-key.pem` (readable only by its owner), with a
backup kept in a password manager. If it is lost, updates have to be downloaded by hand until a
release with a new key reaches users.

For each release:

1. Build and upload the release archives. Fetch them with `website/scripts/fetch-downloads.sh`
   (`vercel build --prod` runs it too).
2. Run `node tools/sign-updates.mjs ~/.drrocket-release/release-key.pem` from the repo root.
   It refuses a key whose public half isn't `release-key.pub`. Commit
   `website/update-signatures.json`, then build and deploy the website as usual.
3. Keep the archives byte for byte. The website only adds a signature when the version, URL and
   checksum all match, so a replaced archive needs signing again (and the usual CDN purge).

The signature covers a domain separator, the version, the platform, the lowercase SHA-256 and
the URL, each followed by a newline. The app requires HTTPS archive URLs. A test feed can still
be read through `DRROCKET_UPDATE_FEED=file://...`, but it can't get around the key or allow
unsigned or local archives.

The app checks the signature before downloading, and checks the saved archive and signature
again at startup and just before installing. It never runs downloaded code to ask for its
version, and it keeps macOS's quarantine attribute. To change keys, ship a release built
with the new public key while the old key is still trusted.

Builds before 1.0.0-beta.13 only check the SHA-256 from the feed. They update to beta.13 that
way, and from then on updates must be signed.
