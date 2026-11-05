#!/bin/bash
# The speech-recognition eval without macOS voices: the clips in eval/clips/ (eval/sentences.tsv
# read by four macOS voices, pre-rendered by scripts/render-clips.sh, 16 kHz FLAC) through the CPU
# build of Parakeet Ultra with `dictate-core eval`. Runs on Linux and on a Mac.
# Baseline: eval/baseline-cpu.json (eval/baseline.json is scripts/eval.sh's, Neural Engine).
#
#   scripts/eval-clips.sh --baseline eval/baseline-cpu.json
#   scripts/eval-clips.sh --update-baseline   # after an intended change
#   scripts/eval-clips.sh --json out.json
#
# What it fetches once when missing:
# - the model, `dictate-core model download parakeet-ultra` (2.5 GB from Hugging Face into
#   ~/.local/share/dictate/models on Linux, ~/Library/Application Support/dictate/models on a Mac);
# - on Linux, ONNX Runtime 1.31: dictate-core finds the one `dictate-core model download
#   onnxruntime` put into the data folder (or ORT_DYLIB_PATH, set that to use your own); without
#   either this fetches it from its GitHub release into ~/.cache/dictate/ and sets ORT_DYLIB_PATH.
# Other flags go to `dictate-core eval` (e.g. --model parakeet-ultra-int8).
set -euo pipefail
cd "$(dirname "$0")/.."

args=()
while [ $# -gt 0 ]; do
  case "$1" in
    --update-baseline) args+=(--json eval/baseline-cpu.json) ;;
    *) args+=("$1") ;;
  esac
  shift
done

installed_ort="${XDG_DATA_HOME:-$HOME/.local/share}/dictate/onnxruntime/lib/libonnxruntime.so"
if [ "$(uname -s)" = Linux ] && [ -z "${ORT_DYLIB_PATH:-}" ] && [ ! -e "$installed_ort" ]; then
  version=1.31.0
  case "$(uname -m)" in
    x86_64) arch=x64 ;;
    aarch64 | arm64) arch=aarch64 ;;
    *) echo "no ONNX Runtime build for $(uname -m)" >&2; exit 1 ;;
  esac
  ort="${XDG_CACHE_HOME:-$HOME/.cache}/dictate/onnxruntime-linux-$arch-$version"
  if [ ! -e "$ort/lib/libonnxruntime.so" ]; then
    mkdir -p "$(dirname "$ort")"
    curl -fsSL "https://github.com/microsoft/onnxruntime/releases/download/v$version/onnxruntime-linux-$arch-$version.tgz" \
      | tar -xz -C "$(dirname "$ort")"
  fi
  export ORT_DYLIB_PATH="$ort/lib/libonnxruntime.so"
fi

cargo build --release -p dictate-core --bin dictate-core
bin="${CARGO_TARGET_DIR:-target}/release/dictate-core"
model=parakeet-ultra
for ((i = 0; i < ${#args[@]}; i++)); do
  [ "${args[$i]}" = --model ] && model="${args[$((i + 1))]}"
done
models="$("$bin" model list)"
grep -qE "^$model +installed" <<< "$models" || "$bin" model download "$model"

list="$(mktemp)"
trap 'rm -f "$list"' EXIT
while IFS=$'\t' read -r id dict text; do
  for clip in eval/clips/"$id"-*.flac; do
    [ -e "$clip" ] && printf '%s\t%s\t%s\n' "$clip" "$dict" "$text" >> "$list"
  done
done < <(grep -v '^#' eval/sentences.tsv)
[ -s "$list" ] || { echo "no clips in eval/clips (scripts/render-clips.sh on a Mac)" >&2; exit 1; }

"$bin" eval "$list" --profile eval/profile.toml ${args[@]+"${args[@]}"}
