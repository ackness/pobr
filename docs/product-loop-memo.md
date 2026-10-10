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
- 2026-10-11 I2 trigger: reaffirm tool-only scope, durable pause on every entry,
  atomic ownership, genuine parallel slices, real-WASM/browser proof and independent
  review. Main alone integrates/commits/tags; no push/publication/deployment.
  Do not close M1 from equipment/support-only evidence.

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

## Previous verified iteration: I1

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

1. I3: retain stage-scoped passive planner settings, completed route receipts and
   joint-jewel candidates/results while unmounting the canvas. Reproduce route and
   joint-result navigation loss first. Preserve the original request separately
   from travel-attribute-prepared evaluations; no hidden auto-refresh/search,
   keyboard/hover/heatmap effects or stale late batch may survive leaving Tree.
   Prove unchanged return, hidden input/goal invalidation, stage/tree/data reset,
   exact node/attribute/jewel Apply and cancellation before committing.
2. Exercise equipment → supports → passives → replacement/apply with real WASM.
   Review desktop, narrow layout and keyboard behavior; distinguish baseline,
   preview, diagnostics and stage ownership. Fix the highest-impact observed gap.
3. Close M1 only after all four exits have integrated runtime proof and independent
   review. Until then no next milestone, version bump or release tag.

Browser continuity: Ego TaskSpace `5`, Page `p1`, agent-owned; reused for I1/I2
desktop and 390px inspection. Keep the same space for the ongoing M1 journey;
do not claim a user-owned space or overwrite existing browser builds. Named
development service `pobr-product-loop-dev` serves current source on port 5173;
check availability before reusing it. WASM and active data are already prepared.

## Checkpoint ledger

### I1 — retained upgrade analysis and safe recurring coordination

Checkpoint: `3b0f7a629385f72463c357f0b85f8c936aa64ff5`
(`feat: retain upgrade analysis and start product loop`).
Base: `89364d456699a348e24dfae029ef75fb6e4671c0`. Hourly automation was
observed enabled at I2 entry; the worktree was clean.

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

### I2 — latest verified checkpoint: stage-scoped support planning continuity

Starting checkpoint: `3b0f7a629385f72463c357f0b85f8c936aa64ff5`.
Checkpoint: the Conventional Commit containing this completed I2 row;
its exact resulting hash is recorded in `.cache/product-loop-checkpoint.json`
and must be copied into this tracked ledger at I3 entry.
Application `0.0.33`, active data `4.5.5.3`, branch `multi-agent-self-improvement`.

Outcome and decisions:

- The selected support group retains completed recommendations, socket capacity,
  lineage, search/exclusions and the full evaluated request/settings/objective,
  catalog and diagnostic receipt across same-stage page navigation.
- Leaving Skills cancels scoring and selected-group compatibility; no automatic
  scoring restart on return. Hidden ordinary pickers/editors and optimizer controls
  unmount. Changed build/goal/settings invalidate even while hidden.
- Exact-request Apply equips the displayed complete support set. Retargeting
  discards the previous workspace; deletion/compaction, identical skill-set
  replacement, import and stage replacement reset ownership. No per-group cache,
  reload persistence, passive retention or formula/data change was added.
- Shared compatibility batching now checks cancellation before/after each backend
  batch; scoring and market callers forward their existing signals. An already
  running synchronous WASM call cannot be preempted; its response is rejected and
  subsequent batches do not start after cancellation.
- Independent review identified result loss when lazy Calcs/Tree reconnect layout
  effects under shared Suspense. Main moved retained Skills outside that boundary.
  Main also blocked prior-stage focus during remount. Retarget regressions were
  corrected to single-workspace scope rather than introducing per-group caching.

Observed verification:

- Before: Ego's imported Frost Bomb build completed 824 recalculations, then
  Calcs → Skills removed the optimizer/result. A current-source regression failed
  at the missing completed result. The compatibility-cancellation unit regression
  also failed before the fix: its delayed response resolved instead of rejecting.
- Final production build, including TypeScript checks, passed. Main chunk remains
  above the existing threshold at 507.13 kB; no startup/runtime speedup was measured.
- Scoped units: 14 support optimizer, 8 market planner and 41 build-session tests
  passed. Final isolated production preview: 12 support-continuity and 6 existing
  upgrade-guidance cases passed with real WASM. The 6 trade-analysis cases passed
  in the prior integrated run and reuse that result for unchanged inputs.
- Continuity cases exercise displayed-preview Apply, hidden request/shared-goal
  edits, identical stage/skill-set resets, group retarget/compaction, cancellation,
  late real-engine continuations and stale-stage/fresh-focus transitions.
  Synthetic WeGame fixtures are explicitly identified. Clock control pauses only
  the actual inter-batch yield, never replacing engine scoring.
- Ego: actual imported Frost Bomb build retained identical result text after
  Skills → Calcs → Upgrades → Skills, capacity 5 and lineage disabled; 955 complete
  recalculations, baseline DPS 7,552.3 / EHP 26,098.5, first preview DPS 25,312.9.
  Hidden Skills exposed zero visible controls. Desktop and 390px screenshots were
  inspected; horizontal overflow was zero, Tab reached the lineage checkbox and
  Apply had visible keyboard focus. Existing browser builds were not edited.
  Images: `.cache/product-loop-I2-{desktop,mobile}.png`.
- Independent Spec/Standards source reviewers confirmed both findings resolved,
  including the added identical-set and stale-stage focus regressions.
- An initial browser harness mixed innerText/textContent and overbroad Apply
  selectors (including saved skill sets). Those failures were not product proof;
  corrected assertions preserve the consumer-visible acceptance requirements.
- Rust/WASM sources and pinned data/parity baselines were unchanged; prepared real
  WASM was reused. No Rust/full-release/parity gate, real market purchase, touch
  device, screen-reader, reload persistence or broad performance proof is claimed.

Operational provenance:

- Run `run_538df68ce827`; coordinator
  `term_44e32632-f150-48de-ad9a-4712a77adaf7`; I2 acquired the atomic claim with no
  pause marker and rechecked pause afterward. Current steering was persisted before
  dispatch. Automation `0f3538b3-37df-44be-b888-d085f05d6e8d` remains hourly.
- Parallel supervised editors: implementation `ctx_79eb8b81687b` owned only the two
  skill modules; regressions `ctx_b8af9dc81b0b` owned only the new continuity spec.
  Main owned App/session/shared cancellation, integration fixes, docs and Git.
  Both succeeded and were released after accepted settlement; reclaimable fleet
  was empty. Read-only Spec and Standards review ran independently in parallel.
- Implementation worker attempted an extra browser page before main clarified
  browser ownership; it timed out without evidence. Main inspected TaskSpace 5:
  only agent-owned `p1` existed, so no extra tab remained to reclaim.
- Temporary isolated preview configuration is removed after verification; port
  5173's existing development service and agent-owned page remain for M1.

M1 stays open: equipment/support continuity have scoped proof, but passive/joint
retention and the complete cross-domain acceptance journey do not. No next
milestone is defined, no application version changes and no new tag. No push,
publication or deployment. End at this useful local checkpoint, release only I2's
own claim and leave the recurring trigger enabled for the ranked I3 slice above.
