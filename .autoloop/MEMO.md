# Product improvement loop

## Standing scope
- Work only in the assigned loop worktree and branch; commit only, never push.
- Focus: auto. The initial focus question received no selection.

## Long-term goal
Make PoBR a trustworthy browser workspace for comparing PoE2 upgrades: preserve player work across imports and stages, keep recommendation limitations visible, and make the next action understandable.

## First milestone: trustworthy build-workspace lifecycle
Status: completed in iteration 4, with the runtime evidence below. No release tag
created because this loop is explicitly commit-only.
Exit criteria:
1. Player-facing English and Chinese onboarding accurately distinguishes adding builds, replacing a stage, sharing a build and restoring a workspace backup.
2. Exercise actual import, stage switching, reload and backup/restore with real WASM; investigate any observed loss of notes, drafts, selection or planning context.
3. Address evidenced lifecycle defects with focused regressions, retaining existing calculation and unsupported-effect boundaries.
4. Review import/planning feedback for actionable errors without discarding prior results.

## Completed milestone: trustworthy whole-item comparison decisions
Status: completed in iteration 5. No tag under commit-only scope.
Exercise the primary README promise on a populated character: compare a complete
item before changing equipment, inspect assumptions and limitations, then apply
only the evaluated candidate.
Exit criteria:
1. Real-WASM comparison across compatible positions preserves the current build until explicit application.
2. Applied item text and displayed recalculation agree with the selected preview, including augment assumptions.
3. Baseline limits and new unsupported effects remain visible and are not mistaken for guaranteed gains.
4. Address only evidenced defects in this path and verify the corrected player-facing surface.

## Completed milestone: trustworthy support recommendation decisions
Status: completed in iteration 7 for the search/application/disclosure path; full mechanic correctness is not asserted. No tag under commit-only scope.
Use a populated skill group to verify eligible support comparisons, player
exclusions and explicit application without changing other skill groups.
Exit criteria:
1. Run real-WASM support search and inspect modeled-effect and bounded-search limits.
2. Excluding a support changes the candidate pool without removing the equipped baseline.
3. Applying a selected result preserves the intended skill group and agrees with previewed output.
4. Fix evidenced player-facing defects; do not widen support mechanic coverage without parity evidence.

## Completed milestone: support downside fidelity
Status: completed in iteration 8 for Controlled Destruction / NeverCrit. No tag under commit-only scope.
Investigate the source-described no-critical-hit downside of Controlled Destruction
before trusting its apparent gain on the populated Frost Bomb fixture.
Exit criteria:
1. Reproduce an isolated equipped/removed comparison and inspect effective hit/crit outputs and existing diagnostics.
2. Trace pinned PoB2 behavior through exported data and the Rust consumer; distinguish a calculation defect from a display-only statistic or source-text mismatch.
3. If a defect is proven, fix the actual consumer with source-backed parity/regression coverage and real-WASM verification. Do not infer mechanics from prose alone or silently suppress unsupported effects.

## Completed milestone: recommendation modeling-limit fidelity
Status: completed in iteration 10 for the two audited resource mechanics, not
exhaustive support-economy coverage. Maximum-resource loss and Life sacrifice
now have distinct source-attributed limits; no sustain formula was invented.
Exit criteria:
1. Reproduce the Life-sacrifice candidate's effects against the pinned source and actual calculated request.
2. Distinguish supported numeric effects from silent input/consumer omissions; do not infer coverage from prose or positive DPS.
3. Correct a proven omission through existing ingestion/diagnostic or calculation boundaries, with source-backed tests and real-WASM verification. Do not invent a sustain approximation or special-case a support name.

## Current milestone: trustworthy passive reallocation decisions
Extend the earlier one-point passive smoke to realistic multi-point routes and
refunds on the populated build. This is the next core upgrade workflow whose
preview/application boundaries have not been exercised deeply in this loop.
Exit criteria:
1. Real-WASM route/refund search respects the requested point budget and preserves the current build before explicit application.
2. Applying a selected plan agrees with its preview and preserves unrelated equipment, gems and workspace stages.
3. Existing/new modeling limits remain visible and distinguishable; fix only evidenced defects rather than inferring complete passive coverage.

