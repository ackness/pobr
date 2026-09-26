#!/usr/bin/env bash
# Record provenance only after a complete successful exporter run.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
exec python3 pipeline/data_snapshot.py export
