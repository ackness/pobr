#!/usr/bin/env bash
# The cheap automation contract shared by local checks and both CI tiers.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
bash -n devs/hooks/pre-push
while IFS= read -r -d '' script; do bash -n "$script"; done < <(
  find devs/scripts pipeline tools/pob2-oracle -type f -name '*.sh' -print0
)
# actionlint 1.7.12 predates GitHub's documented queue:max. Ignore only that
# unsupported key; keep all other syntax, expression and shell checks enabled.
# https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency
actionlint -ignore 'unexpected key "queue" for "concurrency" section'
python3 -m unittest discover -s devs/scripts -p 'test_*.py'
python3 -m unittest discover -s web/scripts -p '*_test.py'
python3 -m pipeline.runtime.test_data_snapshot
python3 -m pipeline.runtime.test_data_bundle
python3 -m pipeline.imports.check_data_ownership
python3 -m pipeline.runtime.test_build_data
python3 -m pipeline.runtime.test_locked_update
python3 -m pipeline.imports.test_display_assets
python3 -m pipeline.runtime.test_runtime_pack
python3 -m pipeline.imports.test_data_imports
python3 -m pipeline.imports.test_data_bump
node --test web/scripts/*.test.mjs
