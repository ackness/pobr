#!/usr/bin/env bash
# Refresh parser artifacts and audit all shipped modifier sources against pinned PoB2.
# No downloads, version changes, baseline blessing, or calculation-golden updates.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
DATA="data/$(cat data/CURRENT)"
BASELINE=""
AUDIT_ONLY=0
ORACLE=1
while [[ $# -gt 0 ]]; do
    case "$1" in
        --data|--baseline)
            [[ $# -ge 2 ]] || { echo "Missing value for $1" >&2; exit 2; }
            if [[ "$1" == --data ]]; then DATA="$2"; else BASELINE="$2"; fi
            shift 2 ;;
        --audit-only) AUDIT_ONLY=1; shift ;;
        --offline) ORACLE=0; AUDIT_ONLY=1; shift ;;
        --help)
            echo "Usage: bash pipeline/refresh-modifiers.sh [--data data/<version>] [--baseline <audit.json>] [--audit-only|--offline]"
            exit 0 ;;
        *) echo "Unknown argument: $1" >&2; exit 2 ;;
    esac
done
[[ -d "$DATA" ]] || { echo "Missing data directory: $DATA" >&2; exit 2; }
DATA="$(cd "$DATA" && pwd)"
VERSION="$(basename "$DATA")"
if [[ "$AUDIT_ONLY" -eq 0 && "$DATA" != "$ROOT/data/$VERSION" ]]; then
    echo "Rule extraction requires a repository data/<version> directory; use --audit-only for external data." >&2
    exit 2
fi
VENDOR="$ROOT/vendor/PathOfBuilding-PoE2/src"
REPORT_DIR="$ROOT/.cache/modifier-audit/$VERSION"
mkdir -p "$REPORT_DIR" "$DATA/generated"
PRECOMPILE=(cargo run --quiet -p precompile-mods -- --data "$DATA")
SYNC=(cargo run --quiet -p sync-pob-catalog --)

if [[ "$ORACLE" -eq 1 ]]; then
    PIN="$(python3 - "$DATA/overlay/mod_parser_rules.json" <<'PY'
import json, sys
with open(sys.argv[1], encoding="utf-8") as source:
    print(json.load(source)["_meta"]["vendor_commit"])
PY
)"
    ACTUAL="$(git -C "$VENDOR" rev-parse HEAD)"
    [[ "$PIN" == "$ACTUAL" ]] || {
        echo "Vendor differs from data pin ($PIN); align vendor or regenerate the version first." >&2
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

if [[ "$AUDIT_ONLY" -eq 0 ]]; then
    "${SYNC[@]}" extract-lua --what parser-rules --vendor-root "$VENDOR" --out "$DATA/overlay/mod_parser_rules.json"
    POBR_DATA_VERSION="$VERSION" "${SYNC[@]}" extract-lua --what special-mods --vendor-root "$VENDOR" --out "$DATA/generated/special_vendor.json"
fi

# Validate the effective rules before publishing audit or parser artifacts.
"${PRECOMPILE[@]}" --check > "$REPORT_DIR/validation.json"

AUDIT_ARGS=(--audit "$REPORT_DIR/current.json" --baseline "$PREVIOUS")
if [[ "$ORACLE" -eq 1 ]]; then
    "${PRECOMPILE[@]}" --audit-corpus "$REPORT_DIR/corpus.txt"
    POBR_VENDOR_ROOT="$ROOT/vendor" bash tools/pob2-oracle/run-parsemod.sh --lines-file "$REPORT_DIR/corpus.txt" > "$REPORT_DIR/oracle.jsonl"
    AUDIT_ARGS+=(--oracle-results "$REPORT_DIR/oracle.jsonl")
fi
"${PRECOMPILE[@]}" "${AUDIT_ARGS[@]}"
cp "$REPORT_DIR/current.json" "$DATA/generated/modifier-audit.json"
if [[ "$AUDIT_ONLY" -eq 0 ]]; then
    "${PRECOMPILE[@]}" --report
fi
echo "Modifier audit: $REPORT_DIR/current.json"
echo "Changes and unresolved new wording: $REPORT_DIR/current.delta.json"
