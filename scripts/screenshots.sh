#!/bin/zsh
# Screenshot every page in the light and the dark theme at the smallest window, to catch layout problems
# (buttons over text, cut-off columns) before a release. Used by CI; works locally too.
#   scripts/screenshots.sh [out-dir]          default: target/screenshots
# The app starts as on a first launch (MACPILOT_FRESH): nothing is read from your files.
set -euo pipefail
cd "$(dirname "$0")/.."

OUT=${1:-target/screenshots}
mkdir -p "$OUT"
cargo build --release --features dev-tools --bin macpilot-gui
BIN=target/release/macpilot-gui

PAGES=(overview procs procs:flat battery disk clean clean:dev clean:history apps apps:updates apps:leftovers startup activity settings privacy settings:cleanup)
THEMES=(light dark)
failed=0
for theme in $THEMES; do
  for page in $PAGES; do
    file="$OUT/${theme}-${page//:/-}.png"
    rm -f "$file"
    # Now and then macOS does not hand a capture back to a window in the background: try twice.
    for _try in 1 2; do
      MACPILOT_FRESH=1 MACPILOT_LANG=${MACPILOT_LANG:-ru} MACPILOT_HOST=Mac MACPILOT_THEME=$theme \
        MACPILOT_PAGE=$page MACPILOT_SIZE=1200x700 MACPILOT_SHOT="$file" "$BIN" >/dev/null 2>&1 &
      pid=$!
      for _ in {1..60}; do [[ -f $file ]] && break; sleep 1; done
      kill $pid 2>/dev/null || true
      wait $pid 2>/dev/null || true
      [[ -f $file ]] && break
    done
    if [[ -f $file ]]; then echo "ok   $file"; else echo "FAIL $theme $page"; failed=1; fi
  done
done
exit $failed
