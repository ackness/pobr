# Data diagnostics

Run these from the repository root under `mise exec --`. They compare explicit
inputs and write reports; none selects `data/CURRENT` or publishes a dataset.

| Tool | Purpose |
| --- | --- |
| `diff_data.py` | Compare prepared versions, including optional semantic summaries; also used by `mise run data:diff` |
| `modifier-audit.sh` | Audit the prepared modifier rules (`precompile-mods --check`/`--audit`) and compare them with the pinned PoB2 parser unless `--offline`. Writes `.cache/modifier-audit/<version>/` and the prepared `generated/modifier-audit.json`; never extracts or rewrites rules. See [modifier audit](../README.md#词条审计) |
| `source_oracle.py` | Manually audit ordinary description ownership/compatibility against declared source and candidate files |

For Python argument help, use `python3 -m pipeline.diagnostics.diff_data --help`
or `python3 -m pipeline.diagnostics.source_oracle --help`. Supply the source,
legacy baseline and fresh report directory explicitly for the source audit.
Keep reports under `.cache/`; an inventory or comparison does not establish game
semantics, upstream freshness or successful runtime consumption.
