#!/bin/bash
# Raycast script command (see souffleur-starten.sh).

# @raycast.schemaVersion 1
# @raycast.title Souffleur beenden
# @raycast.mode silent
# @raycast.packageName Dictate
# @raycast.icon 💬
# @raycast.description Beendet den laufenden Souffleur; die Mitschrift bleibt gespeichert.

curl -fsS -m 3 -X POST http://127.0.0.1:47619/souffleur/stop >/dev/null 2>&1 \
  && echo "Souffleur beendet" || { echo "Dictate läuft nicht"; exit 1; }
