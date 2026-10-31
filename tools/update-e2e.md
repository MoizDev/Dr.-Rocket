# Local release/update assurance

Run from the repository root (Node, Python 3, OpenSSL and Rust installed):

```sh
node --test tools/sign-updates.test.mjs website/lib/download-provenance.test.mjs
CARGO_BUILD_JOBS=2 python3 tools/update-e2e.py --staging-only
CARGO_BUILD_JOBS=2 python3 tools/update-e2e.py
```

On headless Linux, run the last command under `xvfb-run -a`; install the same GUI libraries
as `.github/workflows/update-test.yml`. Native installs use scratch app bundles/DMG on macOS,
a zip on Windows, and a tarball on Linux. The test checks a real app's HTTPS download,
recovery/reverification of staging, Restart to Update, relaunch, and advancement from 98.0.0
to 99.0.0. The staging-only mode verifies transport/signatures/receipts and tamper rejection;
it does **not** test native installation. No Cargo versions or production manifests are edited.

`updater-e2e` is a compile-time feature requiring a generated ephemeral Ed25519 public key,
localhost HTTPS origin, TLS certificate, scratch root and test version. `build.rs` refuses
release profiles and builds without debug information. Test executables/config must remain
under the compiled OS-temporary scratch root. The runtime cannot substitute keys or enable
this feature. Production builds always use `release-key.pub`, including with a feed override;
assets remain HTTPS-only and signed. The harness serves only its archive/feed directory,
never its ephemeral private keys, and terminates tracked apps and deletes scratch material.
These fixtures do not test production code signing, notarization or Gatekeeper policy.

## Download and signing boundary

`website/scripts/fetch-downloads.sh` delegates to the Node fetcher. Every offered platform is
required. A complete release is staged and verified against GitHub API tag/asset identity,
size and SHA-256 digest retrieved over authenticated HTTPS. The local `.provenance.json`
receipt records that metadata with the exact repository/version/platform/artifact names.
The receipt is a trusted local build input, not a GitHub detached signature. It cannot prove
that a publisher uploaded an archive with the correct embedded executable version; the
publisher still owns that assertion.

If GitHub is unavailable/private, the same-version official-site fallback requires the
unchanged pinned Ed25519 signature, exact official URL and matching downloaded SHA-256.
The version is not advanced until all platforms pass. Failure invalidates all prior stable
names and receipts. Older releases without GitHub digests must use a verified signed site
copy; otherwise fetching fails. There is no unverified cache fallback.

The signer reauthenticates GitHub metadata over HTTPS at signing time (or verifies the
same-version site's pinned signature), then rechecks provenance and bytes for **all** platforms before atomically replacing
`update-signatures.json`. The website checks provenance, bytes and matching valid signatures
before exporting download links or a feed; missing/unsigned platforms stop the build. A new
release must therefore be fully fetched, signed and reviewed before a site build succeeds.
The website verifier contains the same unchanged public pin (inside Next's project root);
the Node regressions enforce parity with the app's `release-key.pub` for future key rotations.

CI preserves manual/PR entry points and runs on master/main/optimizations pushes. Release
packaging/upload jobs require the reusable platform CI and native updater jobs for the same
resolved SHA. The requested tag must match Cargo's version and still resolve to that SHA at
upload time. This does not gate a separate manual macOS packaging/upload operation performed
outside the workflow.
