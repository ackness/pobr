#!/usr/bin/env bash
# Run the PoB2 headless parse-mod oracle (M5b-D1 live differential + M6-C golden).
#
# Thin runner around `oracle.lua --mode <parsemod|modcache-dump>` (the roadmap's
# "oracle.lua --mode parsemod" entry point). Resolves vendor paths relative to the
# repo, with an override so it runs from an isolation worktree (where vendor/ is
# gitignored and only lives in the main checkout).
#
# Usage:
#   # Live parse-mod (stdin or --lines-file) -> JSONL of vendor parseMod output:
#   echo "20% increased Fire Damage" | tools/pob2-oracle/run-parsemod.sh
#   tools/pob2-oracle/run-parsemod.sh --lines-file lines.txt > out.jsonl
#
#   # Offline golden: dump vendor Data/ModCache.lua -> canonical golden JSON:
#   tools/pob2-oracle/run-parsemod.sh --mode modcache-dump --out modcache_golden.json
#
# Default --mode is parsemod (back-compat with the M5b-D1 invocation).
#
# Vendor override (for worktrees): set POBR_VENDOR_ROOT to a checkout that
# contains PathOfBuilding-PoE2/, e.g.
#   POBR_VENDOR_ROOT=/path/to/main/repo/vendor tools/pob2-oracle/run-parsemod.sh ...
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
VENDOR_ROOT="${POBR_VENDOR_ROOT:-$REPO_ROOT/vendor}"
POB_SRC="$VENDOR_ROOT/PathOfBuilding-PoE2/src"
RUNTIME="$VENDOR_ROOT/PathOfBuilding-PoE2/runtime/lua"

LUAJIT="${LUAJIT:-${POBR_LUAJIT:-/opt/homebrew/bin/luajit}}"

if [[ ! -d "$POB_SRC" ]]; then
	echo "run-parsemod: vendor PoB2 missing (${POB_SRC}) -- skip" >&2
	echo "run-parsemod: set POBR_VENDOR_ROOT to a checkout containing PathOfBuilding-PoE2/" >&2
	exit 3
fi
if [[ ! -x "$LUAJIT" ]] && ! command -v "$LUAJIT" >/dev/null 2>&1; then
	echo "run-parsemod: luajit unavailable (${LUAJIT}) -- skip" >&2
	exit 3
fi

# Detect explicit --mode; default to parsemod. Make --lines-file / --out paths
# absolute (the runner cd's into POB_SRC before invoking luajit).
HAS_MODE=0
ARGS=()
i=1
while [[ $i -le $# ]]; do
	a="${!i}"
	case "$a" in
		--mode)
			HAS_MODE=1
			j=$((i + 1))
			ARGS+=("--mode" "${!j}")
			i=$((i + 2))
			;;
		--lines-file | --out)
			j=$((i + 1))
			f="${!j}"
			if [[ "$f" != /* ]]; then f="$(cd "$(dirname "$f")" && pwd)/$(basename "$f")"; fi
			ARGS+=("$a" "$f")
			i=$((i + 2))
			;;
		*)
			ARGS+=("$a")
			i=$((i + 1))
			;;
	esac
done
if [[ "$HAS_MODE" -eq 0 ]]; then
	ARGS=("--mode" "parsemod" "${ARGS[@]}")
fi

cd "$POB_SRC"
LUA_PATH="$RUNTIME/?.lua;$RUNTIME/?/init.lua;./?.lua;;" CI=true \
	"$LUAJIT" "$SCRIPT_DIR/oracle.lua" "${ARGS[@]}"
