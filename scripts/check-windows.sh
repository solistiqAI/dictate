#!/bin/bash
# Compile check of the Windows build (beta) from a Mac: `cargo xwin` cross-checks the whole
# workspace for x86_64-pc-windows-msvc with the Windows SDK and MSVC CRT it fetches once (into
# ~/Library/Caches/cargo-xwin, the licence is accepted for you). Type-checks and runs build scripts
# (llama.cpp's cmake build with clang-cl, ONNX Runtime's Windows download); it does not link and
# nothing runs. Clean = `cargo check` and `cargo clippy` both green for the Windows target.
#
#   scripts/check-windows.sh                      # CPU build (the default)
#   scripts/check-windows.sh --features directml  # Parakeet on DirectML
#   (vulkan and cuda need the Vulkan SDK / CUDA toolkit for llama.cpp: only on a Windows PC)
#
# Needs once: rustup target add x86_64-pc-windows-msvc; cargo install --locked cargo-xwin;
# LLVM (clang-cl, llvm-lib: brew install llvm); Ninja and cmake (uv tool install ninja cmake).
set -euo pipefail
cd "$(dirname "$0")/.."
for llvm in /opt/homebrew/opt/llvm/bin /opt/homebrew/opt/llvm@*/bin /usr/local/opt/llvm/bin; do
    [ -x "$llvm/clang-cl" ] && PATH="$llvm:$PATH" && break
done
export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"
for tool in clang-cl llvm-lib ninja cmake cargo-xwin; do
    command -v "$tool" >/dev/null || { echo "missing $tool (see the top of $0)" >&2; exit 1; }
done
target=x86_64-pc-windows-msvc
cargo xwin check --release --target "$target" --all-targets "$@"
cargo xwin clippy --release --target "$target" --all-targets "$@" -- -D warnings
echo "Windows ($target): check and clippy clean"
