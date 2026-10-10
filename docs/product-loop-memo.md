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
- 2026-10-11 I3 trigger: current user steering reaffirms tool-only scope, durable
  pause precedence on manual/reused entry, atomic single ownership, supervised
  parallel work, real-WASM/browser proof and independent review. Continue M1;
  main alone integrates/commits/tags. No push/publication/deployment.
- 2026-10-11 I4 trigger: reaffirm all existing tool-only, pause/atomic ownership,
  supervised parallel work, real-WASM/browser and independent-review constraints.
  Continue M1 with route diagnostic provenance and the cross-domain journey;
  main alone integrates/commits/tags. No push/publication/deployment; no partial
  M1 closure. Entry checkpoint `c35c037010c325b076b42bac060d798816ea0e7a`
  (`feat: retain stage-scoped passive planning`); clean worktree, pause absent,
  new claim rechecked after acquisition, automation observed enabled.
- 2026-10-11 I5 trigger: reaffirm current tool-only, durable pause/atomic ownership,
  genuine parallel work, real-WASM/browser, independent review and main-only Git
  constraints. Continue M1's ranked replacement workspace retention seam; support
  diagnostic provenance remains separate. No partial M1 closure, next milestone,
  version/tag, push, external publication or deployment. Entry checkpoint
  `0634e3fe4747d667f240e5f71e771e6356bfdb21`; clean existing branch, pause absent
  before and after new atomic claim, recurring automation observed enabled.

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

Starting checkpoint: `89364d456699a348e24dfae029ef75fb6e4671c0` (application `0.0.33`).
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

1. I6: preserve support diagnostic provenance at the existing frontend engine seam.
   `supportOptimizer.ts` reduces identity-row baseline strings to a Set/count and
   discards new candidate reasons/errors before returning its complete receipt.
   Follow the existing route `baselineUnsupported` / `excluded` convention:
   retain raw baseline lines and associate exact new reasons/errors with the
   complete evaluated support set/variant, including removal/replacement probes.
   Keep diagnostic exclusions separate from manual/compatibility exclusions and
   recommendations; no Apply on excluded edits, no scoring/count-policy change.
   Preserve current retained owner, exact-request Apply, family/lineage boundaries.
   No Rust/formula/data/DTO scope expansion. A real baseline Guard fixture plus
   safe exact Apply is proposed, not yet run. Reversed-index, combination-only,
   error/non-finite and removal policy cases can use the existing injected seam;
   do not claim those synthetic reasons are real-engine diagnostic proof.
2. Repeat the copied-stage equipment → supports → passives → replacement/apply
   journey after support provenance is integrated, including supported-effect
   caveats, desktop/narrow/keyboard, stage safety and independent review. I5
   replacement continuity is complete scoped proof, not the renewed full journey.
   Close M1 only when all four exits have runtime evidence; no next milestone,
   version bump or release tag yet. Never treat absent diagnostic strings or known
   compatibility metadata as proof that every support mechanic is modeled.

Browser continuity: Ego TaskSpace `5`, Page `p1`, agent-owned; reused for I1–I5
desktop, 390px and 320px inspection. Keep the same space for the ongoing M1 journey;
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

### I2 — stage-scoped support planning continuity

Starting checkpoint: `3b0f7a629385f72463c357f0b85f8c936aa64ff5`.
Checkpoint: `ff7181bb50458cfeeabec7e036eab35dcdd70fdc`
(`feat: retain stage-scoped support planning`), confirmed at I3 entry against
the existing branch history; the entry worktree was clean.
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

### I3 — stage-scoped passive planning continuity

Starting checkpoint: `ff7181bb50458cfeeabec7e036eab35dcdd70fdc`
(`feat: retain stage-scoped support planning`); entry worktree was clean.
Checkpoint: `c35c037010c325b076b42bac060d798816ea0e7a`
(`feat: retain stage-scoped passive planning`), confirmed at I4 entry against
existing history and `.cache/product-loop-checkpoint.json`.
Application `0.0.33`, active data `4.5.5.3`, branch `multi-agent-self-improvement`.

Outcome and decisions:

