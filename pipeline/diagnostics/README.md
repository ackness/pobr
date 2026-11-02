# Data diagnostics

Run these from the repository root under `mise exec --`. They compare explicit
inputs and write reports; none selects `data/CURRENT` or publishes a dataset.

| Tool | Purpose |
| --- | --- |
| `diff_data.py` | Compare prepared versions, including optional semantic summaries; also used by `mise run data:diff` |
| `diff-vendor-calcs.sh` | Compare two pinned vendor revisions and their extracted catalogs |
| `json_entry_diff.py` | Entry-level JSON helper used by the vendor comparison |
| `source_oracle.py` | Manually audit ordinary description ownership/compatibility against declared source and candidate files |

For Python argument help, use `python3 -m pipeline.diagnostics.diff_data --help`
or `python3 -m pipeline.diagnostics.source_oracle --help`. Supply the source,
legacy baseline and fresh report directory explicitly for the source audit.
Keep reports under `.cache/`; an inventory or comparison does not establish game
semantics, upstream freshness or successful runtime consumption.
