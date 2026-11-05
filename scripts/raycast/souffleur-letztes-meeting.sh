#!/bin/bash
# Raycast script command (see souffleur-starten.sh).

# @raycast.schemaVersion 1
# @raycast.title Souffleur: letztes Meeting
# @raycast.mode silent
# @raycast.packageName Dictate
# @raycast.icon 💬
# @raycast.description Öffnet die Mitschrift des letzten Meetings (mit Antworten und „Das Wichtigste“).

file=$("$HOME/.local/bin/dictate" souffleur show --path 2>/dev/null) || { echo "Noch kein Meeting gespeichert"; exit 1; }
open "$file"
echo "${file##*/}"