- Tree retains route expansion/mode/point budget/travel attributes and compatible
  completed route receipts, plus joint jewel raw/socket/candidates/results.
  Existing evaluated objective, baseline, bounded-search/unsupported counts and
  joint diagnostic strings remain paired with original request and tree/data
  identity. Route receipts additionally retain the travel-prepared request for
  exact attribute Apply; joint Apply guards the original request and uses the
  complete evaluated node/attribute/jewel plan, not an unused prepared copy.
- A stage-keyed, independently suspended Tree owner survives navigation while the
  entire canvas/edit surface unmounts. Leaving cancels both searches/debounce,
  pauses tracking and clears transient previews; no hidden controls, keyboard,
  hover/heatmap publication or automatic scoring restart survives. Hidden input/
  shared-goal changes invalidate incompatible results. Same-context return is
  display-only; explicit Run can restart. Intentional visible route auto-refresh,
  including Apply, remains; collapse/cancel pauses it.
- Tree/data identity changes reset the owner; delayed old loads are rejected.
  Copied identical stages reset settings/candidates/receipts and cannot replay old
  focus. Historical tree calculation/export remains supported without exposing
  current-tree editing. Route cards now show absolute baseline → preview values.
- Main added joint-library abort checks after asynchronous backend/catalog/base
  and candidate-validation prerequisites, before another engine call can begin.
  Running synchronous WASM cannot be preempted; retired responses are rejected.
  No engine formulas, pinned data, parity baselines or JSON contract changed.

Observed verification:

- Before: actual imported Frost Bomb route search evaluated 392 plans, displayed
  10 recommendations, then lost its receipt/expansion on Tree → Calcs → Tree.
  Joint search evaluated 1,046 plans with one Ruby placement and lost both on the
  same round trip. Both real-WASM navigation regressions failed against baseline
  production dist at the missing returned planner body before Apply assertions.
- Final production build including TypeScript passed; existing main-chunk warning
  remains at 507.39 kB. Restored route-key memoization avoids repeated complete
  request/topology serialization on progress renders; no runtime speedup measured.
- Scoped units: 17 passive planner, 14 joint planner, 11 passive graph and 41
  build-session tests passed. The new cancellation edge initially exposed an
  extra candidate evaluation after translation cancellation; the corrected
  boundary expectation distinguishes baseline/identity work already completed.
- Final isolated production preview: all 16 passive-continuity cases and 6
  existing upgrade-guidance cases passed across the final scoped runs. Coverage
  includes exact displayed node/attribute/jewel Apply, original/prepared request
  distinction, hidden equipment/goal invalidation, identical stages, stale focus,
  historical trees, hidden keyboard safety, cancellation/late continuation, and
  retired generation A settling while real generation B remains pending.
- Existing 8 passive-jewel and 12 support-continuity cases passed in the prior
  integrated run; those unchanged behavior checks reuse that result after the
  receipt-storage-only review refinements. No additional full release gate run.
- Synthetic Witch/item/league fixtures are explicitly identified, including the
  deliberately synthetic +1,000 Life candidate; scoring/topology/Apply stay real
  WASM. Scheduling controls hold only actual inter-batch continuations after
  positive incomplete progress, never substituting engine responses.
- Ego TaskSpace 5/p1: imported Frost Bomb route and joint receipt text identical
  after Calcs/Skills/Upgrades round trips; hidden canvas/run control counts zero,
  no resumed progress. Baseline DPS 7,552.35 / EHP 26,098.45; first route preview
  DPS 9,085.53. Existing saved build was not edited. Desktop and 390px screenshots
  inspected; document/main horizontal overflow zero; Tab reached Apply with
  visible keyboard focus. Images: `.cache/product-loop-I3-{desktop,mobile}.png`.
- Initial new harness used an invalid display slot instead of stable `ring1`,
  mixed innerText/textContent expectations, and selected a remote attribute whose
  click legitimately allocates a path instead of opening the one-point picker.
  Corrected fixture/observations preserve acceptance; those failures were not
  treated as product regressions.

Independent review and M1 evaluation:

- Spec and Standards source reviews ran independently in parallel. No new
  correctness/lifecycle safety defect or documented standards breach identified.
  Main preserved visible Apply tracking, restored key memoization and removed
  unused joint prepared-request storage. The Spec overlap-coverage finding is
  resolved by two passing real-engine overlapping-generation scenarios.
