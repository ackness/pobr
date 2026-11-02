#!/usr/bin/env bash
# Exact external-owner candidate/selection; no discovery, raw export or vendor mutation.
# Usage: mise exec -- bash pipeline/bump-version.sh --lock <lock> --out <fresh-dir>
#        Add --activate --expected-current <version> only for explicit local selection.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
exec python3 "$ROOT/pipeline/locked_update.py" "$@"
