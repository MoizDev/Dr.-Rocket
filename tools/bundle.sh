#!/bin/sh
# Build a release Dr. Rocket.app in target/ (and optionally install it to ~/Applications).
# Usage: tools/bundle.sh [--install]
set -e
cd "$(dirname "$0")/.."
cargo build --release
app="target/Dr. Rocket.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/drrocket "$app/Contents/MacOS/drrocket"

# Icon set from the rendered PNGs.
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
codesign --force --deep -s - "$app" >/dev/null 2>&1 || true
echo "Built $app"
if [ "$1" = "--install" ]; then
  mkdir -p "$HOME/Applications"
  rm -rf "$HOME/Applications/Dr. Rocket.app"
  cp -R "$app" "$HOME/Applications/"
  echo "Installed to ~/Applications/Dr. Rocket.app"
fi
