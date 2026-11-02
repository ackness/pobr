#!/usr/bin/env bash
# Audit the prepared modifier rules against the pinned PoB2 parser (test reference).
# Read-only for rules: it never extracts, downloads, changes versions or blesses baselines.
# The report goes to .cache/modifier-audit/<version>/ and generated/modifier-audit.json
# of the prepared (ignored) data directory.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
DATA="data/$(cat data/CURRENT)"
BASELINE=""
ORACLE=1
while [[ $# -gt 0 ]]; do
    case "$1" in
        --data|--baseline)
            [[ $# -ge 2 ]] || { echo "Missing value for $1" >&2; exit 2; }
            if [[ "$1" == --data ]]; then DATA="$2"; else BASELINE="$2"; fi
            shift 2 ;;
        --offline) ORACLE=0; shift ;;
        --help)
            echo "Usage: bash pipeline/diagnostics/modifier-audit.sh [--data data/<version>] [--baseline <audit.json>] [--offline]"
            exit 0 ;;
        *) echo "Unknown argument: $1" >&2; exit 2 ;;
    esac
done
[[ -d "$DATA" ]] || { echo "Missing data directory: $DATA (run mise run data:prepare)" >&2; exit 2; }
DATA="$(cd "$DATA" && pwd)"
VERSION="$(basename "$DATA")"
VENDOR="$ROOT/vendor/PathOfBuilding-PoE2/src"
REPORT_DIR="$ROOT/.cache/modifier-audit/$VERSION"
mkdir -p "$REPORT_DIR" "$DATA/generated"
PRECOMPILE=(cargo run --quiet -p precompile-mods -- --data "$DATA")

if [[ "$ORACLE" -eq 1 ]]; then
    PIN="$(bash devs/scripts/driver.sh reference commit)"
    ACTUAL="$(git -C "$VENDOR" rev-parse HEAD)"
    [[ "$PIN" == "$ACTUAL" ]] || {
        echo "Vendor differs from reference pin ($PIN); run mise run vendor." >&2
        exit 2
    }
fi

# Keep the previous report separate: a failed audit must not bless its own regression.
PREVIOUS="$(mktemp "${TMPDIR:-/tmp}/pobr-modifier-baseline.XXXXXX")"
trap 'rm -f "$PREVIOUS"' EXIT
if [[ -n "$BASELINE" ]]; then
    cp "$BASELINE" "$PREVIOUS"
elif [[ -f "$DATA/generated/modifier-audit.json" ]]; then
    cp "$DATA/generated/modifier-audit.json" "$PREVIOUS"
else
    "${PRECOMPILE[@]}" --audit "$PREVIOUS"
fi

# Validate the effective rules before writing any audit artifact.
"${PRECOMPILE[@]}" --check > "$REPORT_DIR/validation.json"

AUDIT_ARGS=(--audit "$REPORT_DIR/current.json" --baseline "$PREVIOUS")
if [[ "$ORACLE" -eq 1 ]]; then
    "${PRECOMPILE[@]}" --audit-corpus "$REPORT_DIR/corpus.txt"
    POBR_VENDOR_ROOT="$ROOT/vendor" bash tools/pob2-oracle/run-parsemod.sh --lines-file "$REPORT_DIR/corpus.txt" > "$REPORT_DIR/oracle.jsonl"
    AUDIT_ARGS+=(--oracle-results "$REPORT_DIR/oracle.jsonl")
fi
"${PRECOMPILE[@]}" "${AUDIT_ARGS[@]}"
cp "$REPORT_DIR/current.json" "$DATA/generated/modifier-audit.json"
echo "Modifier audit: $REPORT_DIR/current.json"
echo "Changes and unresolved new wording: $REPORT_DIR/current.delta.json"