- Spec identified an inherited route diagnostic-provenance gap: the existing
  route library retains an unsupported count but discards baseline unsupported
  strings and individual excluded-candidate reasons. I3 preserves existing
  receipts, not a claim that this gap is solved. Explicitly prioritize it with the
  complete cross-domain journey for I4; M1 exit 3 remains unfulfilled.
- Equipment/support/passive continuity now have scoped proof. The complete
  equipment → supports → passives → replacement/apply journey, including paired
  unsupported-effect limits and safe stage context, is not yet integrated proof.
  M1 stays open; no M2, version bump, tag, push, publication or deployment.
- Limits: prepared WASM reused; no Rust/full-release/parity gate, real market
  purchase, real touch device, screen-reader, reload persistence or broad
  performance claim. Server manifest changes do not replace initialized data;
  no artificial mutable-backend data-version switch was tested.

Operational provenance:

- Run `run_ffc72017bb93`; coordinator
  `term_21034ed0-4c1e-4105-bd08-71f298a8b81c`; new atomic claim, pause checked at
  entry/post-acquisition and before settlement. Current steering persisted before
  dispatch. Automation `0f3538b3-37df-44be-b888-d085f05d6e8d` observed enabled.
- Disjoint supervised development `ctx_f72cd69b81a8` and regression
  `ctx_814e8b4e50c5` ran alongside read-only lifecycle research `ctx_33588f97e202`.
  Main alone owned App/library/integration fixes, documentation, runtime proof
  and Git. Independent Spec `ctx_b0241ab9a941` and Standards `ctx_26175703be85`
  reviewers were read-only. Every worker succeeded within its assigned scope
  and was released after accepted settlement; reclaimable Run fleet was empty.
- Temporary isolated preview/configuration removed after proof. Existing port
  5173 development service and agent-owned browser page remain for M1. After this
  verified commit, release only I3's own claim; retain the enabled recurring
  trigger for the precise ranked I4 slice above.

### I4 — passive route diagnostic provenance

Starting checkpoint: `c35c037010c325b076b42bac060d798816ea0e7a`
(`feat: retain stage-scoped passive planning`); entry worktree was clean.
Checkpoint: `0634e3fe4747d667f240e5f71e771e6356bfdb21`
(`feat: preserve passive route diagnostic provenance`), confirmed at I5 entry
against branch history and `.cache/product-loop-checkpoint.json`.
Application `0.0.33`, active data `4.5.5.3`, existing branch unchanged.

Outcome and decisions:

- Route receipts preserve baseline unsupported strings verbatim, and associate
  new unsupported reasons/engine errors with complete probe and combination
  allocations/refunds. Existing search, ranking and unmodeled-count semantics
  remain unchanged. Excluded routes are separate diagnostic disclosures without
  preview/Apply controls; Apply additionally requires a recommended receipt plan.
- The existing original/travel-prepared request distinction, captured objective,
  stage invalidation, cancellation and hidden-response rejection remain. The
  receipt displays captured active data and requested tree identity; absent tree
  annotation is labeled Default, not Unavailable or a guessed numeric version.
- Localized native disclosures explain baseline limits, new excluded effects and
  preview/current distinction. Main resolved independent review findings by
  separating diagnostic/recommendation classes, limiting card styles to direct
  children and wrapping raw diagnostic strings without rewriting them.
- Actual 390px inspection found Stage collapsed to ~38px while the build remained
  editable. Main adjusted only the narrow toolbar flex sizing, then inspected
  readable stage/build context at 390px and 320px with zero horizontal overflow.
- No Rust/WASM source, formulas, JSON handshake, pinned data or parity baseline
  changed. Existing prepared real WASM/data reused. Ordinary tree text can still
  be filtered before diagnostics; this change exposes existing engine reports,
  not complete diagnostic-ingestion coverage or complete numerical parity.

Observed verification:

- Failing-before: both new real-WASM diagnostic scenarios completed an actual
  route search against preserved I3 production dist, then failed because the
  baseline diagnostic receipt was absent. Pinned Vorana/Very Large timeless
  fixture gives `Tree:46819: missing timeless jewel seed data` in baseline and
  complete excluded route `[49512,5936]` with each node's new seed-data reason.