## Verified support evidence
- Iteration 6 searched the populated Frost Bomb group (index 4, level 19/quality 20, four support slots). The initial result reported 824 complete recalculations, 8 baseline limits and 8 excluded complete edits.
- Excluded Atziri's Impatience from a recommendation, then excluded equipped Efficiency II through the checklist. Neither altered equipped socket groups or baseline DPS 7347.1; subsequent recommendations contained neither support.
- Applied Considered Casting, Concentrated Area, Controlled Destruction and Elemental Focus. The preview's 19799.8 DPS became displayed hit DPS 19800. Other 12 groups were deep-equal; Frost Bomb remained level 19/quality 20 with main group index 4.
- Reload retained both exclusion IDs under the FrostBombPlayer preference key; Efficiency II remained visibly unchecked.
- Iteration 6 found no search/application defect, but chips lacked effect descriptions. Iteration 7 added a native disclosure using optional verbatim English descriptions from the pinned PoB2 skill definitions, with localized labels and explicit numeric/coverage limits.
- A throwaway assertion incorrectly assumed GemInput carries is_support; corrected the verification to inspect the actual ID/level/quality contract. This was a smoke-script error, not an application failure.
- Regeneration added 538 descriptions to 567 support entries; 28 lack known compatibility and Feeding Frenzy II lacks source prose. Deleting the new fields reproduces the original catalog exactly; the companion trade map regenerated byte-identically.
- Real-WASM search produced 931 complete recalculations. Source-described costs are visible; keyboard toggling, 390px no-overflow layout, all three UI languages and description-free catalog fallback were verified. English prose carries lang=en. Eighteen support tests and TypeScript checking passed; the optional benchmark was skipped.
- Iteration 8 proved the Controlled Destruction discrepancy is a real calculation defect: global_cannot_crit was absent from implicit extraction, NeverCrit was rejected by mapping, and the shared crit resolver had no consumer. All three links are now fixed; no-crit wins over other crit bonuses and its source remains in attribution.
- Both permanent regressions failed before the fix. The pinned PoB2 oracle confirmed crit chance/multiplier zero; after rebuilding WASM, the minimal support fixture reports zero/zero with its noncritical damage benefit retained. Twenty-one crit tests, sixteen support-gating tests, two no-regression parity gates, and scoped fmt/Clippy passed. Independent review approved.
- The populated fixture's formerly inflated 19799.75 DPS is now 4419.6 with Controlled Destruction equipped. Top eight recommendations omit it. Applied the top replacement (Considered Casting, Concentrated Area, Elemental Focus, Atziri's Impatience): preview 28546.6 became displayed 28547, crit returned to 100%, other twelve groups were unchanged, and reload retained the result.
- Exact regeneration added only five implicit flag entries: Controlled Destruction and four Elemental Storm stat sets. Storm's raw hit components remain present in sets 2–4 and all four show zero crit; trigger throughput/full-DPS parity was not established by the direct fixture.
- Iteration 9 proved Atziri's resource-loss input survives extraction but has no upstream or native on-use consumer. Canonical stat prose specifies maximum Life/Mana/Energy Shield. A curated raw-stat classifier now discloses that proven missing mechanic through the existing compatible-host limit observer; costs, recovery and DPS are unchanged.
- Equipped Atziri now appears among nine baseline limits. After removing it and searching with four slots, top eight plans omit it; two excluded complete sets show attributed loss receipts and no Apply controls. Applied Cooldown Recovery II instead: preview 19670.8 displayed 19671, other twelve groups remained unchanged, and reload retained it.
- Nineteen support-gating tests, five WASM support contracts, eighteen planner tests and scoped fmt/Clippy passed. Rebuilt WASM and inspected the real warning surface; independent review approved. Tests preserve unknown/control/manual-consumer behavior and verify the raw mechanic under another support name.
- The temporary numeric oracle probe did not include the lineage support (supports was empty), so it was not used as parity evidence. Source payload/map/consumer inspection establishes the diagnostic, not an inferred sustain model. Maximum-resource-loss disclosure is not exhaustive sacrifice/resource-economy coverage.
- Iteration 10 reproduced Extraction's 10% Life sacrifice missing from diagnostics while its damage benefit applied. Pinned description entry 764 says Life sacrifice without maximum/current semantics. Added a separate raw-stat classification; the same stat is retained for Guatelitzi's Ablation (25) and Sacrificial Offering (15), without support-name filtering.
- Extended the existing three resource-effect regressions to both canonical mechanics, preserving all previous assertions. Two failed before the addition; all nineteen support-gating tests, five WASM support contracts, eighteen planner tests and scoped fmt/Clippy passed afterward. Independent review approved without findings.
- Rebuilt WASM: the minimal Extraction fixture now reports a precise value-10 sacrifice receipt, with DPS46.561230576 and ordinary costs unchanged. Populated search shows two excluded Extraction complete sets with no Apply controls; it finds no better modeled plan than the current Cooldown Recovery II setup. Screenshot inspected; no build edits were applied this iteration.

