#!/usr/bin/env bash
# Compatibility entry point: release-check.sh [--detect-only] <base-rev>
set -euo pipefail
command=bump
if [ "${1:-}" = "--detect-only" ]; then
  command=detect
  shift
fi
exec python3 "$(dirname "${BASH_SOURCE[0]}")/release-policy.py" "$command" "$@"