- Passing units: 19 passive planner, 14 joint planner and 11 graph cases. Probe,
  reversed batch-local combination indexes and allocation/refund exclusions
  preserve exact engine reasons/errors and reject unsafe recommendations. An
  added incidental top-ranked tie expectation failed; main deleted it rather than
  changing legitimate ranking or repinning incidental order.
- Integrated production build/typecheck passed. Initial integrated real-WASM
  run passed both diagnostic, all 16 passive-continuity and all 6 guidance cases.
  Main-chunk warning remains 507.39 kB; no runtime speedup/performance claim.
- Final production build/typecheck and isolated preview passed 12 real-WASM
  diagnostic/guidance/layout cases after all class, wrapping, default-tree and
  narrow-stage fixes. The 16 lifecycle-continuity cases reuse their passing
  integrated result: lifecycle/library inputs were unchanged by those refinements.
- Ego imported Frost Bomb copied stage `M1 I4 verified planning`: equipment
  reference Sum 26.842, baseline DPS 7,552.35/EHP 26,098.45; 824 complete support
  recalculations, first support preview DPS 29,387.7; 392 route evaluations,
  first route preview DPS 9,085.53. Preview and same-stage page navigation did
  not change the build; equipment reference survived support/passive navigation.
- Complete synthetic NORMAL Sapphire Ring replacement displayed DPS 7,125.3
  against current 7,552.35, warned of a downgrade and 10 unresolved effects.
  Keyboard-focused Apply had visible focus; Enter applied only `ring1`, actual
  sidebar DPS matched the preview, and old support/route/equipment receipts
  disappeared. Stored non-ring state and original stage were compared exactly:
  unchanged. Switching original/copy restored DPS 7,552/7,125 respectively.
- Fresh post-replacement route baseline was DPS 7,125.3 and first preview 8,570.1.
  Its 10 actual engine baseline limitations were disclosed by keyboard Enter
  with visible focus at 390px. At 390/320/1440px, document/main horizontal
  overflow was zero; stage selector widths were 249/179/238px. Screenshots:
  `.cache/product-loop-I4-diagnostics-{mobile,desktop}.png`.
- Actual replacement draft continuity gap reproduced separately on the copied
  stage: synthetic complete `M1 Preview Ring` and one evaluated result became
  empty text/zero results after Upgrades → Skills → Upgrades. No Apply occurred
  in that observation. This is I5 work, not a successful continuity claim.
- Browser harness limits: Ego automatic scrolling over the tree canvas failed
  to reach offscreen planner controls; real wheel input over the outer margin
  reached them. A CSS subtree snapshot root and a relative Ego filesystem path
  were invalid; corrected observation/artifact calls did not change product
  behavior. No touch-device, screen-reader, actual trade purchase, reload
  persistence, broad performance or Rust/full-release/parity proof is claimed.

Independent review and milestone evaluation:

- Independent Spec found default-tree annotation displayed as Unavailable;
  Standards found diagnostic cards sharing recommendation selectors/styles and
  unwrapped baseline text. All three were corrected and targeted source
  followups confirmed resolution. Narrow-stage sizing was independently
  reviewed without a new correctness concern. Runtime proof remains main-owned.
- M1 remains open: replacement workspace loss and support count-only provenance
  prevent claiming the full trustworthy cross-domain outcome. The next slice is
  the ranked I5 retention seam above, followed by support diagnostic provenance
  and renewed complete journey proof. No M2, version bump or release tag.

Operational provenance:

- Run `run_4000ff95fd7b`, coordinator
  `term_dd87e2a4-2cb7-4e4c-aa88-c3862a8ec790`. Entry/post-acquisition pause
  checks passed; atomic new claim and current steering persisted before dispatch.
  Automation `0f3538b3-37df-44be-b888-d085f05d6e8d` observed enabled.
