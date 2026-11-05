#!/bin/bash
# Raycast script command (install.sh copies it to ~/Library/Application Support/dictate/raycast;
# add that folder once in Raycast → Settings → Extensions → Script Commands → Add Directories).

# @raycast.schemaVersion 1
# @raycast.title Souffleur starten
# @raycast.mode silent
# @raycast.packageName Dictate
# @raycast.icon 💬
# @raycast.description Startet den Souffleur (Meeting-Begleiter) und zeigt die Seitenleiste.

souffleur() { curl -fsS -m 3 -X POST http://127.0.0.1:47619/souffleur >/dev/null 2>&1; }
if ! souffleur; then
  # Dictate is not running: start it and ask again once its server is up.
  open -a Dictate
  for _ in $(seq 20); do
    sleep 0.5
    souffleur && break
  done || true
  souffleur || { echo "Dictate antwortet nicht"; exit 1; }
fi
echo "Souffleur läuft"
