#!/usr/bin/env bash
# Run the PoB2 headless parseMod differential oracle (M5b Track D-1).
#
# Reads modifier-text lines from stdin (or a --lines-file), emits JSONL of the
# PoB2 parseMod output per line. Resolves vendor paths relative to the repo.
#
# Usage:
#   echo "20% increased Fire Damage" | tools/pob2-oracle/run-parsemod.sh
#   tools/pob2-oracle/run-parsemod.sh --lines-file lines.txt > out.jsonl
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
POB_SRC="$REPO_ROOT/vendor/PathOfBuilding-PoE2/src"
RUNTIME="$REPO_ROOT/vendor/PathOfBuilding-PoE2/runtime/lua"

LUAJIT="${LUAJIT:-${POBR_LUAJIT:-/opt/homebrew/bin/luajit}}"

if [[ ! -d "$POB_SRC" ]]; then
	echo "run-parsemod: vendor PoB2 missing (${POB_SRC}) -- skip" >&2
	exit 3
fi
if [[ ! -x "$LUAJIT" ]] && ! command -v "$LUAJIT" >/dev/null 2>&1; then
	echo "run-parsemod: luajit unavailable (${LUAJIT}) -- skip" >&2
	exit 3
fi

# --lines-file path made absolute (script cd's into POB_SRC).
ARGS=()
i=1
while [[ $i -le $# ]]; do
	a="${!i}"
	if [[ "$a" == "--lines-file" ]]; then
		j=$((i + 1))
		f="${!j}"
		if [[ "$f" != /* ]]; then f="$(cd "$(dirname "$f")" && pwd)/$(basename "$f")"; fi
		ARGS+=("--lines-file" "$f")
		i=$((i + 2))
	else
		ARGS+=("$a")
		i=$((i + 1))
	fi
done

cd "$POB_SRC"
LUA_PATH="$RUNTIME/?.lua;$RUNTIME/?/init.lua;./?.lua;;" CI=true \
	"$LUAJIT" "$SCRIPT_DIR/parsemod.lua" "${ARGS[@]}"
