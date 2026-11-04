#!/bin/zsh
# Write the Homebrew cask for a release: the version and the SHA-256 of its zip.
#   scripts/cask.sh v0.8.2 [dist/SHA256SUMS.txt] > macpilot.rb
set -euo pipefail
cd "$(dirname "$0")/.."

TAG=${1:?usage: scripts/cask.sh <tag> [SHA256SUMS.txt]}
SUMS=${2:-dist/SHA256SUMS.txt}
VERSION=${TAG#v}
ZIP="MacPilot-$TAG-macos-universal.zip"
SHA=$(awk -v f="$ZIP" '$2 == f || $2 == "*" f { print $1 }' "$SUMS")
[[ ${#SHA} == 64 ]] || { echo "no checksum for $ZIP in $SUMS" >&2; exit 1; }
sed -E -e "s/^  version \".*\"/  version \"$VERSION\"/" -e "s/^  sha256 \".*\"/  sha256 \"$SHA\"/" packaging/homebrew/macpilot.rb
