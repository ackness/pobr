# Latest complete data publications

Normal builds consume the immutable [`pipeline/data.lock.json`](../data.lock.json).
`data:latest` separately discovers the latest complete publication from RePoE
`master` and translation `main`, each at `publication/index.json`. These branches
locate metadata; runtime inputs still use full Git revisions, sizes and SHA-256
pins. Owner `handoff/*` tags keep the pinned commits alive and do not select the
active dataset.

The owner index (`owned-data-publication-index/v2`) points to an immutable
`owned-data-publication/v2` descriptor. An owner may declare more roles than
PoBR consumes. PoBR reads RePoE `game` and `test_fixtures`, and
translation `language` and `support_display`; the translation descriptor's
display-source binding must equal RePoE's `display_source` role, and its
catalog-validation evidence is checked. Scope remains verbatim. Dependency
completeness does not certify client identity, game semantics or translation
quality. RePoE also publishes the neutral `crafting` facts; its game part
already carries the catalog projected from them, so PoBR does not read that role.
The old v1 RePoE handoff table cannot be a latest channel.

The data lock records both owner publications next to the consumed artifacts.
Its generation and previous lock digest distinguish updates within the same patch;
`latest-channels.json` is a separate small configuration validated on every run. Each owner and
role has a monotonic sequence with an exact predecessor; forks, regressions,
empty publication advances and incomplete roles fail. Code-only owner commits
with unchanged publication metadata do not create data updates.

```sh
# Report latest indexed publications and the independent official observation.
mise run data:latest check
# Freeze a fresh metadata candidate; installed files and defaults stay unchanged.
mise run data:latest resolve --out .cache/latest-review
# A different game_patch additionally requires the explicit --new-target flag.
mise run data:latest resolve --new-target --out .cache/latest-review-next
# Offline byte/evidence replay, without a freshness claim or runtime acceptance.
mise run data:latest verify --root .cache/latest-review
# Verify the accepted immutable declarations, without resolving current heads.
mise run data:latest verify-composition
# Stage only the frozen data lock with an exact baseline guard.
mise run data:latest apply --candidate .cache/latest-review \
  --expected-lock <previous-data-lock-sha256>
```

`bootstrap --out <fresh-file>` records the current owner frontiers for a data
lock whose consumed artifacts already match published, reachable owner indexes. It requires
every declaration to match; it does not choose new data. Bootstrap output must
be fresh. The initial descriptor commits must remain reachable through merge
history or `handoff/*` tags. Squashing them away or deleting the tag would break
fixed Git inputs.

Discovery resolves each owner head once before reading either index. The official
version observer is the pinned independent RePoE module, with a bounded invocation
and receipt age. Its result describes the endpoint advertisement only. If the
official version is newer than either complete owner publication, the check
reports incomplete freshness and preserves accepted defaults. Unavailable,
stale, corrupt or missing metadata never falls back to an old "latest" result.
`--observation <directory>` permits reuse of an existing fresh observer receipt;
it still checks its pins, producer and age.

Same-target candidates preserve local policies, shared rules and owner scopes.
Each changed consumed artifact must have exactly one owner `handoff/*` tag naming
its revision; the candidate records it. Changed game or language inputs
derive the snapshot manifest and runtime identity from pinned package metadata;
the resolver then prepares the candidate's complete data and records its exact
compact `release_id`, which the candidate Full CI rebuilds and must match.
Owners may add or retire roles PoBR does not consume; a consumed role, the RePoE
display source and the language catalog binding must stay present. A new patch requires compatible owned requirements and source
decisions through a reviewed migration; patch strings cannot relabel old data.
When both owner publications name the identical new `game_patch` for the same
game and realm, `resolve --new-target` freezes an explicit candidate carrying
that target, the new `data/CURRENT`, the recomputed identities and the
`target_change` receipt entry. It is never automatic: `auto_eligible` is false
for a changed target, so scheduled proposal and `recheck` never adopt it, and
the consumer migration lands in the same reviewed change.

Full CI verifies the complete snapshot, the recorded immutable owner declarations
and owner tags (without a new official query or mutable channel lookup), then
builds the compact release from the prepared snapshot. It downloads the same
run's standalone WASM, materializes the final compact pack and runs
`smoke-runtime-candidate.mjs` against that candidate alone.
The package's old embedded data is never a fallback. Calculation provenance,
worked builds, editing/reopening and preserved diagnostics enter its receipt.

## Rollout status

Current owner publication sequences, hosted activation conditions and the
remaining end-to-end exercise live in
[the current data status](../../docs/data-status.md#当前数据锁与托管条件).
`auto_eligible` from `check` describes metadata/freshness eligibility only, not
credential readiness or adoption. Do not repeat bootstrap registration or decode
unchanged large inputs to establish progress.

## Automatic proposal and adoption

[data-sync.yml](../../.github/workflows/data-sync.yml) checks every six hours, on
manual dispatch or `owned-data-published` repository dispatch. One reserved branch,
`automation/data-latest`, carries only the data lock. Proposal uses
the exact current default commit as parent and an expected-SHA branch lease;
interrupted PR creation or Full dispatch can be retried without a second branch.
Neither owner repository sends the `owned-data-published` repository dispatch
yet, so the six-hour schedule is the effective trigger.
There is no automatic app release or deployment.

The controller uses trusted default-branch code. A `workflow_run` completion for
the reserved branch rechecks its PR, exact head, baseline data lock, current
owner frontier and official observation. The latest `Contribution checks` PR
run must contain a successful `PR gate`; a separate exact-head dispatched
`Full validation` run must contain a successful `Full gate`. Missing, queued,
approval-required, skipped, failed and cancelled gates all remain incomplete.
Candidate code is never checked out or executed by this privileged callback.
Scheduled and manual proposals also reconcile an already published candidate:
if a completion callback failed temporarily, the next check can adopt the same
still-fresh head using its successful gates, without repeating Full CI. Incomplete
gates keep the candidate waiting.

Completion callbacks report `candidate-awaiting-gates` when either gate is not
successful, including its observed run status and conclusion. This is a normal
refusal to adopt: the failing validation run remains failed, and no merge occurs.
Invalid protection, changed inputs, authentication and infrastructure errors
still fail the synchronization run.

Some repository tests bind reviewed facts to the complete snapshot manifest.
A game update can therefore require explicit review of
`crates/pobr-gamedata/tests/fixtures/repository-test-pins.json`, even when those
particular facts are unchanged. Keep the automatic branch restricted to the
frozen data declaration. Review the changed members and update any affected
test bindings through a normal PR with the candidate lock; do not automatically
bless values or bypass a failed Full gate. Manual merges need the same validation
judgment: the required `PR gate` alone proves only the quick checks.

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
CAS. Git history retains prior data locks for rollback.

Owner publication indexes register completed production. Their bootstrap alone
does not implement upstream discovery, new client collection, translation review
or autonomous data generation. If an owner has not produced a complete new
publication, the consumer reports that gap; it cannot invent one from an official
version number or source branch date.