## Verified whole-item evidence
- Both ring positions were compared without mutating workspace bytes. Ring 2 applied the exact evaluated candidate, preserving all other item slots; preview DPS 6654.85 displayed as 6655 after recalculation.
- A synthetic unmodeled line remained in diagnostics (11 versus baseline 10) and no recommended badge appeared. Manual application remains available under explicit partial-calculation warnings.
- A bare Ancestral Tiara comparison inherited Perfect Iron Rune and disclosed one assumed socket plus excluded costs. As-listed EHP was 24099.31; inherited setup was 24699.76. Applying inherited setup stored the named rune/socket payload and displayed EHP 24700, preserving other slots.
- Identified missing post-apply feedback. Added a localized status naming the evaluated item and destination, gated on matching equipped text/weapon set and settled session work. It does not claim complete mechanic modeling.
- Real browser verified equipment, jewel and flask confirmations; English/Traditional/Simplified messages; absence before apply; clearing on text edit and navigation. Comparison/augment unit tests (39) and typecheck passed.

## Initial survey, 2026-10-11
- README positions the product around comparing gear, supports and passive upgrades; incomplete mechanics are explicitly disclosed.
- docs/web-contracts.md describes independent builds and stages, additive default imports, explicit current-stage replacement and whole-workspace restoration.
- web/src/lib/buildWorkspace.ts implements workspace creation, stage selection, atomic envelope serialization and rejecting malformed collections.
- web/e2e/build-workspace.spec.ts covers independent builds/stages, sharing, reload, restoration and unapplied drafts. Coverage exists; execution is not yet verified in this worktree.
- Recent history concentrates on rejected-import preservation, library deduplication, hand-role candidates and attribution identity. This makes lifecycle reliability a concrete first milestone rather than a speculative new feature.
- `gh issue list --state open --limit 15` succeeded with no open issue rows.
- Confirmed documentation gap: README says importing another build replaces the current one, contradicting the current workspace contract and test scenarios. Correct this first without claiming runtime verification.

