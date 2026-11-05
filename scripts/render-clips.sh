#!/bin/bash
# Renders eval/sentences.tsv with macOS voices into eval/clips/ (16 kHz mono FLAC, committed), so
# the eval runs where `say` does not: Linux (`scripts/eval-clips.sh`). Synthetic speech
# only. Run it on a Mac after changing eval/sentences.tsv, then measure a new eval/baseline-cpu.json
# (`scripts/eval-clips.sh --update-baseline`) and commit both.
#
#   scripts/render-clips.sh                 # the voices below, only missing clips
#   DICTATE_CLIP_VOICES="Anna" scripts/render-clips.sh
#   scripts/render-clips.sh --force         # all again
set -euo pipefail
cd "$(dirname "$0")/.."
# Four of the German voices scripts/eval.sh uses (two female, two male); every voice adds ~2.5 MB.
voices="${DICTATE_CLIP_VOICES:-Anna|Eddy (Deutsch (Deutschland))|Sandy (Deutsch (Deutschland))|Reed (Deutsch (Deutschland))}"
force=0
[ "${1:-}" = "--force" ] && force=1
out=eval/clips
mkdir -p "$out"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

IFS='|' read -r -a list <<< "$voices"
while IFS=$'\t' read -r id dict text; do
  for voice in "${list[@]}"; do
    # "Eddy (Deutsch (Deutschland))" → eddy
    name="$(printf '%s' "${voice%% *}" | tr '[:upper:]' '[:lower:]')"
    clip="$out/$id-$name.flac"
    if [ "$force" = 1 ] || [ ! -s "$clip" ]; then
      say -v "$voice" -o "$tmp/clip.aiff" "$text"
      afconvert -f flac -d 'flac@16000' -c 1 "$tmp/clip.aiff" "$clip"
    fi
  done
done < <(grep -v '^#' eval/sentences.tsv)
echo "$(ls "$out"/*.flac | wc -l | tr -d ' ') clips, $(du -sh "$out" | cut -f1) in $out"
