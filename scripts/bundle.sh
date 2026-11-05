#!/bin/sh
# Builds target/Dictate.app: the menu bar app with its own microphone/Accessibility entries.
#
# A team's own build (bundle id, signing identity, icon) comes from a brand folder:
# DICTATE_BRAND=<folder> with brand.env (sets DICTATE_BUNDLE_ID, DICTATE_SIGN_IDENTITY) and
# AppIcon.icns. Without one: bundle id app.dictate, the icon in assets/, and signed with
# $DICTATE_SIGN_IDENTITY when set, else ad-hoc. macOS ties the Microphone and Accessibility
# permissions to bundle id + identity: a stable identity keeps them across rebuilds, ad-hoc loses
# them on every build.
set -eu
if [ -n "${DICTATE_BRAND:-}" ]; then DICTATE_BRAND=$(cd "$DICTATE_BRAND" && pwd); export DICTATE_BRAND; fi
cd "$(dirname "$0")/.."
icon=assets/AppIcon.icns
if [ -n "${DICTATE_BRAND:-}" ]; then
  [ -f "$DICTATE_BRAND/brand.env" ] && . "$DICTATE_BRAND/brand.env"
  [ -f "$DICTATE_BRAND/AppIcon.icns" ] && icon="$DICTATE_BRAND/AppIcon.icns"
fi
bundle_id=${DICTATE_BUNDLE_ID:-app.dictate}
# The main window's page (ui/, Svelte) is built first; build.rs embeds ui/dist into the binary.
(cd ui && { [ -d node_modules ] || npm ci; } && npm run build)
# llama.cpp (Gemma for "Korrigieren", src/llm.rs) builds with cmake, which Command Line Tools lack.
PATH="$PATH:$HOME/.local/bin:/opt/homebrew/bin"
command -v cmake >/dev/null || { echo "cmake missing: uv tool install cmake (or brew install cmake)" >&2; exit 1; }
cargo build --release
# Parakeet on the Neural Engine: the Swift helper with FluidAudio (ane/, src/platform/macos/ane.rs).
swift build -c release --package-path ane
# 0.<minor>.<commits on HEAD>, so every commit is a new, higher version (macOS keys
# its icon and app caches by version); "+dirty" when the tree has uncommitted changes.
base=$(sed -n 's/^version = "\([0-9]*\.[0-9]*\)\..*"/\1/p' Cargo.toml | head -1)
build=$(git rev-list --count HEAD 2>/dev/null || echo 0)
version="$base.$build"
git diff --quiet HEAD 2>/dev/null || version="$version+dirty"
app=target/Dictate.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/dictate "$app/Contents/MacOS/dictate"
cp ane/.build/release/dictate-ane "$app/Contents/MacOS/dictate-ane"
cp "$icon" "$app/Contents/Resources/AppIcon.icns"   # from scripts/icon.sh
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Dictate</string>
  <key>CFBundleDisplayName</key><string>Dictate</string>
  <key>CFBundleIdentifier</key><string>$bundle_id</string>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleExecutable</key><string>dictate</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$build.$(date +%s)</string>
  <key>LSMinimumSystemVersion</key><string>14.0</string>
  <key>LSUIElement</key><true/>
  <key>NSMicrophoneUsageDescription</key>
  <string>Dictate records your voice and turns it into text locally on this Mac.</string>
  <key>NSAudioCaptureUsageDescription</key>
  <string>In a meeting the Souffleur listens to what the Mac plays (the others in the call) and transcribes it locally.</string>
</dict>
</plist>
PLIST
# The permission prompts in German on a German Mac.
mkdir -p "$app/Contents/Resources/de.lproj"
cat > "$app/Contents/Resources/de.lproj/InfoPlist.strings" <<STRINGS
NSMicrophoneUsageDescription = "Dictate nimmt deine Stimme auf und wandelt sie lokal auf dem Mac in Text um.";
NSAudioCaptureUsageDescription = "Der Souffleur hört im Meeting mit, was der Mac abspielt (die anderen im Call), und schreibt es lokal mit.";
STRINGS
identity=${DICTATE_SIGN_IDENTITY:-}
# Media pause/resume: the mediaremote-adapter helper (vendor/mediaremote-adapter/DICTATE-VENDOR.md),
# loaded into Apple's /usr/bin/perl at runtime. Signed first, the app signature then seals it.
adapter=vendor/mediaremote-adapter
framework="$app/Contents/Resources/MediaRemoteAdapter.framework"
mkdir -p "$framework"
clang -dynamiclib -fobjc-arc -fvisibility=default -arch "$(uname -m)" -mmacosx-version-min=14.0 \
  -I "$adapter/include" -I "$adapter/src" \
  "$adapter"/src/adapter/*.m "$adapter"/src/private/*.m "$adapter"/src/utility/*.m "$adapter"/src/test/NowPlayingTest.m \
  -framework Foundation -framework AppKit -framework UniformTypeIdentifiers -framework MediaPlayer \
  -install_name @rpath/MediaRemoteAdapter.framework/MediaRemoteAdapter \
  -o "$framework/MediaRemoteAdapter"
cp "$adapter/bin/mediaremote-adapter.pl" "$app/Contents/Resources/"
codesign --force --sign "${identity:--}" "$framework/MediaRemoteAdapter"
codesign --force --sign "${identity:--}" --identifier "$bundle_id.ane" "$app/Contents/MacOS/dictate-ane"
codesign --force --sign "${identity:--}" --identifier "$bundle_id" "$app"
echo "built $app ($bundle_id, signed: ${identity:-ad-hoc})"
