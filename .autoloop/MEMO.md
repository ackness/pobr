# PoBR autonomous improvement loop

## Scope and standing decisions

- Loop: `loop-20261011-1650`; branch: `autoloop/loop-20261011-1650`.
- Work only in `/Users/wuyong/codes/game/pobr/.omp/worktrees/autoloop-loop-20261011-1650`.
- Focus: auto. The initial focus question was cancelled; the restarted instruction explicitly leaves selection to the assistant.
- Commit only. The driver handles publication; do not push, open PRs or create release tags.
- Leave the paused `/goal` unchanged as instructed.
- No inbox steering at initialization.
- Prior-loop observations are historical leads, not verification of this branch. Do not duplicate its pending changes without checking current evidence.

## Product and survey

PoBR is a browser-based PoE2 planner using a Rust/WASM engine, versioned game data and three UI languages. Its primary promise is comparing equipment, support sets and connected passive upgrades before changing a build. Calculation coverage is explicitly incomplete.

Current starting commit: `6bd4cee` (`fix: identify complete attribution skill groups`). Recent history emphasizes import recovery, deduplicated item libraries and preservation of imported weapon-hand candidates. `gh issue list --state open --limit 15` completed successfully with no open issues returned.

Survey sources: README product promise; CLAUDE architecture/validation table; `docs/web-contracts.md` support/passive/workspace contracts; `web/src/lib/passivePlanner.ts`; `web/e2e/passive-continuity.spec.ts`; current git history. The existing planner combines connected routes, bounded refunds, atomic travel attributes, unsupported-effect receipts and explicit application. Existing browser coverage includes synthetic three-point routes and jewel continuity; realistic populated-build behavior still needs direct observation in this loop.

## Long-term goal

Make PoBR a trustworthy decision workspace for real PoE2 builds: comparisons must show their assumptions and limits, preserve player work, and produce the same result when applied as when previewed.

## Completed milestone: Trustworthy passive reallocation decisions

Prioritize passive planning because it combines topology, scarce point budgets, travel attributes, refunds, calculation limits and persisted state in one player decision. This is a complementary scope to recent import work and prior-loop support-mechanic investigation.

Exit criteria:

1. Observe real shipped-WASM multi-point route searches on a populated imported build, including total/new/refunded point counts and no mutation before Apply.
2. Exercise a valid bounded refund/reallocation and confirm connectivity and protected allocations are retained; if the imported topology correctly disables refunds, verify the explanation and use a connected fixture for the positive path.
3. Applied nodes and travel attributes agree with the chosen preview, with unrelated equipment, gems and stages preserved through navigation/reload.
4. Baseline limitations and excluded edits remain visible; stale results cannot apply after relevant edits.
5. Fix only evidenced defects, with targeted regression checks and a direct runtime proof. Do not infer complete passive mechanic coverage from ranking or numerical parity.

## Iteration 1 plan

Establish this branch's runnable browser surface and inspect the passive planning workflow against the contract. Record concrete evidence and implement one bounded defect if observed. Do not assume previous-worktree WASM or running servers represent this branch.

## Candidate next priorities

- Continue realistic route/refund and preview/application verification until milestone exit criteria are met.
- Evaluate import/workspace discovery only when an observed workflow problem warrants it; existing history already contains substantial correctness work.
- Revisit broader calculation limits separately, without treating undocumented raw stats as uniformly unsupported.

## Iteration 1 evidence and handoff