## Verified lifecycle evidence
- Iteration 2 built release WASM and synced data 4.5.5.3 in this worktree, then ran Vite on 127.0.0.1:5181.
- Real browser: Starter level 30 and Endgame level 90 retained distinct notes across stage selection and reload. Importing a synthetic level-75 Deadeye added a second build without overwriting either stage.
- Downloaded workspace backup restored two builds after creating a third temporary build; confirmation appeared before replacement.
- Rejected arbitrary JSON preserved the active level, notes and collection. The raw `invalid .build json: missing field passives` error remained visible across pages; actionable/localized import guidance merits review.
- An unapplied +123-life ring draft survived page navigation and reload without becoming equipped. The downloaded workspace backup contained it; the shared build file excluded it.
- Corrected the observed obsolete editor notice in all three languages. English surface visually verified; workspace unit tests (5) and TypeScript check passed.
- Iteration 3 used the committed monk-invoker-frost-bomb code: level 98, 13 socket groups, 10 equipped items, 131 passives. Replacement cancellation preserved exact workspace bytes. Confirmed replacement changed only its copied target stage to level 45; the original populated stage remained intact.
- Sharing and reimporting the two-stage build added an independent build with fresh build/stage IDs and semantically equal saved stages. An initial JSON-string comparison failed only because property order changed; deepStrictEqual passed.
- A one-point passive search produced a 6265.34 → 6488.01 DPS recommendation. Rejecting arbitrary JSON preserved the workspace bytes, displayed stats and completed plan text. Applying the retained recommendation gave 132 allocated nodes and displayed hit DPS 6488.
- Iteration 4 added recovery advice after rejected pastes in all three languages. Recognized PoBR envelopes point to file import with a restore warning; other inputs point to complete supported sources. The original diagnostic remains intact.
- Browser checks verified file/source advice, unchanged workspace bytes after rejected backup paste, advice clearing on edit, language switching and successful populated-code retry with no remaining error or advice. Existing import tests (17) and typecheck passed. No decoder or calculation contract changed.

## Operational notes
- Driver state reports PR publication failure resolving base branch `autoloop-base`. No publishing commands were run; repair remains outside commit-only scope.
- Browser confirmation handling worked when upload and acceptDialog occurred in one ego-browser invocation; splitting across rounds did not restore. Reported tooling discrepancy, not an application defect.
- Pinned PoB2 vendor ce566eac45ea8a86477f513c7ee65a1ebe60014e is now cloned inside this worktree. Iteration 7 regenerated trade metadata, repacked the snapshot manifest and synced 67 runtime files; existing release WASM was reused because no engine/API behavior changed.

## Priority queue
1. Exercise multi-point passive routes/refunds and preview/application integrity on the populated fixture.
2. Keep resource-loss/sacrifice warnings visible; complete support economics and other unknown stat families remain unclaimed.
3. Any further mechanic coverage needs canonical input/consumer evidence and a failing-before regression, not prose or oracle DPS alone.

---

<!-- Merged from autoloop/loop-20261011-1650; the content above is from autoloop/loop-20261011-1441. -->

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

## Completed milestone: Reconcile canonical support-effect fidelity

At milestone start, a targeted static check found no `NeverCrit` or `global_cannot_crit` in this branch's `calc/crit.rs`, `rules/stat_map_engine.rs` or `extract_skill_overrides.lua`. Prior-loop corrections provided concrete leads, requiring current-branch reproduction before integration:

- `d7863a1` — enforce canonical never-crit effects.
- `94392bc` — disclose unmodeled support resource loss.
- `650b70c` — disclose unmodeled support life sacrifice.

Exit criteria:

1. Check current ancestry/content and integration status before reusing prior work; do not independently reimplement an existing fix or overwrite newer code.
2. Reproduce affected support behavior with this branch's real engine. Integrate only still-relevant corrections, preserving current attribution/import/planner work.
3. Verify canonical crit precedence and host scope, accurate source-attributed resource limitations, and actual recommendation consequences with required targeted/parity checks and rebuilt WASM.
4. Keep unmodeled resource loss distinct from ordinary costs; do not invent maximum/current-pool semantics or claim complete mechanic coverage.

## Iteration 6: Reconciled never-crit correction

