#!/usr/bin/env bash
#
# Populate the real-game test sandbox from an installed Steam game.
#
# Most of the suite builds its game tree from synthetic fixtures. Three tests instead run
# over a copy of a REAL install, which is where genuine header flags, Creation Club
# content, Bethesda's mixed-case filenames, and multi-megabyte plugins actually exercise
# the engine:
#
#   crates/deploy/tests/real_game_roundtrip.rs    deploy -> verify -> repair -> purge
#   crates/loadorder/tests/real_plugin_scan.rs    plugin discovery + classification
#   src-tauri/tests/real_archive_workflow.rs      archive -> extract -> stage -> deploy -> purge
#
# They SKIP cleanly when the sandbox is absent, so CI and other machines are unaffected.
#
# SAFETY: the real install is only ever READ. Everything is copied into
# `target/realtest/game`, which is gitignored build output, and each test additionally
# copies THAT into its own temp dir before deploying — so a failure mid-run can never
# leave a damaged tree that a later run would mistake for a pristine baseline.
#
# Usage:
#   scripts/realtest-setup.sh [PATH_TO_GAME_DIR]
#
# Defaults to a Skyrim Special Edition Steam library path; pass your own if it differs.

set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC_GAME="${1:-$HOME/SteamLibrary/steamapps/common/Skyrim Special Edition}"
SRC="$SRC_GAME/Data"
SANDBOX="$REPO/target/realtest/game"

if [[ ! -d "$SRC" ]]; then
  echo "error: no Data dir at '$SRC'" >&2
  echo "usage: $0 [PATH_TO_GAME_DIR]" >&2
  exit 1
fi

mkdir -p "$SANDBOX/Data"

# Plugins only: they carry the headers the scan classifies and are small enough to copy
# quickly. The .bsa archives add bulk without exercising anything the engine reasons about.
n=0
shopt -s nullglob
for f in "$SRC"/*.esm "$SRC"/*.esl "$SRC"/*.esp; do
  cp -n "$f" "$SANDBOX/Data/" 2>/dev/null && n=$((n + 1)) || true
done

total=$(find "$SANDBOX/Data" -type f | wc -l)
echo "copied $n new plugin file(s); sandbox now holds $total"
echo "sandbox: $SANDBOX"

if [[ "$total" -lt 10 ]]; then
  echo "warning: only $total files — the real-game tests want a fuller install" >&2
fi