- Branch-local frozen-lockfile dependencies installed; release WASM built from this branch; `sync-data` copied 67 runtime files and 17 dated references. No vendor clone was needed.
- Vite service `pobr-passives-1650` serves this worktree at `http://127.0.0.1:5182/`. Do not use the prior loop's port 5181 as evidence for this branch.
- Real browser imported `examples/demo-bd-test/builds/monk-invoker-frost-bomb/code.txt`: level 98 Invoker, 131 allocated nodes, 13 skill groups, 10 equipment slots.
- Three-point search with the existing Balanced DPS / EHP objective evaluated 391 plans and displayed 10 recommendations, 10 baseline calculation limits, and an explicit bounded-search notice. No excluded edits appeared in this particular search; it does not verify the exclusion path.
- Best route: Cooldown Recovery Rate, Temporal Mastery, Elemental Damage; highlighted node IDs `17796`, `12412`, `45497`. Both searching and Show on tree left the entire saved envelope unchanged.
- Apply changed only `allocatedNodes`, from 131 to 134. Every other state field, including all groups, equipment and attribute choices, remained unchanged; the other saved build was also unchanged. This route contained no new travel attributes, so atomic attribute changes remain unverified here.
- Direct calculation through the same branch's real backend returned TotalDPS `7560.833856375`, TotalEHP `23661.486537325`, Life `1`, matching the displayed preview `7560.83`, `23661.49`, `1`. Sidebar displayed Hit DPS `7,561`.
- A reload assertion based on JSON string equality failed because restored object keys use a different order. A fresh import/search/Apply/reload run compared every state field with Node's `isDeepStrictEqual`: semantic equality true, zero changed fields. No product fix is warranted for serialization ordering.
- Reallocate mode correctly disabled search for this imported fixture. Weapon-exclusive node counts are 23 and 22; the visible explanation names weapon-set passives and other unsafe imported topology. Positive refunds are not verified by a disabled control.
- Visually inspected route preview and disabled-refund screenshots; temporary screenshots removed. Browser task space 29 finished. No permanent tests or product code changed during this investigation.
- Browser automation caveat: automatic click scrolling hung on the nested planner drawer; after inspecting the unchanged state, `scrollIntoView({block: 'center'})` followed by a real click completed the search in seconds. Tool issue reported; do not attribute that timeout to calculation performance.

## Iteration 2 evidence and handoff

- Reused unchanged branch-local WASM/data and Vite service. No rebuild or test-suite rerun was needed for this investigation.
- Built a synthetic connected Witch fixture through the actual backend's `encodeBuild`, then imported its code via the normal UI (no storage replacement, mocked engine, or injected scores). Character level 90; Fireball level 16; Sapphire Ring with +80 maximum Life; occupied Ruby socket with synthetic +1000 maximum Life. These values test protection, not item legality or a build recommendation.
- Witch root `54447`; protected socket `61419`; connected socket path `[44871,56216,9485,60685,1826,39037,57710,8616,46819,61419]`. Added crit branch `[15618,32404,5501]` as refundable allocation. All eight existing travel attributes started as Intelligence.
- Selected Reallocate existing points, maximum refunds 3, Max Life objective, Strength for new travel attributes. Real search evaluated 228 plans; search and Show on tree left the entire persisted envelope unchanged.
- Top preview refunded `[5501,15618,32404]` and allocated `[8569,11672,47177]`. Apply changed only `allocatedNodes` and `attributeChoices`; all three new attributes became Strength and all retained choices stayed unchanged. Allocation remained 13 nodes; all 13 were connected to the root under the actual graph, and the complete occupied-socket path remained allocated.
- Real recalculation returned DPS `75.345833303`, Life `2352`, EHP `1731.785923019`, matching preview `75.35`, `2352`, `1731.79`. Screenshot inspection showed distinct added/refunded node markers and the zero-net-point tradeoff.
- Copied the applied build into `Refund verification copy`. The copied stage opened with the planner collapsed and no old plan actions. Current state was unchanged.
- Copying intentionally resets stage-local travel preferences: Intelligence was selected from the majority of allocated choices. Its Max Life search completed 512 evaluations with no recommendations; the smoke initially waited for a recommendation rather than completion and timed out. Selecting Strength produced three-point plans. This was not an engine hang or a product defect.
- Changing point budget from 3 to 1 refreshed all ten displayed plans to one-point edits without mutating the build. Applying one plan changed only the copied stage (13 to 14 nodes). Original stage snapshot was deeply equal to its pre-copy value. Reload preserved the copied state; switching back to Current restored the exact 13-node refunded state, including gear, skills and attributes.
- Browser space 30 finished; temporary screenshot removed. Port 5182 remains available. Saved synthetic Witch build has Current (13 nodes) and Refund verification copy (14 nodes), alongside the earlier imported builds.
- No production defect was found in these exercised paths. No permanent tests or production code changed; do not infer complete passive-mechanic coverage from these calculations.

## Iteration 3 evidence and milestone closure