- Confirmed `d7863a1` is absent from current ancestry and the six relevant source/test/override files still matched its parent exactly. No PR was returned for the prior loop head. Reused that correction through surgical edits, not an independent redesign; regenerated overrides and manifest from pinned vendor `ce566eac45ea8a86477f513c7ee65a1ebe60014e`. Preserved this loop's UI, notes and other data.
- Workspace policy blocked an attempted `git restore` before any file mutation or chained test. No bypass was attempted; subsequent integration used forward edits and the normal generator.
- Reproduced before fixing: a minimal Frost Bomb 19 / Controlled Destruction 1 request retained 13% crit and multiplier 2, DPS 36.758866244, without a diagnostic. The imported core regression failed with expected 0, actual 0.63.
- Added canonical highest-precedence `NeverCrit` resolution, including trace attribution; declared the implemented flag consumable and retained `global_cannot_crit` during implicit extraction. Regeneration added only Controlled Destruction and four Elemental Storm stat sets; only the override hash changed in the manifest.
- Passed 21 core crit tests, 16 support-gating tests, two parity no-regression tests and scoped core/build fmt/Clippy. Rebuilt release WASM and synced runtime data. No full-suite or fresh Lua-oracle claim.
- Actual rebuilt WASM: supported crit/multiplier 0, DPS 32.529970128; another bare group remained 13% crit, multiplier 2, DPS 29.407092995. All four Elemental Storm requests returned zero crit/multiplier; their triggerless fixture also returned zero DPS, so this is not full trigger-mechanic verification.
- Populated Invoker browser smoke: adding Controlled Destruction changed displayed crit from 100%/4.59 to 0%/0 and DPS from 6,265 to 1,706. Support search completed 953 recalculations and returned eight plans, none retaining Controlled Destruction. Removing it restored the original displayed stats and invalidated stale plans. Top recommendation still included Atziri's Impatience, reinforcing the next resource-limit investigation; no recommendation was applied.
- `ReviewNeverCritReconcile` approved with no findings. Inspected the real UI screenshot, finished browser space 34 and removed the temporary image. Vite remains on 5182; pinned vendor and rebuilt WASM are ready for the next integration.

## Iteration 7: Resource diagnostics and milestone closure

- Confirmed the three relevant source/test inputs exactly matched `94392bc^`; combined existing corrections `94392bc` and `650b70c` through surgical edits. No data, frontend or numerical calculation changes.
- Before integration, real WASM returned no limits for Frost Bomb 19 + Atziri's Impatience (DPS 58.814185814) or Extraction (46.561230576), both with ManaCost 89 / LifeCost 0. The native positive receipt regression failed: expected one, got zero.
- Compatible support observation now classifies only the two established finite nonzero unknown raw stats, preserving support/set/stat/value/host identity. Maximum Life/Mana/ES loss and unspecified-pool Life sacrifice remain distinct. Existing conditional diagnostics and numeric benefits/costs are unchanged.
- Reused three prior behavioral regressions. Review suggested stronger guards: extended zero coverage to NaN and both infinities and added recognition-transition coverage using real empty, numerical and LIST catalog entries applied to both exact stat IDs. Fixed an initial test-only private-method compile error by reading the raw catalog fixture; no production API was exposed.
- Passed the 19-test support suite and four WASM support contract tests, then all four resource regressions after the review additions (20 distinct support tests verified in total). Scoped core/build fmt/Clippy passed. Rebuilt release WASM. Numeric outputs did not change, so no new parity or data generation was needed.
- Rebuilt browser WASM returned exactly one correctly attributed receipt per support and every stat matched the pre-fix response. On the populated Invoker, bare-baseline search completed 987 recalculations, returned eight recommendations without Atziri/Extraction and recorded 12 complete-edit exclusions. Both resource effects appeared in explicit exclusion receipts without Apply controls.
- With Atziri equipped, search completed 1121 recalculations: its known limitation remained visible, all eight recommendations retained it, and newly introduced Extraction was still excluded. Removing the smoke support restored displayed DPS 6,265 and cleared stale recommendations.
- Inspected the 390px exclusion view: raw identity/reason wrapped without horizontal overflow. Finished browser space 35 and removed the temporary screenshot. `ReviewResourceReconcile` approved; its one low coverage finding is resolved. Vite remains on 5182.
- Milestone exit criteria met across canonical never-crit precedence/host scope, distinct resource-limit semantics, attributed receipts and actual recommendation consequences. No claim of complete support mechanics, resource sustain, or Elemental Storm trigger-DPS coverage. Commit-only scope means no release tag.

## Completed milestone: Explain support recommendations with canonical effects

At milestone start, recommendation cards showed names, levels, quality and predicted gains rather than intended effects; raw exclusion receipts were not a substitute for mechanic explanations. Prior commit `22b9a63` (`feat: expose sourced support effect descriptions`) required current-branch reconciliation before reuse.