- Disjoint implementation `ctx_28856930367d`, regressions `ctx_03e420c5e15c`
  and read-only journey research `ctx_816240e10bae` ran in a parallel supervised
  wave. Main alone owned integration fixes, browser/services, docs and Git.
  Independent Spec `ctx_e579f5c3c000` and Standards `ctx_33d32fffd311` ran in
  parallel; accepted reviewer terminals were reused only for targeted followups
  `ctx_20588ffff104` and `ctx_0b181d8b265c`, then released after settlement.
  Reclaimable Run fleet was empty. No worker Git mutation or browser ownership.
- Retain agent-owned Ego 5/p1 and existing port 5173 for M1. Temporary I4
  production preview/configuration are removed after final verification.
  Release only this coordinator's claim after its verified commit; leave the
  recurring trigger enabled. No push/publication/deployment.

### I5 — latest verified checkpoint: stage-scoped replacement continuity

Starting checkpoint: `0634e3fe4747d667f240e5f71e771e6356bfdb21`
(`feat: preserve passive route diagnostic provenance`); entry worktree was clean.
Checkpoint: the Conventional Commit containing this completed I5 ledger row;
exact resulting hash is written to `.cache/product-loop-checkpoint.json` after
commit and must be copied here at I6 entry.
Application `0.0.33`, active data `4.5.5.3`, existing branch unchanged.
At I5 settlement, `v0.0.33^{commit}` resolves to
`d11a69317c452cde614b62fedd806352a91014ff`; I1's application-0.0.33 starting
checkpoint is not that tag's target. No tag was created or moved.

Outcome and decisions:

- The stage-keyed retained Upgrades owner now retains complete replacement text,
  chosen position, exact evaluated report/request/objective/catalog/data/tree
  receipt and per-position augment choices. Equipment candidates, maximum
  replacement count and compatible completed equipment receipts survive same-stage
  page navigation. Unapplied local affix drafts also survive and still block Apply.
- Leaving synchronously aborts both controllers, clears pending paste/debounce,
  busy/progress and disables child pickers; late generations cannot publish or
  clear a newer run. Returning displays completed receipts without scoring.
  Existing picker disable handling closes menus and releases document listeners.
- Changed materialized requests, weapon set, objective, catalog/data/tree/socket
  identity invalidate incompatible evaluated receipts even while hidden. Text and
  candidates remain current-stage preferences; identical copied-stage transitions
  remount and reset all workspace state. Objective changes invalidate equipment
  receipts at every replacement cap; this is a conservative receipt policy, not
  a formula/search/ranking change. Users explicitly recalculate under the new goal.
- Apply/add-candidate recheck the current receipt and request synchronously. Single
  Apply uses the exact selected prepared item/augment payload; joint Apply uses the
  complete evaluated plan. Raw copied text, unrelated sources, existing unsupported
  diagnostics and feasibility gates remain intact. Render keys are memoized rather
  than serializing the full build on every progress update.
- Main added abort checks after catalog/backend acquisition and before equipment
  evaluation entry. The existing evaluator already checks cancellation before its
  engine batch; the final-offhand fix additionally avoids entering its prerequisites.
  A synchronous WASM call already running still cannot be preempted.
- No Rust/WASM source, formulas, pinned data, parity baselines or JSON contract
  changed. No per-build cache, reload persistence or hidden background scoring.

Observed verification:

- Before: Ego populated a complete synthetic Sapphire Ring and real replacement
  receipt, then observed empty text/zero results after Skills → Upgrades. All
  three new navigation/raw-Apply, prepared-augment-Apply and joint-plan regressions
  failed against preserved I4 production dist at lost text or cap reset 3 → 2.
- Four prerequisite-cancellation regressions failed before the library fixes:
  translation ran after cancelled single/position backend acquisition, rune
  catalog ran after cancelled equipment acquisition, and offhand cancellation
  still entered evaluation. Final tests assert AbortError and no subsequent work.
- Production build including TypeScript passed. Scoped units: 24 replacement,
  17 equipment, 15 augment and 20 affix cases passed. After strengthening only
  cancellation error assertions, the 41 affected replacement/equipment cases
  passed again; unchanged augment/affix cases reuse their result.
