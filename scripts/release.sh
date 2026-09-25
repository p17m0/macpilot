#!/bin/zsh
# Build everything that goes into a GitHub release, into dist/:
#   MacPilot-<version>.dmg                          drag-to-Applications disk image
#   MacPilot-<version>-macos-universal.zip          the app (used by the Homebrew cask)
#   macpilot-cli-<version>-macos-universal.tar.gz   the terminal app
#   SHA256SUMS.txt
#
# Signing and notarization are optional and controlled by environment variables:
#   CODESIGN_IDENTITY   "Developer ID Application: Your Name (TEAMID)"   — sign (see scripts/bundle.sh)
# and one way to authenticate with the notary service:
#   NOTARY_PROFILE      a profile saved with `xcrun notarytool store-credentials`
#   APPLE_API_KEY_PATH + APPLE_API_KEY_ID + APPLE_API_ISSUER               App Store Connect API key
#   APPLE_ID + APPLE_TEAM_ID + APPLE_APP_PASSWORD                          Apple ID + app-specific password
# Without them the result is ad-hoc signed and not notarized (users have to right-click → Open once).
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=${1:-v$(grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)}
DIST=dist
scripts/bundle.sh --universal

NOTARY=()
if [[ -n ${NOTARY_PROFILE:-} ]]; then
  NOTARY=(--keychain-profile "$NOTARY_PROFILE")
elif [[ -n ${APPLE_API_KEY_ID:-} ]]; then
  NOTARY=(--key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY_ID" --issuer "$APPLE_API_ISSUER")
elif [[ -n ${APPLE_ID:-} ]]; then
  NOTARY=(--apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password "$APPLE_APP_PASSWORD")
fi
SIGNED=0
[[ ${CODESIGN_IDENTITY:--} != "-" ]] && SIGNED=1

notarize() {
  echo "Notarizing $1…"
  xcrun notarytool submit "$1" "${NOTARY[@]}" --wait
}

if [[ $SIGNED == 1 && ${#NOTARY[@]} -gt 0 ]]; then
  # The app and the CLI go in one submission; the ticket is then stapled to the app.
  ditto -c -k --keepParent "$DIST/MacPilot.app" "$DIST/notarize.zip"
  ditto -c -k "$DIST/macpilot" "$DIST/notarize-cli.zip"
  notarize "$DIST/notarize.zip"
  notarize "$DIST/notarize-cli.zip"
  xcrun stapler staple "$DIST/MacPilot.app"
  rm -f "$DIST/notarize.zip" "$DIST/notarize-cli.zip"
else
  echo "Not notarizing (set CODESIGN_IDENTITY and notary credentials to do it)."
fi

ZIP="MacPilot-$VERSION-macos-universal.zip"
DMG="MacPilot-$VERSION.dmg"
CLI="macpilot-cli-$VERSION-macos-universal.tar.gz"
rm -f "$DIST/$ZIP" "$DIST/$DMG" "$DIST/$CLI"
ditto -c -k --keepParent "$DIST/MacPilot.app" "$DIST/$ZIP"
tar -czf "$DIST/$CLI" -C "$DIST" macpilot

# Disk image: the app next to a link to /Applications.
STAGE="$DIST/dmg"
rm -rf "$STAGE" && mkdir -p "$STAGE"
cp -R "$DIST/MacPilot.app" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "MacPilot" -srcfolder "$STAGE" -ov -format UDZO "$DIST/$DMG" >/dev/null
rm -rf "$STAGE"
if [[ $SIGNED == 1 ]]; then
  codesign --force --timestamp --sign "$CODESIGN_IDENTITY" "$DIST/$DMG"
  if [[ ${#NOTARY[@]} -gt 0 ]]; then
    notarize "$DIST/$DMG"
    xcrun stapler staple "$DIST/$DMG"
  fi
fi

(cd "$DIST" && shasum -a 256 "$DMG" "$ZIP" "$CLI" > SHA256SUMS.txt)
echo "Release files in $DIST/:"
cat "$DIST/SHA256SUMS.txt"
