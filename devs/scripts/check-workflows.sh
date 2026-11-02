#!/usr/bin/env bash
# The cheap automation contract shared by local checks and both CI tiers.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
for script in devs/scripts/*.sh pipeline/*.sh tools/pob2-oracle/*.sh; do bash -n "$script"; done
# actionlint 1.7.12 predates GitHub's documented queue:max. Ignore only that
# unsupported key; keep all other syntax, expression and shell checks enabled.
# https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency
actionlint -ignore 'unexpected key "queue" for "concurrency" section'
python3 -m unittest discover -s devs/scripts -p 'test_*.py'
# The optional Lua/vendor extraction cases run in full CI with lua-dkjson.
python3 pipeline/test-data-update.py CompatibleUpdateTests
python3 pipeline/test-data-snapshot.py
python3 pipeline/test-native-candidate.py
python3 pipeline/test-data-bundle.py
python3 pipeline/check_data_ownership.py
python3 pipeline/test-build-data.py
python3 pipeline/test-locked-update.py
python3 pipeline/test-trade-crafting.py
python3 pipeline/test-display-assets.py
python3 pipeline/test-runtime-pack.py
python3 pipeline/test-runtime-ordinary.py
python3 pipeline/test-data-imports.py
python3 pipeline/test-locale-inputs.py
python3 pipeline/test-ordinary-inputs.py
python3 pipeline/test-source-verifier.py
python3 tools/pobr-data-adapter/tests/test_smoke_native_facts.py
node --test pipeline/test-native-name-translations.mjs pipeline/test-native-display-texts.mjs web/scripts/package-wasm.test.mjs web/scripts/sync-data.test.mjs web/scripts/bench-calc.test.mjs