Exit criteria:

1. Compare that correction with current ancestry/source/data before reuse; preserve newer trade-catalog, attribution and planner work.
2. Use pinned-source support descriptions as optional qualitative English prose, clearly labeled as source text, not level/quality-specific values or modeled-coverage evidence. Do not infer effects from names or stat templates.
3. Provide accessible localized disclosure controls and an explicit missing-description state, including older catalogs, without changing planning or Apply semantics.
4. Verify the real recommendation surface, keyboard operation, EN/TW/CN and narrow layouts with appropriate targeted checks and reproducible data generation.

## Iteration 8: Sourced support explanations

- All five source files and the trade catalog matched `22b9a63^` exactly. Reused that feature through surgical edits, preserving current diagnostic/passive work. Added optional support-only qualitative English descriptions to the existing catalog and native `details` disclosures to recommendation cards.
- Regenerated from the pinned vendor plus current crafting inputs: 538 of 567 support records carry descriptions. Removing only the new description fields reproduces the previous parsed catalog exactly; all other data/provenance fields are unchanged. The full artifact is byte-identical to the prior generated `22b9a63` artifact. Manifest changes only the trade-catalog hash, preserving the earlier never-crit override hash.
- Passed 35 support-optimizer/trade-catalog/trade-market tests and production build/typecheck. Synced data; reused iteration 7's unchanged release WASM. Existing bundle-size advisory remains; no Rust/parity rerun for display-only metadata.
- Real populated Invoker search retained the same 987 recalculations, 12 complete-edit exclusions and top DPS 20,129.5. Enter opened the disclosure, Space closed it, and Enter reopened it. Localized labels/names changed across EN/TW/CN while all English prose and `lang=en` annotations stayed unchanged; toggling/localization did not mutate the build or scores.
- Inspected desktop and 390px screenshots: source/coverage disclaimer, readable tradeoffs and no horizontal overflow. No actual screen-reader speech claim.
- Legacy-data smoke removed descriptions only from the real backend's returned catalog before SkillsPanel mounted. All eight plans and displayed metrics stayed identical; all five top-plan supports showed explicit localized unavailable text, with locale-correct `lang` attributes.
- Fixture setup initially waited for a hidden responsive stats panel, then imported a bare Vite backend URL that created a separate singleton and failed to affect the UI. Used the actual query-bearing backend module from resource entries; the corrected legacy check passed. Reload removed the injection and restored all 538 descriptions.
- Applied the top legacy-catalog plan: exact selected gems/groups and every recalculated stat matched the preview, DPS 20129.498504087; unrelated state remained unchanged. The saved test Invoker now has Considered Casting, Concentrated Area, Elemental Focus, Cooldown Recovery II and Deliberation in main group 4. Other saved builds remain intact.
- `ReviewSupportDescriptions-2` approved with no findings. Updated the web contract, finished browser space 36 and removed both temporary screenshots. Explanation milestone exit criteria met; no release tag under commit-only scope.

## Closed milestone: Verify source-stated support tradeoffs

The explained-and-applied Elemental Focus plan initially returned `IgniteDPS=38.352778282` despite its canonical elemental-ailment prohibition. Iteration 9 confirmed and fixed that discrepancy. Continue checking source-stated tradeoffs without confusing potential damage, ailment eligibility and configured enemy conditions.

Exit criteria:

1. Trace the actual pinned Elemental Focus payload and current ailment eligibility/output semantics; distinguish potential damage from an incorrectly allowed ailment.
2. Reproduce any confirmed discrepancy in a minimal native/runtime fixture before changing code, checking previous work and existing consumers first.
3. If a correction is warranted, keep its effects support/host-local, preserve unrelated numerical behavior and source attribution, and verify with targeted regressions, relevant parity and rebuilt real WASM.
4. Keep unsupported semantics explicit rather than guessing. If the reported stat is correct by contract, record that evidence and choose the next concrete gap instead of changing a correct calculation.

## Iteration 9: Canonical elemental ailment prohibitions

