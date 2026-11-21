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

PATCH="${POBR_PATCH:-}"
if [[ -z "$PATCH" && -f "$ROOT/data/CURRENT" ]]; then
    PATCH="$(head -1 "$ROOT/data/CURRENT" | tr -d '[:space:]')"
fi
if [[ -z "$PATCH" ]]; then
    echo "regen-check: unable to determine version (missing data/CURRENT and POBR_PATCH is unset)" >&2
    exit 1
fi

COMMITTED_FLAT="$ROOT/data/$PATCH"
if [[ ! -d "$COMMITTED_FLAT" ]]; then
    echo "regen-check: data/$PATCH does not exist; run mise run data:prepare first" >&2
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
if [[ -d "$ROOT/data/overlay-common" ]]; then
    cp -RL "$ROOT/data/overlay-common" "$PC_ROOT/data/overlay-common"
fi
[[ ! -d "$ROOT/examples" ]] || ln -s "$ROOT/examples" "$PC_ROOT/examples"

cargo run --quiet -p precompile-mods --manifest-path "$ROOT/Cargo.toml" -- \
    --data "$PC_DATA" --report >/dev/null

COVERAGE="$PC_DATA/generated/parse-coverage.json"
if [[ ! -f "$COVERAGE" || ! -f "$BASELINE" ]]; then
    echo "regen-check: missing parse-coverage.json or baseline; cannot check ratchet" >&2
    exit 1
fi
CUR_RATIO="$(sed -n 's/.*"coverage_ratio"[[:space:]]*:[[:space:]]*\([0-9.]*\).*/\1/p' "$COVERAGE" | head -1)"
BASE_RATIO="$(sed -n 's/.*"coverage_ratio"[[:space:]]*:[[:space:]]*\([0-9.]*\).*/\1/p' "$BASELINE" | head -1)"
if [[ -z "$CUR_RATIO" || -z "$BASE_RATIO" ]]; then
    echo "regen-check: unable to read coverage_ratio" >&2
    exit 1
fi
# Floating-point comparison via awk.
if awk "BEGIN { exit !(${CUR_RATIO} + 0.0000005 < ${BASE_RATIO}) }"; then
    STATUS=1
    echo ""
    echo "regen-check: coverage ratchet failed -- current ${CUR_RATIO} < baseline ${BASE_RATIO}"
    echo "Parse coverage must not decrease; if expected (e.g. expanded corpus), update devs/ci/parse-coverage-baseline.json in the same PR"
elif awk "BEGIN { exit !(${CUR_RATIO} > ${BASE_RATIO} + 0.0000005) }"; then
    echo "regen-check: note -- coverage ${CUR_RATIO} exceeds baseline ${BASE_RATIO}; you may raise the baseline in the same PR"
else
    echo "regen-check: OK -- coverage ratchet met (current ${CUR_RATIO}, baseline ${BASE_RATIO})"
fi

exit "$STATUS"
