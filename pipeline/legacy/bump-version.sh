#!/usr/bin/env bash
# Exact external-owner candidate/selection; no discovery, raw export or vendor mutation.
# Usage: mise exec -- bash pipeline/legacy/bump-version.sh --lock <lock> --out <fresh-dir>
#        Add --activate --expected-current <version> only for explicit local selection.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
exec python3 -m pipeline.runtime.locked_update "$@"