- Reused the existing real-WASM fixture pattern from `web/e2e/passive-diagnostics.spec.ts`, but created its build through the real encoder and normal UI import rather than replacing browser storage. Witch level 90, Fireball level 16, the ten-node socket path from iteration 2, and Heroic Tragedy with Vorana seed 1234 at socket 61419.
- Two-point Max Life search evaluated 151 plans. Baseline limits included `Tree:46819: missing timeless jewel seed data` and the raw Remembrancing line. There were 26 excluded complete edits; searching left the saved envelope unchanged.
- Expanded the exact route `Attribute #49512 · Elemental Damage #5936`. It contained precisely the two new seed-data reasons for those IDs, not the baseline node's reason, and no buttons. Visually inspected the receipt. Navigation to Calcs and back preserved all baseline/excluded associations and build state.
- Selected Strength travel attributes to produce ten actionable plans before testing invalidation. After navigation paused tracking, replaced the timeless jewel with a Ruby granting +20 maximum Life using the socket editor and Save & recalculate. Only `jewels` changed; every old receipt and plan action disappeared, with no automatic search.
- Explicit fresh search evaluated 101 plans and returned ten recommendations against the new baseline. No baseline/exclusion disclosure or stale seed-data text remained. This proves invalidation by an actual calculation input change, not only by planner controls.
- Closed the bounded milestone: populated multi-point routes, conservative import refund guard, connected positive refunds, atomic travel attributes, protected sockets, exact preview/application agreement, stage isolation/reload, baseline/new diagnostic separation, and stale-result invalidation were exercised across iterations 1–3. This is not a claim of complete passive mechanics, exhaustive graph search, all languages or all device sizes.
- Browser space 31 finished and temporary screenshot removed. No production defect was found in the checked correctness paths; no production files or permanent tests were changed. Do not extend verification indefinitely.

## Completed milestone: Clear passive-plan application outcomes

Observed gap: after applying two nodes `[42736,4203]` in iteration 3, automatic search replaced the selected result with a new 115-plan baseline. The planner had no status message or applied confirmation. Iterations 1–2 similarly required inspecting stored state and sidebar numbers to establish what Apply did.

Exit criteria:

1. Successful ordinary passive-route application gives a concise, accessible confirmation identifying the applied allocation/refund, without presenting an old preview as a current recommendation.
2. Joint jewel/passive application has equivalent explicit feedback for its relevant edits.
3. Automatic recommendation refresh does not erase the acknowledgment; unrelated build changes and stage changes cannot leave a misleading success message. Failed/stale/no-op application does not claim success.
4. Use existing English, Simplified Chinese and Traditional Chinese copy/style conventions. No persisted history, undo subsystem, calculation changes or new diagnostics are required.
5. Exercise both changed UI paths with real WASM, navigation and stale-state boundaries, plus targeted checks and independent review for non-trivial implementation.

## Iteration 4 implementation and evidence

- Ordinary `TreeOptimizer` now retains `{version, plan}` after validated atomic Apply. `useBuildSession.setAllocatedNodes` calls `apply` once, synchronously advancing the documented edit revision. The acknowledgment is exposed only for that revision while calculation is neither busy nor failed. It is independent of recommendation search state; future edits cannot revive it by restoring identical inputs.
- Added existing-style `role="status"` feedback listing allocation/refund counts, node labels and stable IDs, with English, Traditional Chinese and Simplified Chinese headings. No engine/schema/CSS framework changes or persisted history. `docs/web-contracts.md` records this ordinary-route contract.
- Added a real-WASM lifetime regression in `web/e2e/passive-continuity.spec.ts`: actual added IDs appear; receipt survives fresh search and same-stage navigation; a jewel edit clears it; restoring identical inputs does not revive it; copying a stage resets it.
- Verification passed: 19 `passivePlanner.test.ts` cases, production web build including TypeScript checking, and the one new Playwright test. The first Playwright run exposed a test-only `innerText` versus `textContent` line-break mismatch; corrected comparisons with `useInnerText` and reran only that target successfully. Build emitted the existing >500 kB chunk advisory; no bundle-size change was attempted.
- Live browser added `[30555,53960,8975]` and displayed those exact IDs after the automatic 180-plan refresh. Switched through all three languages; inspected desktop and 390px screenshots. Node names retain existing catalog fallback behavior (for example Attribute remains English).
- Live connected refund smoke displayed three new Attribute IDs and refunds `5501/15618/32404`, with their names and counts. A throwaway scheduling gate held a completed real backend calculation before delivery: busy state had no success receipt; release displayed it. A separate injected rejection after real calculation showed an error and no receipt. Reload removed the injection and recovered normal calculation; these are lifecycle fault-injection checks, not naturally occurring engine failures.
- `ReviewPassiveReceipt` approved without blocking findings; its low-risk coverage suggestions were addressed by actual-ID assertions and the live boundary checks. Reviewer is settled done. Actual screen-reader speech was not tested; only semantic live-status markup and visible behavior are verified.
- Finished browser space 32. Removed temporary screenshots and isolated Playwright config. Vite remains on 5182. Unchanged Rust/WASM/data reused.

