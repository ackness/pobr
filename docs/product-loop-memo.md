# PoBR product-loop memo

## Durable steering

- 2026-10-11: user correction: this is a **PoE2 game tool**, not an incremental
  game and not a game. Reference PoBR and adjacent PoE2 tools. Game hooks,
  incremental progression and gamification are out of scope.
- Main Agent owns goal, decisions, integration and Git. Parallel independent
  agents are encouraged only where they improve actual product outcomes.
- Explicit natural-language steering wins immediately; suggestions are evaluated
  against the current milestone. Preserve important preferences across triggers.
- Verified, independently valuable progress gets focused Conventional Commits.
  Release-worthy integrated milestones get annotated tags under existing rules.
  Do not optimize for task completion, agent volume or producing commits.
- No push, PR or deployment authorization was given. Keep work on the existing
  `multi-agent-self-improvement` branch and preserve unrelated changes.

## Active milestone M1: trustworthy, continuous upgrade decisions

Outcome: a player can inspect a build, compare equipment/support/passive options,
move between the relevant pages and apply an improvement without losing useful
planning context or mistaking an old result for the current build.

Exit scenarios (all required before closing M1):

1. Completed equipment/market analysis survives page navigation for the same
   build stage. Leaving the page stops active work without losing completed
   positions; returning can resume. Build, objective/category and stage changes
   cannot reuse stale results. Market-only edits update links without recalculation.
2. Support and passive planning retain useful same-context results across pages,
   including their constraints; applying a plan or changing its inputs invalidates
   incompatible results. Verify cancellation and late asynchronous responses.
3. A real-WASM journey goes equipment → supports → passives → replacement/apply,
   with clear baseline/preview distinctions, unsupported-effect limits and safe
   stage switching. Desktop, narrow layout and keyboard access are reviewed.
4. Independent review finds no unresolved correctness/safety issue; relevant
   contract/regression checks and actual product interaction pass. Any measured
   performance regression is resolved or explicitly accepted with evidence.

Do not expand M1 into a complete engine rewrite or revive hidden build-reference
navigation. Current calculation parity, data versions and source diagnostics stay
protected. Select the next milestone only after these exit scenarios pass.

## Latest verified iteration: I1

Starting checkpoint: `89364d456699a348e24dfae029ef75fb6e4671c0` (`v0.0.33`).
Initial worktree was clean; active data `4.5.5.3`; application version `0.0.33`.

Evidence used to prioritize:

- `docs/frontend-usability-review.md:49-56` explicitly records cross-page planning
  retention as unfinished; its other conclusions are historical, not fresh proof.
- Real-WASM Ego inspection: Ring 1 analysis showed market Sum `26.842`,
  DPS `7,552.35`, EHP `26,098.45`; visiting Calcs then Upgrades removed the
  reference entirely. This reproduces navigation loss before editing.
- Existing all-position resume and same-context market-change behavior are already
  implemented. Preserve those instead of building a second cache convention.

Selected slice: retain the Upgrades analysis workspace within one stage while
pausing work when hidden and preserving current invalidation rules. Support/tree
continuity remains a subsequent slice, not a claim about I1.

Orca coordination Run: `run_bd26e91173ae`.
Coordinator: `term_c99541f4-4ee4-441d-8651-530a976a2201`.
Scheduled automation: `0f3538b3-37df-44be-b888-d085f05d6e8d`, hourly,
existing workspace, OMP provider, reuse-session, single-owner/pause precheck.
Registered disabled during integration; enable only after a verified checkpoint.
Manual trigger smoke: run `81535afd-ad52-4063-b556-c714a616d83d` dispatched a
real OMP coordinator in this worktree. It read both documents, failed the atomic
claim and reported no edits/dispatches/commits. Manual runs bypass the scheduled
precheck; the claim is the final concurrency guard. Its terminal is retained by
the automation for reuse, not a second integration owner.

Research worker `ctx_9a5cc7b6bd0e` succeeded and was released. Development worker
`ctx_8a2c40b08e87` succeeded; release returned `retained / user_takeover`, so its
terminal was left untouched. No reclaimable supervised workers remain.
The coordinator integrated App, review fixes, documentation and validation.

### Product research and decisions

- [PoB2](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2) documents
  integrated item/support/passive impact, persistent stats and item paste.
  Documentation was inspected; the desktop UI was not exercised.