- Minimal real WASM reproduced the defect: Fireball 20 had IgniteDPS 0.088695624 bare versus 0.138587011 with Elemental Focus, with no warning. The core prohibition regression failed before implementation: expected zero, got 8000. This is actual chance/uptime-derived DPS, not merely potential magnitude.
- Pinned `sup_int.lua:3771-3805` provides 25% more elemental damage plus implicit `cannot_inflict_elemental_ailments`. The existing stat map produced five `Cannot` flags, but extraction dropped the implicit key and no consumers admitted or honored those flags.
- Added one private attributed prohibition helper at the five existing elemental trace entry points. The ordinary flag-check path does not allocate. Ignite returns zero chance/magnitude/duration/DPS; Shock zero chance/effect; Chill/Freeze/Electrocute zero effect/buildup. Each flag remains independent of the others and of bleed/poison.
- Damaging-ailment orchestration now stops on zero probe chance before the maximum-stack fallback, so prohibited ailments and zero-chance bleed/poison do not report active instances. A regression confirms zero chance means no stacks and 100% chance still creates them.
- Native real-data regression checks all five nonzero baselines, prohibition, retained 25% elemental hit benefit, zero Ignite active stacks, and unchanged complete output in another group, in both effective modes. Core regression covers independent flags, bonuses, disabling source ancestry and unaffected bleed/poison.
- Regeneration added exactly two implicit entries: Elemental Focus and Elemental Armament III. The latter's `sup_str.lua:2891-2920` explicitly carries the same prohibition. All other override metadata/entries are unchanged; manifest changes only that file's hash.
- Passed 76 core ailment tests, 21 support tests, two parity no-regression tests and scoped core/build fmt/Clippy. Rebuilt release WASM and synced data; no full-suite or fresh Lua-oracle claim.
- Rebuilt WASM: minimal supported Fireball keeps hit DPS 156.041666604 but Ignite is zero; bare remains hit DPS 124.833333283 / Ignite 0.088695624. Added elemental hit inputs prove all five outputs are positive bare and zero supported, for both Focus/Fireball and Armament III/Armour Breaker. No other group is mutated.
- Inspected the populated Invoker UI: hit DPS remains 20,129, invalid Ignite is absent and combined DPS now matches the hit result. `ReviewElementalProhibition` approved without findings. Finished browser space 37 and removed the screenshot.
- Follow-up lead, not a confirmed defect: Armament III's source promises 12% more attack damage per elemental ailment on the enemy. Direct probes with `config_inputs.conditionEnemyShocked` or `conditionEnemyChilled` left both bare/supported DPS at 288642.172580676, no warnings. Those inputs might be inactive/unrecognized under the request's calculation mode; verify accepted configuration and multiplier semantics before deciding whether the benefit is missing.

## Iteration 10: Disclose Armament III's unmodeled conditional benefit

- Resolved the configuration ambiguity: `loadConfigOptions` exposes `conditionEnemyChilled` as an accepted checkbox, and WASM requests default to effective mode. A known `100% increased Damage against Chilled Enemies` modifier responds to that input: the minimal Armour Breaker fixture changes from DPS 2520.807363666 to 5042.010631836. Adding Armament III leaves both results unchanged and originally reported no limit.
- Pinned `sup_str.lua:2891-2920` carries `support_elemental_armament_attack_damage_+%_final_per_elemental_ailment_on_target=12`; `gem_stat_descriptions.lua` entry 954 explicitly describes more/less Attack Damage per elemental ailment on the enemy. The stat is unknown to the mapper and has no separate consumer. This is a missing benefit, not an inactive configuration or the already-corrected prohibition.
- Added only that exact stat to the existing finite/nonzero Unknown-only `SupportStatLimit` classifier. No guessed ailment counts, numerical model, new JSON fields, data regeneration or frontend special case. Existing recognized-mapping, compatibility, source identity and group boundaries remain in force.
- Added a JSON regression for the source-specific receipt with enemy Chill off/on and absence of leaked limits from another or incompatible main group. It failed before the change (zero receipts, expected one), then passed. All five support JSON contract tests, 21 native support tests and scoped core/WASM fmt/Clippy passed. Rebuilt release WASM; no fresh parity/full-suite/Lua-oracle claim for this diagnostic-only change.
- Rebuilt real WASM preserves the four repro DPS values exactly, with one receipt only for equipped Armament III. Real UI testing in the isolated `Armament coverage verification` build shows the receipt in current-build limits while equipped. After removal and recalculation, its candidate appears among excluded complete edits, with no Apply button and no Armament recommendation; the stale baseline warning disappears.
- Inspected the exclusion screenshot. Restored the previously selected Invoker, finished browser space 38 and removed both screenshot files. The named synthetic test build remains in the local library; the seven pre-existing builds remain listed.
- Source-tradeoff milestone exit criteria are met: the demonstrated prohibition error is corrected and the demonstrated missing conditional benefit is explicitly disclosed. Commit-only scope means no tag, push or PR action. The paused `/goal` remains untouched.

