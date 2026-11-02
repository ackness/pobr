# Latest complete data publications

Normal builds consume immutable locks. `data:latest` separately discovers the
latest complete publication from RePoE `master` and translation `main`, each at
`publication/index.json`. These branches locate metadata; runtime inputs still
use full Git revisions, sizes and SHA-256 pins. Handoff branches retain immutable
artifacts and do not select the active dataset.

The owner index (`owned-data-publication-index/v2`) points to an immutable
`owned-data-publication/v2` descriptor. RePoE declares game, required crafting,
ordinary facts, display source, source verifier and fixtures. Translation
declares retained language, display catalog/library, reviewed supplements,
workbench, support display and locale generation, plus its exact display-source
binding and catalog-validation evidence. Scope remains verbatim. Dependency
completeness does not certify client identity, game semantics or translation
quality. The old v1 RePoE handoff table lacks crafting and cannot be a latest
channel.

`data-composition.json` pins both owner publications and every consumer lock,
request, source decision and channel configuration. Its generation and previous
composition digest distinguish updates within the same patch. Each owner and
role has a monotonic sequence with an exact predecessor; forks, regressions,
empty publication advances and incomplete roles fail. Code-only owner commits
with unchanged publication metadata do not create data updates.

```sh
# Report latest indexed publications and the independent official observation.
mise run data:latest check
# Freeze a fresh metadata candidate; installed files and defaults stay unchanged.
mise run data:latest resolve --out .cache/latest-review
# Offline byte/evidence replay, without a freshness claim or runtime acceptance.
mise run data:latest verify --root .cache/latest-review
# Verify the accepted immutable declarations, without resolving current heads.
mise run data:latest verify-composition
# Stage only that frozen small metadata with an exact baseline guard.
mise run data:latest apply --candidate .cache/latest-review \
  --expected-composition <previous-composition-sha256>
```

`bootstrap --out pipeline/latest/data-composition.json` is a one-time registration of
already accepted locks against published, reachable owner indexes. It requires
every declaration to match; it does not choose new data. Bootstrap output must
be fresh. The initial descriptor commits must remain reachable through merge
history or persistent publication refs. Squashing them away and deleting their
only branch would break fixed Git inputs.

Discovery resolves each owner head once before reading either index. The official
version observer is the pinned independent RePoE module, with a bounded invocation
and receipt age. Its result describes the endpoint advertisement only. If the
official version is newer than either complete owner publication, the check
reports incomplete freshness and preserves accepted defaults. Unavailable,
stale, corrupt or missing metadata never falls back to an old "latest" result.
`--observation <directory>` permits reuse of an existing fresh observer receipt;
it still checks its pins, producer and age.

Same-target candidates preserve source decisions, local policies, shared rules
and owner scopes. Changed game/language inputs derive expectations from pinned
package metadata, followed by actual preparation in Full CI. Ordinary input
checksums follow the new authenticated manifest. Existing source/policy guards
still apply. A new patch requires compatible owned requirements and source
decisions through a reviewed migration; patch strings cannot relabel old data.

Full CI verifies the complete snapshot and recorded immutable owner declarations
(without a new official query or mutable channel lookup), produces all ordinary
domains and the display sidecar, then requires projection equivalence between the full and compact
consumers. Only existing audit-header projection is allowed; missing members,
numeric token changes, runtime provenance or display differences reject the
candidate. It downloads the same run's standalone WASM, materializes the final
compact pack and runs `smoke-runtime-candidate.mjs` against that candidate alone.
The package's old embedded data is never a fallback. Calculation provenance,
worked builds, editing/reopening and preserved diagnostics enter its receipt.

## Automatic proposal and adoption

[data-sync.yml](../../.github/workflows/data-sync.yml) checks every six hours, on
manual dispatch or `owned-data-published` repository dispatch. One reserved branch,
`automation/data-latest`, carries only the small metadata closure. Proposal uses
the exact current default commit as parent and an expected-SHA branch lease;
interrupted PR creation or Full dispatch can be retried without a second branch.
There is no automatic app release or deployment.

The controller uses trusted default-branch code. A `workflow_run` completion for
the reserved branch rechecks its PR, exact head, baseline composition, current
owner frontier and official observation. The latest `Contribution checks` PR
run must contain a successful `PR gate`; a separate exact-head dispatched
`Full validation` run must contain a successful `Full gate`. Missing, queued,
approval-required, skipped, failed and cancelled gates all remain incomplete.
Candidate code is never checked out or executed by this privileged callback.
Scheduled and manual proposals also reconcile an already published candidate:
if a completion callback failed temporarily, the next check can adopt the same
still-fresh head using its successful gates, without repeating Full CI. Incomplete
gates keep the candidate waiting.

Unattended PR checks require an App installation token or PAT configured as
`DATA_SYNC_TOKEN`, with repository contents, pull requests and Actions permissions
needed by the controller. A default `GITHUB_TOKEN` is sufficient for read-only
checks. The dedicated credential must also be able to read branch protection
so acceptance can verify the strict base guard. A default `GITHUB_TOKEN` does not
establish unattended PR CI. Without dedicated auth, a changed
publication reports `needs-data-sync-token` before pushing a proposal.

Automatic adoption additionally requires hosted master protection with an
up-to-date required `PR gate` and enforcement for administrators. This closes the
base-change race between preflight and GitHub's head-conditional merge; workflow
concurrency alone cannot do so. Missing protection reports
`needs-strict-branch-checks` and preserves defaults. These hosted settings and the
secret are activation requirements, not effects of committing the workflow.
Full gate remains an additional controller requirement, not a global requirement
for unrelated documentation PRs.

`latest-sync-result` records the observed outcome and exact checks. Successful
adoption changes default metadata through the normal protected PR merge. Existing
local installations select accepted data explicitly via
[locked update](../runtime/locked-update.md), including `--expected-runtime` for same-patch
CAS. Git history retains prior compositions for rollback.

Owner publication indexes register completed production. Their bootstrap alone
does not implement upstream discovery, new client collection, translation review
or autonomous data generation. If an owner has not produced a complete new
publication, the consumer reports that gap; it cannot invent one from an official
version number or source branch date.
