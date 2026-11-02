#!/usr/bin/env bash
# The cheap automation contract shared by local checks and both CI tiers.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
while IFS= read -r -d '' script; do bash -n "$script"; done < <(
  find devs/scripts pipeline tools/pob2-oracle -type f -name '*.sh' -print0
)
# actionlint 1.7.12 predates GitHub's documented queue:max. Ignore only that
# unsupported key; keep all other syntax, expression and shell checks enabled.
# https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency
actionlint -ignore 'unexpected key "queue" for "concurrency" section'
python3 -m unittest discover -s devs/scripts -p 'test_*.py'
# The optional Lua/vendor extraction cases run in full CI with lua-dkjson.
python3 -m pipeline.legacy.test_data_update CompatibleUpdateTests
python3 -m pipeline.runtime.test_data_snapshot
python3 -m pipeline.tree.test_tree_inputs
python3 -m pipeline.native.test_native_candidate
python3 -m pipeline.runtime.test_data_bundle
python3 -m pipeline.imports.check_data_ownership
python3 -m pipeline.runtime.test_build_data
python3 -m pipeline.runtime.test_locked_update
python3 -m pipeline.latest.test_latest_data
python3 -m pipeline.latest.test_latest_sync
python3 -m pipeline.imports.test_trade_crafting
python3 -m pipeline.imports.test_display_assets
python3 -m pipeline.runtime.test_runtime_pack
python3 -m pipeline.runtime.ordinary.test_runtime_ordinary
python3 -m pipeline.imports.test_data_imports
python3 -m pipeline.imports.test_locale_inputs
python3 -m pipeline.runtime.ordinary.test_ordinary_inputs
python3 -m pipeline.imports.test_source_verifier
python3 tools/pobr-data-adapter/tests/test_smoke_native_facts.py
node --test pipeline/native/test-native-name-translations.mjs pipeline/native/test-native-display-texts.mjs pipeline/native/test-native-socketable-translations.mjs web/scripts/package-wasm.test.mjs web/scripts/sync-data.test.mjs web/scripts/bench-calc.test.mjs web/scripts/smoke-runtime-candidate.test.mjs