## Iteration 5 implementation and milestone closure

- Joint `JewelPassivePlanner` now records its applied revision, plan and candidate snapshot only after `session.applyTreePlan` returns true. Uses the ordinary planner's busy/error/revision lifetime; no new abstraction, mutation API or persisted state.
- Localized status identifies the applied jewel label/socket or kept-current-jewels choice, and allocation/refund counts, names and IDs. Capturing from the completed search prevents later candidate-list changes from rewriting the acknowledgment.
- Parameterized the existing real-WASM lifetime regression for both planners; added joint → ordinary → joint mutual invalidation coverage. Tests validate actual applied nodes/attributes/jewels and numeric previews through the existing exact-Apply helper, not synthetic callback echoes.
- Passed 14 joint-planner tests, 83 session tests, production web build/typecheck and all three scoped Playwright application-confirmation cases. Existing chunk-size advisory remains. No Rust/WASM/data changes; no full parity claim.
- Live joint Apply replaced socket 61419 with a synthetic Ruby granting +1200 maximum Life and added nodes `[29148,34840,1433]`; status matched the actual persisted changes. Removed the candidate afterward: the captured placement stayed correct. Fresh search retained the acknowledgment.
- Verified EN/TW/CN copy and inspected desktop/390px screenshots. Adding the identical equipped jewel produced no zero-node/no-refund plan among the 20 recommendations. Applying a kept-jewel route reported Keep current jewels and added `[3025,38732,17672]`, without claiming a replacement.
- Held a completed real calculation before delivery: no success while busy, receipt after release. Injected rejection after real calculation: error displayed, receipt absent. Reload removed the injection and recovered normal calculation without a persisted acknowledgment. These are lifecycle fault-injection checks.
- `ReviewJointReceipt` approved with no findings and is settled done. Actual screen-reader speech remains unverified. Finished browser space 33 and removed temporary screenshots/Playwright config; Vite remains on 5182.
- Feedback milestone complete across ordinary and joint application, recommendation/candidate edits, same-stage navigation, subsequent edits/restoration, copied stages, pending/failure states and localization.

## Current milestone: Reconcile canonical support-effect fidelity

Next priority is calculation trust, not more passive UI verification. A targeted static check found no `NeverCrit` or `global_cannot_crit` in this branch's `calc/crit.rs`, `rules/stat_map_engine.rs` or `extract_skill_overrides.lua`. This is a concrete lead, not yet a current-branch runtime reproduction. Prior-loop corrections are available in this repository:

- `d7863a1` — enforce canonical never-crit effects.
- `94392bc` — disclose unmodeled support resource loss.
- `650b70c` — disclose unmodeled support life sacrifice.

Exit criteria:

1. Check current ancestry/content and integration status before reusing prior work; do not independently reimplement an existing fix or overwrite newer code.
2. Reproduce affected support behavior with this branch's real engine. Integrate only still-relevant corrections, preserving current attribution/import/planner work.
3. Verify canonical crit precedence and host scope, accurate source-attributed resource limitations, and actual recommendation consequences with required targeted/parity checks and rebuilt WASM.
4. Keep unmodeled resource loss distinct from ordinary costs; do not invent maximum/current-pool semantics or claim complete mechanic coverage.

Next iteration: begin with the never-crit lead and prior commit reconciliation, then choose the smallest verified integration. Work and commits remain exclusively in this worktree; no publishing or changes to other branches.
