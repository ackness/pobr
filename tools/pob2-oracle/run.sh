#!/usr/bin/env bash
# Run the PoB2 headless calc oracle on a decoded build XML.
#
# Usage:
#   tools/pob2-oracle/run.sh <decoded.xml> [out.json]
#
# Resolves paths relative to the repo so it can be invoked from anywhere.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
POB_SRC="$REPO_ROOT/vendor/PathOfBuilding-PoE2/src"
RUNTIME="$REPO_ROOT/vendor/PathOfBuilding-PoE2/runtime/lua"

LUAJIT="${LUAJIT:-/opt/homebrew/bin/luajit}"

XML="${1:?usage: run.sh <decoded.xml> [out.json]}"
OUT="${2:-}"

# Make the XML path absolute (the script cd's into POB_SRC).
if [[ "$XML" != /* ]]; then XML="$(cd "$(dirname "$XML")" && pwd)/$(basename "$XML")"; fi
if [[ -n "$OUT" && "$OUT" != /* ]]; then OUT="$(pwd)/$OUT"; fi

cd "$POB_SRC"
LUA_PATH="$RUNTIME/?.lua;$RUNTIME/?/init.lua;./?.lua;;" CI=true \
	"$LUAJIT" "$SCRIPT_DIR/oracle.lua" "$XML" "$OUT"
