#!/bin/bash
# Compile check and unit tests of the Linux build (beta) in a Debian container with the GTK,
# WebKitGTK, AppIndicator, ALSA and libxdo development packages (docs/linux.md), so it runs on a
# Mac with Docker, or on a Linux machine without those packages. Builds and tests; nothing runs
# on a desktop. Clean = `cargo check`, `cargo clippy -D warnings` and `cargo test` green for both
# crates.
#
#   scripts/check-linux.sh                    # CPU build (the default)
#   scripts/check-linux.sh --features cuda    # needs nvcc in the image: not in this one
#
# The cargo registry and the build go into Docker volumes (dictate-linux-cargo,
# dictate-linux-target), so the second run is quick and the Mac's target/ stays untouched.
# DICTATE_LINUX_IMAGE picks another image (default rust:1.99, Debian).
set -euo pipefail
cd "$(dirname "$0")/.."
command -v docker >/dev/null || { echo "missing docker" >&2; exit 1; }
image="${DICTATE_LINUX_IMAGE:-rust:1.99}"
packages="libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev libasound2-dev \
libxdo-dev libdbus-1-dev pkg-config cmake clang libclang-dev"
args="$*"
docker run --rm \
    -v "$PWD":/src -w /src \
    -v dictate-linux-cargo:/usr/local/cargo/registry \
    -v dictate-linux-target:/target -e CARGO_TARGET_DIR=/target \
    "$image" bash -euo pipefail -c "
        apt-get update -qq
        DEBIAN_FRONTEND=noninteractive apt-get install -y -qq --no-install-recommends $packages >/dev/null
        rustup component add clippy rustfmt >/dev/null 2>&1 || true
        cargo check --release --all-targets $args
        cargo clippy --release --all-targets $args -- -D warnings
        cargo test --release $args
    "
echo "Linux ($(docker run --rm "$image" uname -m)): check, clippy and tests clean"