## Closed milestone: Conditional support damage fidelity

Completed after the user requested continuation beyond the ten-iteration loop. Armament III now uses the source-grounded configured-target model below; its obsolete limitation receipt is removed. The broad trustworthy-workspace goal is not declared complete, and the paused `/goal` remains untouched.

## Post-loop continuation: Numerical Armament III support

- Research established five independent elemental types from pinned `ModParser.lua:3866-3872`; canonical entry 954 supplies the attack-damage percentage, and `SkillStatMap.lua:922-924` / `ModStore.lua:550-571` establish the single final-per-count MORE convention. There is still no direct Armament consumer in pinned Lua, so this is explicitly a source-grounded model rather than Armament-specific oracle parity.
- Added an exact code-owned missing-entry rule in `map_stat`, preserving real catalog precedence and the usual support attribution/group path. One Attack-scoped Damage MORE modifier consumes the runtime payload and derived `EnemyElementalAilmentCount`; it does not compound separate MORE terms.
- Count only configured Ignited, Chilled, Frozen, Shocked and Electrocuted, once each, in effective mode. Ignore stacks, Burning, Immobilised and infliction capability. Positive counts allocate one multiplier entry; zero/panel counts remove any imported entry without allocating a zero key.
- Removed the stale Frozen→Chilled implication, which contradicts current pinned ConfigOptions. Preserved Ignited→Burning. Explicit raw true/false ailment inputs now override imported legacy values, preventing stale bonuses after a disable edit.
- Removed the obsolete Armament unsupported-stat arm and migrated the JSON contract to numerical behavior. Regressions cover 0–5 distinct counts, alternate payloads, catalog precedence, unrelated unknowns, spell/ordinary-DoT gates, physical/chaos attack components, once-only bleed/poison source-hit scaling, source attribution, prohibition, group compatibility and false-edit transitions.
- Passed 77 core mapping tests, 13 config-resolver tests, all 24 support tests, two parity gates and five support JSON contracts (121 distinct tests), scoped core/build/WASM fmt/Clippy, and release WASM build. Initial effective-DPS assertions incorrectly assumed linear armour mitigation; corrected that expectation while preserving exact raw-hit/component/ailment factors and exact neutral/panel assertions. No tolerance widening or production workaround.
- Rebuilt real WASM passed 16 paired cases: every singleton 1.12, Frozen+Chilled 1.24, all five 1.60, zero/panel 1.00. All five self-inflicted elemental outputs remain zero and the obsolete receipt is absent.
- Real UI: external Chill changes the equipped test build's average hit from 6.4 to 7.17 and displayed DPS from 0.42 to 0.47. Equipped baseline has no Armament warning; removing it and recalculating no longer excludes the candidate for that mechanic. Eight plans remain available. Restored the prior Invoker, closed space 39 and removed the screenshot; the named synthetic test build retains Armament plus external Chill for reproduction.
- `ReviewArmamentModel` found no production defects; its stale-contract finding is resolved by the updated web contract. No generated data, extraction provenance, user build or other branch was modified. Commit-only continuation; no automatic push.
