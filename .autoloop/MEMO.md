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

## Next milestone: Conditional support damage fidelity

Armament III's numerical benefit is still unmodeled, now safely disclosed. Before implementing it, establish authoritative ailment-type counting, frozen/chilled implications, externally supplied versus self-inflicted conditions and effective-mode behavior with a source-backed native/runtime or oracle fixture. Do not infer those semantics from a zero-response probe. Preserve the implemented prohibition and retire its Unknown-only receipt only when the real mapping/consumer exists. The broad trustworthy-workspace goal is not declared complete; the driver governs the ten-iteration limit.
