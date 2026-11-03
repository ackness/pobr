# Data inputs and owned rules

PoBR Git tracks `CURRENT` and the owned rules in `overlay-common/`. Complete game,
language and other owner-produced inputs are prepared locally from immutable locks.
See the [pipeline guide](../pipeline/README.md) for the source and adoption boundaries.

| Path | Purpose |
| --- | --- |
| `CURRENT` | Selected local game-data version; not a claim of upstream freshness |
| Tracked `overlay-common/*.json` | PoBR-owned rules and curation |
| Ignored `overlay-common/*.json` | Shared files supplied by the selected data lock |
| Ignored `<version>/` | Prepared runtime snapshot with a verified manifest |

Prepare the selected combination with `mise run data:prepare`. This imports pinned
inputs; it does not discover a new upstream release or adopt a candidate.
The [build-data guide](../pipeline/runtime/build-data.md) describes explicit candidate and
historical-lock preparation. CI prepares only the version that
[`pipeline/data.lock.json`](../pipeline/data.lock.json) declares; a historical
snapshot never replaces `CURRENT`.

Edit owned rules in their tracked files. Keep generated snapshots, source downloads
and validation reports out of Git. Temporary outputs belong in `.cache/`; preserve
source pins and receipts before removing material needed to explain an adoption.
