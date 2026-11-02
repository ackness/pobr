# Historical compatibility workflows

This directory retains explicit local-table and pinned-vendor workflows. Current
builds instead use the immutable owner locks in
[`runtime/`](../runtime/build-data.md); these scripts do not discover or download
a new official dataset.

| Files | Existing entry or reader |
| --- | --- |
| `regen-all.sh`, `config.json` | `mise run data:legacy:regen`, with already available matching tables and receipts |
| `refresh-modifiers.sh` | Explicit parser-rule refresh/audit and the reference regeneration path |
| `bump-version.sh` | Compatibility wrapper forwarding to the lock-driven `data:update` implementation |
| `extract-*.lua`, `vendor-tree.lua`, `trade-*.lua` | Retained catalog extraction, tree selection and trade fallback checks |
| `lua-shims/` | Lua compatibility input used by the legacy regeneration path |
| `gem-quality/`, `advance_quality_receipt.py` | Fixed historical quality receipts and explicit local receipt verification/advancement |
| `test_*`, `test-*.lua` | Failure-isolation, foreign-key and compatibility regressions |

`config.json` selects historical local-table diagnostics, not the current data
channel. Preserve the declared patch, table fingerprints and source receipts.
Reports and candidate output belong in `.cache/`. Missing inputs remain a failure
or an explicit skip at the relevant boundary; compatibility is not source approval.
