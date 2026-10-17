#!/bin/sh
# Build a release Dr. Rocket.app in target/, and optionally a universal binary, a DMG, a
# signed and notarized build, and an install to ~/Applications.
#
# Usage: tools/bundle.sh [--universal] [--dmg] [--install]
#
#   --universal   one app for Apple Silicon and Intel Macs (needs the x86_64-apple-darwin
#                 Rust target: `rustup target add x86_64-apple-darwin`)
#   --dmg         also make target/Dr.Rocket-<version>.dmg, the branded drag-to-Applications
#                 window (needs dmgbuild: `pipx install dmgbuild`)
#   --install     copy the app to ~/Applications
#
# Distribution signing (otherwise the app is signed ad hoc, which only runs on this Mac):
#   DEVELOPER_ID="Developer ID Application: Your Name (TEAMID)"   sign for distribution
#   NOTARY_PROFILE=<profile>   also notarize the DMG with Apple and staple the ticket
#                              (create the profile once with `xcrun notarytool store-credentials`)
set -e
cd "$(dirname "$0")/.."

universal=0; dmg=0; install=0
for arg in "$@"; do
  case "$arg" in
    --universal) universal=1 ;;
    --dmg) dmg=1 ;;
    --install) install=1 ;;
    *) echo "unknown option: $arg" >&2; exit 2 ;;
  esac
done

app="target/Dr. Rocket.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
if [ "$universal" = 1 ]; then
  cargo build --release --target aarch64-apple-darwin
  cargo build --release --target x86_64-apple-darwin
  lipo -create -output "$app/Contents/MacOS/drrocket" \
    target/aarch64-apple-darwin/release/drrocket target/x86_64-apple-darwin/release/drrocket
else
  cargo build --release
  cp target/release/drrocket "$app/Contents/MacOS/drrocket"
fi

# Icon set from the rendered PNGs (regenerate with `cargo run --example make_icon`).
iconset="$(mktemp -d)/AppIcon.iconset"
mkdir -p "$iconset"
for s in 16 32 128 256 512; do
  cp "assets/icon/icon_$s.png" "$iconset/icon_${s}x${s}.png"
  d=$((s * 2))
  cp "assets/icon/icon_$d.png" "$iconset/icon_${s}x${s}@2x.png"
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/AppIcon.icns"

version=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Dr. Rocket</string>
  <key>CFBundleDisplayName</key><string>Dr. Rocket</string>
  <key>CFBundleIdentifier</key><string>dev.moiz.drrocket</string>
  <key>CFBundleExecutable</key><string>drrocket</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
  <key>CFBundleDocumentTypes</key>
  <array>
    <dict>
      <key>CFBundleTypeName</key><string>Racket Source</string>
      <key>CFBundleTypeRole</key><string>Editor</string>
      <key>LSHandlerRank</key><string>Alternate</string>
      <key>CFBundleTypeExtensions</key>
      <array><string>rkt</string><string>rktl</string><string>rktd</string><string>scrbl</string><string>ss</string><string>scm</string></array>
    </dict>
  </array>
</dict>
</plist>
PLIST

if [ -n "$DEVELOPER_ID" ]; then
  # Hardened runtime and a secure timestamp are required for notarization.
  codesign --force --options runtime --timestamp -s "$DEVELOPER_ID" "$app"
  echo "Signed with $DEVELOPER_ID"
else
  codesign --force --deep -s - "$app" >/dev/null 2>&1 || true
fi
echo "Built $app"

if [ "$dmg" = 1 ]; then
  image="target/Dr.Rocket-$version.dmg"
  rm -f "$image"
  # The branded window (background, arrow, icon positions) needs dmgbuild:
  # `pipx install dmgbuild` or `pip install dmgbuild`.
  if ! command -v dmgbuild >/dev/null 2>&1; then
    echo "--dmg needs dmgbuild (pipx install dmgbuild)" >&2
    exit 1
  fi
  dmgbuild -s tools/dmg/settings.py -D app="$app" "Dr. Rocket" "$image" >/dev/null
  if [ -n "$DEVELOPER_ID" ]; then
    codesign --force --timestamp -s "$DEVELOPER_ID" "$image"
  fi
  if [ -n "$NOTARY_PROFILE" ]; then
    xcrun notarytool submit "$image" --keychain-profile "$NOTARY_PROFILE" --wait
    xcrun stapler staple "$image"
    echo "Notarized and stapled"
  fi
  echo "Built $image"
fi

if [ "$install" = 1 ]; then
  mkdir -p "$HOME/Applications"
  rm -rf "$HOME/Applications/Dr. Rocket.app"
  cp -R "$app" "$HOME/Applications/"
  echo "Installed to ~/Applications/Dr. Rocket.app"
fi
