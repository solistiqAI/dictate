#!/bin/bash
# Speech-recognition check without a microphone: macOS voices read eval/sentences.tsv, Dictate
# transcribes the clips, the word error rate (WER) says how well. Run before and after a change to
# the model, the dictionary, the filters or the trimming; `--baseline` fails when it got worse.
#
#   scripts/eval.sh                      # all German voices, prints the table
#   scripts/eval.sh --json out.json      # also writes the numbers
#   scripts/eval.sh --baseline eval/baseline.json
#   scripts/eval.sh --update-baseline    # after an intended change: new eval/baseline.json
#   DICTATE_EVAL_VOICES="Anna Markus" scripts/eval.sh   # fewer voices (faster)
#
# Other flags go to `dictate eval` (e.g. --model parakeet-ultra for the CPU build).
set -euo pipefail
cd "$(dirname "$0")/.."
cache="${DICTATE_EVAL_CACHE:-$HOME/Library/Caches/dictate-eval}"
mkdir -p "$cache"

args=()
while [ $# -gt 0 ]; do
  case "$1" in
    --update-baseline) args+=(--json eval/baseline.json) ;;
    *) args+=("$1") ;;
  esac
  shift
done

# One voice per line; the full name ("Eddy (Deutsch (Deutschland))") is what `say -v` needs.
voices="$cache/voices.txt"
if [ -n "${DICTATE_EVAL_VOICES:-}" ]; then
  printf '%s\n' $DICTATE_EVAL_VOICES > "$voices"
else
  say -v '?' | grep -E '[[:space:]]de_DE[[:space:]]' | sed -E 's/[[:space:]]+de_DE[[:space:]].*//' | sort -u > "$voices"
fi
[ -s "$voices" ] || { echo "no German macOS voice found (System Settings → Accessibility → Spoken Content)" >&2; exit 1; }

list="$cache/list.tsv"
: > "$list"
while IFS=$'\t' read -r id dict text; do
  while IFS= read -r voice; do
    # The text's hash in the name: a changed sentence is rendered again.
    sum="$(printf '%s' "$text" | shasum | cut -c1-8)"
    wav="$cache/$id-$sum-$(printf '%s' "$voice" | tr -c 'A-Za-z0-9' '_').wav"
    if [ ! -s "$wav" ]; then
      say -v "$voice" -o "$cache/tmp.aiff" "$text"
      afconvert -f WAVE -d LEI16@48000 "$cache/tmp.aiff" "$wav"
      rm -f "$cache/tmp.aiff"
    fi
    printf '%s\t%s\t%s\n' "$wav" "$dict" "$text" >> "$list"
  done < "$voices"
done < <(grep -v '^#' eval/sentences.tsv)

bin="./target/release/dictate"
[ -x "$bin" ] || cargo build --release
exec "$bin" eval "$list" --profile eval/profile.toml ${args[@]+"${args[@]}"}
