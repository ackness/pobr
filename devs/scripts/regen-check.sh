#!/usr/bin/env bash
# Check precompiled modifier rule regeneration on prepared data (CI data guard).
#
# Re-runs precompile-mods --report in a temporary directory without mutating the
# working tree; parse coverage must not drop below devs/ci/parse-coverage-baseline.json.
# Game facts, keystone derived rules, and language data come from owner releases
# verified by data:prepare; keystone consistency is checked by RePoE `adapter:keystones`.
# Fails when prepared data is missing: run `mise run data:prepare` first.
#
# Usage:
#   devs/scripts/regen-check.sh                      # Version from data/CURRENT
#   POBR_PATCH=0.5.5.5 devs/scripts/regen-check.sh   # Explicit version

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DATA_ROOT="${POBR_DATA_ROOT:-$ROOT/data}"

PATCH="${POBR_PATCH:-${POBR_DATA_VERSION:-}}"
if [[ -z "$PATCH" && -f "$DATA_ROOT/CURRENT" ]]; then
    PATCH="$(head -1 "$DATA_ROOT/CURRENT" | tr -d '[:space:]')"
fi
if [[ -z "$PATCH" ]]; then
    echo "regen-check: unable to determine version (missing $DATA_ROOT/CURRENT and POBR_PATCH is unset)" >&2
    exit 1
fi

COMMITTED_FLAT="$DATA_ROOT/$PATCH"
if [[ ! -d "$COMMITTED_FLAT" ]]; then
    echo "regen-check: $COMMITTED_FLAT does not exist; run mise run data:prepare first" >&2
    exit 1
fi

TMP_OUT="$(mktemp -d "${TMPDIR:-/tmp}/pobr-regen-check.XXXXXX")"
trap 'rm -rf "$TMP_OUT"' EXIT
echo "regen-check: patch=${PATCH}, temp output ${TMP_OUT}"

STATUS=0

# ============================================================================
# Modifier rule precompilation and coverage ratchet
# ----------------------------------------------------------------------------
# precompile-mods writes <data>/generated/{parsed_mods.json,parse-coverage.json}.
# Copy prepared data to an isolated directory first to keep the checkout clean;
# dereference symlinks so output cannot point back to the source directory.
# Corpora come from examples/.
# ============================================================================
BASELINE="$ROOT/devs/ci/parse-coverage-baseline.json"
echo "regen-check: running precompile-mods (modifier rule precompilation)..."
PC_ROOT="$TMP_OUT/precompile"
PC_DATA="$PC_ROOT/data/$PATCH"
mkdir -p "$PC_DATA"
cp -RL "$COMMITTED_FLAT/." "$PC_DATA/"
if [[ -d "$DATA_ROOT/overlay-common" ]]; then
    cp -RL "$DATA_ROOT/overlay-common" "$PC_ROOT/data/overlay-common"
fi
[[ ! -d "$ROOT/examples" ]] || ln -s "$ROOT/examples" "$PC_ROOT/examples"

cargo run --quiet -p precompile-mods --manifest-path "$ROOT/Cargo.toml" -- \
    --data "$PC_DATA" --report >/dev/null

COVERAGE="$PC_DATA/generated/parse-coverage.json"
if [[ ! -f "$COVERAGE" || ! -f "$BASELINE" ]]; then
    echo "regen-check: missing parse-coverage.json or baseline; cannot check ratchet" >&2
    exit 1
fi
read_ratio() {
    python3 - "$1" <<'PY'
import json, math, sys

with open(sys.argv[1], encoding="utf-8") as handle:
    document = json.load(handle)

value = None
if isinstance(document, dict):
    summary = document.get("summary")
    if isinstance(summary, dict) and "coverage_ratio" in summary:
        value = summary["coverage_ratio"]
    elif "coverage_ratio" in document:
        value = document["coverage_ratio"]

if not isinstance(value, (int, float)) or isinstance(value, bool) or not math.isfinite(value):
    raise SystemExit(f"regen-check: unable to read coverage_ratio from {sys.argv[1]}")

print(repr(float(value)))
PY
}
CUR_RATIO="$(read_ratio "$COVERAGE")"
BASE_RATIO="$(read_ratio "$BASELINE")"
if python3 - "$CUR_RATIO" "$BASE_RATIO" <<'PY'
import sys
cur, base = float(sys.argv[1]), float(sys.argv[2])
if cur + 0.0000005 < base:
    sys.exit(1)
if cur > base + 0.0000005:
    sys.exit(2)
sys.exit(0)
PY
then
    echo "regen-check: OK -- coverage ratchet met (current ${CUR_RATIO}, baseline ${BASE_RATIO})"
else
    cmp_rc=$?
    if [[ "$cmp_rc" -eq 1 ]]; then
        STATUS=1
        echo ""
        echo "regen-check: coverage ratchet failed -- current ${CUR_RATIO} < baseline ${BASE_RATIO}"
        echo "Parse coverage must not decrease; if expected (e.g. expanded corpus), update devs/ci/parse-coverage-baseline.json in the same PR"
    elif [[ "$cmp_rc" -eq 2 ]]; then
        echo "regen-check: note -- coverage ${CUR_RATIO} exceeds baseline ${BASE_RATIO}; you may raise the baseline in the same PR"
    else
        exit "$cmp_rc"
    fi
fi

exit "$STATUS"
