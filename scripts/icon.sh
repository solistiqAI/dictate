#!/bin/sh
# Renders an icon SVG to an .icns for the app bundle (macOS), the same way Tauri apps make theirs.
#   scripts/icon.sh                          assets/app-icon.svg → assets/AppIcon.icns (the default icon)
#   scripts/icon.sh my-icon.svg out.icns     a team's own icon (see DICTATE_BRAND in scripts/bundle.sh)
set -eu
cd "$(dirname "$0")/.."
svg=${1:-assets/app-icon.svg}
icns=${2:-assets/AppIcon.icns}
out=$(mktemp -d)
trap 'rm -rf "$out"' EXIT
npx -y @tauri-apps/cli icon "$svg" --output "$out" >/dev/null
cp "$out/icon.icns" "$icns"
echo "$icns updated"