- [poe.ninja PoE2](https://poe.ninja/poe2/builds/forbiddenrites) exposes character,
  league/snapshot context and Copy/Trade actions. Its ladder coverage and incomplete
  configuration rules mean popularity is not a personalized upgrade recommendation.
- [PoE2DB supports](https://poe2db.tw/us/Support_Gems) and
  [rings](https://poe2db.tw/us/Rings) expose compatibility, requirements and tradeoffs.
  Live reference facts do not override PoBR's pinned data.
- [Official trade filter schema](https://www.pathofexile.com/api/trade2/data/filters)
  was inspected. The listing UI was blocked by HTTP 403/Cloudflare; no real listing,
  purchase or weighted-search interaction was verified.
- Decision: navigation is not a calculation dependency. Retain same-context
  analysis, cancel hidden work, invalidate effective calculation changes. Session
  retention does not imply reload persistence or background computation.
- Next candidates: consistent baseline/preview/diagnostic receipts, then a bounded
  cross-domain shortlist. These are proposals, not measured demand. Never add
  marginal gains together to claim a valid combined upgrade.
- I1 keeps the existing replacement-comparison reset-on-leave behavior by unmounting
  that child when hidden. Retaining replacement candidates belongs to the later
  workflow slice; hidden child calculations must not run unattended.

### Next-slice code research (not runtime proof or implementation)

Passive continuity scout found that simply hiding TreePanel is unsafe: tracked
passive work can auto-refresh while away, joint-jewel searches have separate
controllers, and retained tree key handlers/hover/heatmap need independent
deactivation. Do not copy I1 retention blindly to the canvas.

Recommended later seam: lift only stage-scoped planner settings, joint candidates
and valid completed snapshots; unmount the canvas and keep execution/controllers,
progress and previews transient. Preserve original request identity separately
from travel-attribute-prepared evaluation requests: joint Apply guards the original
request. Move mount-time invalidation to the retained owner and skip automatic
reruns of still-valid results. Include data/tree identity, stage transitions,
hidden keyboard safety, late batches and exact point/jewel Apply scenarios.

Support continuity scout recommends retaining exactly one selected support workspace
per stage, not all skill groups. Keep full evaluated request/settings/diagnostic
receipts; close ordinary editors/pickers while away. Pause scoring and selected-group
compatibility, observe invalidation while hidden, and consume focus nonces once.
Group indices are not stable IDs: deletion/compaction and skill-set replacement
must reset ownership. Existing shared exclusions remain preferences, not cached plans.
Verify unchanged round trips, hidden-input changes, late compatibility batches,
group retargeting and real Apply against the displayed preview. These remain I2
design proposals; no support/passive runtime result is claimed by I1.

## Ranked continuation

1. I2: retain one selected support workspace per stage, pause hidden scoring and
   compatibility batches, preserve request/settings/diagnostic identity and safe
   Apply. Reproduce the current navigation loss before implementing; use the
   scout findings above, not whole-page/per-group indefinite retention.
2. Lift passive/joint-jewel planning state without retaining hidden canvas effects.
   Keep exact original-versus-prepared request distinctions and transient previews.
3. Exercise the complete cross-page upgrade/apply journey; fix the most important
   observed trust/usability gap. Keep M1 active until all four exit scenarios pass.

Browser continuity: Ego TaskSpace `5`, Page `p1`, agent-owned; reused for I1
desktop and 390px inspection. Keep the same space for the ongoing M1 journey;
do not claim a user-owned space or overwrite existing browser builds. Named
development service `pobr-product-loop-dev` serves current source on port 5173;
check availability before reusing it. WASM and active data are already prepared.

## Checkpoint ledger

### I1 — retained upgrade analysis and safe recurring coordination

Checkpoint: the Conventional Commit containing this completed ledger entry;
exact resulting hash is recorded in `.cache/product-loop-checkpoint.json` and
must be copied into this tracked ledger when I2 starts. Base `89364d456699a348e24dfae029ef75fb6e4671c0`.

Product outcome: same-stage affix analysis, position and completed all-position
work survive navigation; hidden work aborts and resumes only unfinished positions.
Calculation inputs invalidate even while hidden; stages reset the workspace.
Unallocating a selected jewel socket now selects an available equipment position.
Whole-item comparison/equipment-candidate retention, support/passive retention
and reload-persistent results were not added.

Observed verification:

- Current production build/typecheck passed. Existing main-chunk warning remains
  at 505.72 kB; no measured startup/runtime speedup or redesign is claimed.
- `tradeAnalysis.test.ts`: 2 passed.
- Initial integrated `trade-analysis` + `upgrade-guidance`: 12 passed with real WASM.
  After the review fix, the 6 affected trade-analysis cases passed again; the
  unchanged 6 guidance cases reuse their prior passing result.
- Added removed-socket regression: failed before the fix because no position was
  selected on return; passed after the fix. An incorrect disabled-empty-hand
  assumption was deleted, not used to change legitimate empty-slot searching.
- Ego: Ring 1 reference text identical after Calcs → Upgrades; hidden wrapper
  display `none`, zero visible trade buttons. At 390px, retained Sum `26.842`
  remained visible and main horizontal overflow was zero. Desktop/mobile images:
  `.cache/product-loop-I1-{desktop,mobile}.png`. Browser builds were not edited.
- Independent Spec and Standards reviewers found removed-socket selection and
  manual-pause gaps; both confirmed their targeted fixes. No other actionable
  finding was reported within their source-review scope.
- Manual automation run verified occupied-owner rejection without edits.
  Updated pause-entry command also rejected an isolated paused/unowned scenario
  with exit 1 and did not create an active claim. This does not prove behavior
  while the machine sleeps, future provider availability or every runtime failure.

Version/tag decision: application stays `0.0.33`; **no new tag**. M1 remains open
and full release gates were not run. Evaluate the next existing-series version
(normally `v0.0.34`) only after integrated milestone proof and release validation;
do not push or deploy without separate authorization.

Operational continuation: activate the registered hourly trigger and immediately
trigger I2 only after this verified commit and I1 ownership release. The next
coordinator must record its own claim/Run and absorb any new human steering.