- Integrated isolated production preview: all 30 real-WASM cases passed
  (13 new replacement-continuity, 10 existing item-replacement, one affix-editor,
  six upgrade-guidance). They prove exact raw/prepared/joint Apply, navigation,
  hidden build/goal invalidation at caps 1/2/3, identical-stage resets, debounce
  cancellation and delayed/overlapping real-engine generations. Import/items and
  league fixtures are synthetic; scoring, parsing, augment preparation and Apply
  use shipped WASM/data. Scheduling holds only actual post-batch continuations.
- Ego 5/p1: copied original stage to `M1 I5 replacement continuity`; original and
  existing I4 stages were compared exactly and remained unchanged. Full replacement
  text, selected Ring 2, one prepared candidate, cap 3 and both completed receipts
  were identical after Skills/Calcs/Tree round trips; hidden replacement control
  counts were zero and preview/navigation did not change the build.
- Actual Frost Bomb baseline DPS 7,552.35 / EHP 26,098.45; the explicitly synthetic
  ring preview warned of a downgrade and ten engine limits, with DPS 7,125.3 /
  EHP 24,947.48. Keyboard Enter applied only exact `ring2`; all other state was
  compared exactly, and visible actual DPS 7,125 / EHP 24,947 matched rounding.
  Original/copy stage switching restored DPS 7,552/7,125 and reset the workspace.
- Desktop/390px screenshots inspected; Enter disclosed ten raw limitations and
  Tab reached Apply with visible focus. At 320/390/1440px document/main horizontal
  overflow was zero; narrow stage widths were ~181/251px. Images:
  `.cache/product-loop-I5-{desktop,mobile}.png`.
- Separate actual affix-draft smoke removed a Life affix locally, retained the
  unapplied draft through Skills navigation and kept Apply disabled. Build bytes
  stayed exact; Discard restored the affix and enabled the still-valid Apply.
- Existing 507.39 kB main-chunk warning remains; no runtime speedup or broad
  performance claim. Catalog/data/tree invalidation was independently source
  reviewed, not exercised through a mutable initialized backend. No Rust/full
  release/parity gate, actual market purchase, touch device, screen-reader or
  reload-persistence proof. One overly exact Ego text selector failed; the observed
  Ring 2 target was then selected with the documented scoped selector, not a fix.

Independent review and M1 evaluation:

- Independent supervised Spec and Standards reviews reported no actionable
  correctness, lifecycle-safety or documented-standard finding within source scope.
  Runtime verification remained main-owned; neither review claims complete M1 proof.
- Supplemental read-only research located support baseline/rejection loss and
  established the minimum route-equivalent frontend receipt shape. It also found
  normal support stat-map ingestion silently skips some unsupported/unknown stats;
  preserving emitted strings cannot certify full modeled support coverage. No
  shipped support-only new-reason/error fixture was established. I6 must distinguish
  real baseline provenance/Apply proof from synthetic exclusion-policy coverage,
  disclose existing engine limits and avoid expanding into an engine rewrite.
- M1 remains open for support diagnostic provenance and renewed integrated
  cross-domain journey proof. No M2, version bump, release tag or publication.

Operational provenance:

- Run `run_cdfbf0307100`, coordinator
  `term_6f1d743e-583f-40af-b5f1-1c79efaedaee`. Entry/post-acquisition and settlement
  pause checks passed; atomic new claim and current steering persisted before
  dispatch. Automation `0f3538b3-37df-44be-b888-d085f05d6e8d` observed enabled.
- Disjoint supervised implementation `ctx_10afe0579b40` and regressions
  `ctx_db56bf15170b` ran in parallel; Spec `ctx_0821cb56ec99` and Standards
  `ctx_d6a5ef8d8d43` reviewed independently in parallel. Main alone owned parent/
  library integration, browser/services, docs and Git. All four succeeded within
  assigned scope and were released after accepted settlement; reclaimable fleet
  was empty. Supplemental support-seam scout and cancellation reviewer were
  read-only tool subagents, not Orca editors; their source research is not runtime
  evidence. No worker Git mutation or browser ownership.
- Retain agent-owned Ego 5/p1 and existing port 5173 for the continuing M1 journey.
  Temporary I5 production previews are stopped and isolated config is removed.
  After verified commit, release only this coordinator's owner.json and claim;
  leave the recurring trigger enabled. No push/publication/deployment.
