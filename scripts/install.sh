#!/bin/sh
# Deploys Dictate on this Mac: builds and signs the bundle, swaps it into /Applications, tells
# LaunchServices about it (fresh icon in Spotlight/Finder), restarts the app and links the command
# line tool into ~/.local/bin. Permissions stay
# because the signing identity does (see bundle.sh).
#
# A team's build: DICTATE_BRAND=<folder> (see bundle.sh; brand.env, AppIcon.icns, profile.toml).
# Without one, an installed Dictate.app keeps its bundle id (and with it its permissions) and the
# script says how to keep its icon and profile too.
set -eu
if [ -n "${DICTATE_BRAND:-}" ]; then DICTATE_BRAND=$(cd "$DICTATE_BRAND" && pwd); export DICTATE_BRAND; fi
if [ -n "${DICTATE_PROFILE:-}" ] && [ -f "$DICTATE_PROFILE" ]; then DICTATE_PROFILE=$(cd "$(dirname "$DICTATE_PROFILE")" && pwd)/$(basename "$DICTATE_PROFILE"); fi
cd "$(dirname "$0")/.."
target=${DICTATE_INSTALL_DIR:-/Applications}/Dictate.app
if [ -z "${DICTATE_BRAND:-}" ] && [ -z "${DICTATE_BUNDLE_ID:-}" ] && [ -f "$target/Contents/Info.plist" ]; then
  installed=$(defaults read "$target/Contents/Info" CFBundleIdentifier 2>/dev/null || true)
  if [ -n "$installed" ] && [ "$installed" != app.dictate ]; then
    echo "keeping the installed bundle id $installed (its permissions stay); a team build installs with DICTATE_BRAND=<folder>" >&2
    export DICTATE_BUNDLE_ID="$installed"
  fi
fi
./scripts/bundle.sh
if pgrep -x dictate >/dev/null; then
  pkill -x dictate
  while pgrep -x dictate >/dev/null; do sleep 0.2; done
fi
rm -rf "$target"
cp -R target/Dictate.app "$target"
codesign --verify "$target"
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f -R -trusted "$target"
# The profile (default words, report target, Souffleur tools; core/src/profile.rs), before the app
# starts: DICTATE_PROFILE=<file|https URL|gh:owner/repo/path>, else the brand folder's
# profile.toml. Installed every time (a team's profile is the team's); later versions come from
# its update_from once a day. Without either the installed one (or the built-in neutral one) stays.
profile=${DICTATE_PROFILE:-}
if [ -z "$profile" ] && [ -n "${DICTATE_BRAND:-}" ] && [ -f "$DICTATE_BRAND/profile.toml" ]; then
  profile="$DICTATE_BRAND/profile.toml"
fi
if [ -n "$profile" ]; then
  "$target/Contents/MacOS/dictate" profile install "$profile" | head -1
fi
open "$target"
# The command line tool on the PATH (`dictate souffleur`, `dictate correct`, … in a terminal or from
# an agent): a link into the bundle, so it is always the installed build. DICTATE_BIN_DIR elsewhere.
bin=${DICTATE_BIN_DIR:-$HOME/.local/bin}
mkdir -p "$bin"
if [ -e "$bin/dictate" ] && [ ! -L "$bin/dictate" ]; then
  echo "left $bin/dictate alone (not a link)"
else
  ln -sfn "$target/Contents/MacOS/dictate" "$bin/dictate"
  echo "linked $bin/dictate"
fi
# Raycast script commands ("Souffleur starten", …): Raycast lists them once this folder is added
# under Settings → Extensions → Script Commands → Add Directories.
raycast="$HOME/Library/Application Support/dictate/raycast"
mkdir -p "$raycast"
cp scripts/raycast/*.sh "$raycast/"
echo "Raycast script commands in $raycast"
# Agent plugins for "Talk it over" (docs/agent-plugins.md); a profile picks one with [talk] agent.
agents="$HOME/Library/Application Support/dictate/agents"
mkdir -p "$agents"
cp agents/* "$agents/"
echo "agent plugins in $agents"
echo "installed $(defaults read "$target/Contents/Info" CFBundleShortVersionString) to $target"
# Gemma 4 E4B for "Formatieren" and "Korrigieren" (both on in the default shortcut), strongly
# recommended: without it Apple's model tidies the text, which takes seconds on a long dictation
# (Gemma: about half a second). 5 GB into the data folder, once per Mac; the running app takes it
# at the next dictation. In a terminal the script asks; DICTATE_GEMMA=1 downloads without asking
# (agents: do, unless the user said no), DICTATE_GEMMA=0 skips.
if ! "$target/Contents/MacOS/dictate" model list 2>/dev/null | grep -q '^gemma-4-e4b  *installed'; then
  want=${DICTATE_GEMMA:-}
  if [ -z "$want" ] && [ -t 0 ]; then
    printf 'Download Gemma 4 E4B for Format/Correct now (5 GB, strongly recommended)? [Y/n] '
    read -r answer || answer=n
    case $answer in [nN]*) want=0 ;; *) want=1 ;; esac
  fi
  if [ "$want" = 1 ]; then
    "$target/Contents/MacOS/dictate" model download gemma-4-e4b
  else
    echo "STRONGLY RECOMMENDED: Gemma 4 E4B is not downloaded. Without it Format/Correct run on Apple's" >&2
    echo "  model, seconds slower on long dictations. Get it: dictate model download gemma-4-e4b (5 GB)," >&2
    echo "  or in the app: Recognition → \"Format and clean up\" → Download (Erkennung → Laden)." >&2
  fi
fi
