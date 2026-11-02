#!/usr/bin/env bash
# Offline data-boundary check, run after data:prepare on a clean checkout (CI does so).
# 1. Tracked data/ may hold only CURRENT and the PoBR-owned shared rules.
# 2. Imports must not leave untracked, unignored files or modify tracked files.
# No network access and no import.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
python3 -m pipeline.imports.check_data_ownership
status="$(git status --porcelain --untracked-files=all)"
if [[ -n "$status" ]]; then
  echo "data:check: the worktree is not clean after import; add an ignore rule or fix the writer:" >&2
  printf '%s\n' "$status" >&2
  exit 1
fi
echo "data:check: tracked data ownership OK; worktree clean"
