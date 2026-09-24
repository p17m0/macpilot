#!/bin/zsh
# Build MacPilot and install it on this Mac:
#   MacPilot.app → /Applications (or ~/Applications), `macpilot` → ~/.local/bin
set -euo pipefail
cd "$(dirname "$0")"
[[ -f "$HOME/.cargo/env" ]] && source "$HOME/.cargo/env"
command -v cargo >/dev/null || { echo "Rust is not installed. Get it from https://rustup.rs and run this script again."; exit 1; }

scripts/bundle.sh "$@"

DEST=/Applications
[[ -w $DEST ]] || { DEST="$HOME/Applications"; mkdir -p "$DEST"; }
osascript -e 'tell application "MacPilot" to quit' >/dev/null 2>&1 || true
rm -rf "$DEST/MacPilot.app"
cp -R dist/MacPilot.app "$DEST/"

BIN="$HOME/.local/bin"
mkdir -p "$BIN"
cp dist/macpilot "$BIN/macpilot"

echo "Installed $DEST/MacPilot.app and $BIN/macpilot"
[[ ":$PATH:" == *":$BIN:"* ]] || echo "Add $BIN to your PATH to use the \`macpilot\` command."
