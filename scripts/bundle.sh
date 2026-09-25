#!/bin/zsh
# Build MacPilot.app (and the `macpilot` terminal binary) into dist/.
#
#   scripts/bundle.sh              # fast local build (release profile)
#   scripts/bundle.sh --dist       # optimized build for publishing (profile "dist")
#   scripts/bundle.sh --universal  # Apple Silicon + Intel universal binaries (implies --dist)
set -euo pipefail
cd "$(dirname "$0")/.."

PROFILE=release
UNIVERSAL=0
for arg in "$@"; do
  case $arg in
    --dist) PROFILE=dist ;;
    --universal) PROFILE=dist; UNIVERSAL=1 ;;
    *) echo "unknown option: $arg" >&2; exit 1 ;;
  esac
done

VERSION=$(grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)
DIST=dist
APP="$DIST/MacPilot.app"
rm -rf "$APP" && mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$APP/Contents/Helpers"

if [[ $UNIVERSAL == 1 ]]; then
  rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
  for t in aarch64-apple-darwin x86_64-apple-darwin; do
    cargo build --profile dist --target $t
  done
  lipo -create -output "$APP/Contents/MacOS/MacPilot" target/{aarch64,x86_64}-apple-darwin/dist/macpilot-gui
  lipo -create -output "$DIST/macpilot" target/{aarch64,x86_64}-apple-darwin/dist/macpilot
else
  cargo build --profile $PROFILE
  cp "target/$PROFILE/macpilot-gui" "$APP/Contents/MacOS/MacPilot"
  cp "target/$PROFILE/macpilot" "$DIST/macpilot"
fi

# The terminal app also ships inside the bundle (Homebrew links it; Contents/MacOS/MacPilot would clash on a case-insensitive disk).
cp "$DIST/macpilot" "$APP/Contents/Helpers/macpilot"

# Icon: every size macOS asks for, from the 1024 px master.
ICONSET="$DIST/AppIcon.iconset"
rm -rf "$ICONSET" && mkdir -p "$ICONSET"
for s in 16 32 128 256 512; do
  sips -z $s $s assets/icon.png --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
  sips -z $((s * 2)) $((s * 2)) assets/icon.png --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AppIcon.icns"
rm -rf "$ICONSET"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>MacPilot</string>
  <key>CFBundleDisplayName</key><string>MacPilot</string>
  <key>CFBundleIdentifier</key><string>io.github.macpilot</string>
  <key>CFBundleExecutable</key><string>MacPilot</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleLocalizations</key><array><string>en</string><string>fr</string><string>es</string><string>de</string><string>ru</string></array>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
  <key>NSAppleEventsUsageDescription</key><string>MacPilot asks Finder to move items to the Trash (so “Put Back” works) and quits apps gracefully.</string>
</dict>
</plist>
PLIST

# Signing. Without CODESIGN_IDENTITY: an ad-hoc signature (required on Apple Silicon, fine for your own Mac).
# With CODESIGN_IDENTITY="Developer ID Application: Name (TEAMID)": hardened runtime + secure timestamp,
# ready for notarization (scripts/release.sh).
ID="${CODESIGN_IDENTITY:--}"
if [[ $ID == "-" ]]; then
  for f in "$APP/Contents/Helpers/macpilot" "$APP" "$DIST/macpilot"; do
    codesign --force --sign - "$f" >/dev/null 2>&1 || true
  done
else
  # Inside out: nested code first, then the bundle.
  for f in "$APP/Contents/Helpers/macpilot" "$DIST/macpilot"; do
    codesign --force --options runtime --timestamp --sign "$ID" "$f"
  done
  codesign --force --options runtime --timestamp --entitlements scripts/entitlements.plist --sign "$ID" "$APP"
  codesign --verify --strict --verbose=1 "$APP"
  echo "Signed with: $ID"
fi

echo "Built $APP and $DIST/macpilot (MacPilot $VERSION, profile $PROFILE)"
