## Problem

<!-- Concrete trigger and observed wrong/missing behavior. Link the issue when useful. -->

## Change

<!-- Resulting behavior and affected contracts. For mechanics cite game stat IDs,
modifier text or official data; use PoB2 only as a cross-check. -->

## Validation

<!-- Exact focused commands and observed results; state anything not run.
No per-change full/parity run is required. Visible UI changes: before/after capture.
Docs-only: diff/link review is enough. CI owns its selected integration checks. -->

## Remaining limits

<!-- Known limits, or "none". -->

<!-- Before marking ready, a separate reviewer audits the current head and base.
Copy their actual conclusion into exactly one JSON record below. Get SHA values
from `gh pr view <N> --json headRefOid,baseRefOid`. A new head/base invalidates it.
Do not invent a reviewer session, review or passing checks. CI validates freshness
and schema; it cannot prove independent reasoning under shared credentials.

pobr-audit must start its own comment, for example:
-->

<!-- pobr-audit
{"head":"REPLACE_WITH_FULL_HEAD_SHA","base":"REPLACE_WITH_FULL_BASE_SHA","reviewer":"MODEL_AND_SESSION_OR_REVIEW_LINK","verdict":"pending","findings":[]}
-->
