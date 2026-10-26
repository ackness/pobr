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
VENDOR_ROOT="${POBR_VENDOR_ROOT:-$REPO_ROOT/vendor}"
POB_SRC="$VENDOR_ROOT/PathOfBuilding-PoE2/src"
RUNTIME="$VENDOR_ROOT/PathOfBuilding-PoE2/runtime/lua"
# Pure-Lua shims (e.g. lua-utf8) injected ahead of the vendor runtime so the
# headless wrapper resolves modules missing as native libs on this platform.
SHIM="$SCRIPT_DIR/shim"

# The pinned PoB2 uses +=, which older distro/Homebrew LuaJIT cannot parse.
LUAJIT="$(bash "$REPO_ROOT/devs/scripts/pob2-luajit.sh" resolve)"

XML="${1:?usage: run.sh <decoded.xml> [out.json]}"
OUT="${2:-}"

# Make the XML path absolute (the script cd's into POB_SRC).
if [[ "$XML" != /* ]]; then XML="$(cd "$(dirname "$XML")" && pwd)/$(basename "$XML")"; fi
if [[ -n "$OUT" && "$OUT" != /* ]]; then OUT="$(pwd)/$OUT"; fi

cd "$POB_SRC"
# Close stdin (</dev/null): if PoB's headless startup ever sets a promptMsg it
# calls io.read("*l") and would otherwise block forever waiting on a TTY.
LUA_PATH="$SHIM/?.lua;$RUNTIME/?.lua;$RUNTIME/?/init.lua;./?.lua;;" CI=true \
	"$LUAJIT" "$SCRIPT_DIR/oracle.lua" "$XML" "$OUT" </dev/null
