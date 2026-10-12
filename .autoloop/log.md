# Iteration log

## Iteration 1 — 2026-10-11
- Focus question received no selection; recorded auto. Surveyed README, workspace contracts, workspace implementation, lifecycle E2E scenarios, recent history and open issues (none returned).
- Set long-term direction to trustworthy upgrade comparisons and first milestone to build-workspace lifecycle reliability. Exit criteria and next actions are in MEMO.md.
- Corrected English and Chinese README onboarding: code imports add builds by default; replacement targets the explicitly selected stage; sharing one build differs from restoring the entire workspace, including drafts.
- Verification: reviewed changed prose against docs/web-contracts.md workspace and import contracts, buildWorkspace.ts atomic envelope/stage representation, and build-workspace.spec.ts additive-import, explicit-stage, sharing and restoration scenarios. No links changed. Documentation-only change; no runtime behavior changed, no build or test execution claimed.
- Commit: this entry is included in the iteration's `docs: clarify build imports and workspace backups` commit.
- Next: establish real-WASM browser execution and exercise lifecycle scenarios, then prioritize observed defects rather than speculative changes. Milestone remains open.

## Iteration 2 — 2026-10-11
- Prepared this fresh worktree with frozen pnpm dependencies, a release WASM build and synced data; used the actual Vite application in ego-browser.
- Exercised stage copy, distinct level/notes, reload, stage switching, additive PoB-code import, workspace backup download and confirmed restore. Rejected arbitrary JSON left the valid build intact.
- Verified an unapplied ring draft survived reload, remained absent from equipped items, appeared in the downloaded full backup and stayed excluded from the shared-build file.
- Found and fixed misleading draft persistence copy in English, Simplified Chinese and Traditional Chinese. The prior notice said session-only and incorrectly excluded unapplied drafts from backups. Updated the web guide.
- Verification: actual before/after editor screenshots inspected; revised English notice visible. `pnpm --dir web test src/lib/buildWorkspace.test.ts` passed 5 tests; `pnpm --dir web typecheck` passed. No permanent wording test added.
- A cross-round ego-browser confirmation failed to continue restoration; upload and confirmation in one invocation succeeded. Reported the tool discrepancy. No application restore failure established.
- Driver reports a PR base-ref failure for `autoloop-base`; did not push or change publication configuration.
- Commit: this entry accompanies `fix: explain persisted editor drafts accurately`.
- Next: explicit replacement/cancellation, shared-build reimport and non-empty planner continuity; investigate actionable import-error guidance. Lifecycle milestone stays open.

## Iteration 3 — 2026-10-11
- Used the existing real-WASM Vite server and the committed monk-invoker-frost-bomb example (level 98, 13 groups, 10 items, 131 passives).
- Copied a stage, cancelled replacement and asserted exact workspace-byte preservation. Confirmed replacement then changed only the copied stage to a synthetic level-45 character.
- Downloaded and reimported the two-stage shared file. Verified new build/stage identities and deep equality of both saved snapshots. An overly strict initial JSON-string check detected property ordering, not a semantic difference; corrected the throwaway assertion.
- Ran a one-point passive search. After a rejected arbitrary-JSON import, exact workspace bytes, displayed stats and completed result text remained unchanged. Applied the retained first recommendation: allocation grew 131 → 132 and displayed hit DPS became 6488, matching the preview's 6488.01 rounded display.
- No application lifecycle defect surfaced in these scenarios. The raw `invalid .build json: missing field passives` error still offers no recovery action. Next iteration should improve localized import guidance without suppressing the diagnostic or changing accepted formats.
- Tool limits: ego-browser auto-scroll failed on the below-fold planner button; explicit scrollIntoView followed by a real click worked. Reported the discrepancy. A canvas wait was an incorrect smoke selector for the SVG tree, not an app failure.
- Investigation only: no product code, permanent tests or runtime dependencies changed. Reused iteration-2 WASM; no duplicate build/test run. Removed temporary shared file and screenshot; closed browser space.
- Commit: this entry accompanies `chore: record populated workspace lifecycle verification`.

## Iteration 4 — 2026-10-11
- Added small panel-local recovery advice after rejected pasted imports. Reused buildFileKind for PoBR-envelope ownership; accepted formats, confirmation paths, original diagnostics and calculation state are unchanged.
- All three locales now direct recognizable saves/backups to file import with a whole-workspace warning; other failed inputs direct players to complete supported sources. Advice clears on input edits and new import attempts and is connected to the textarea with aria-describedby.
- Verification: existing import boundary tests passed (17); TypeScript check passed. Real-WASM browser verified both advice branches, original error retention, exact workspace-byte preservation for a rejected backup paste, edit clearing, translated advice and successful populated PoB-code retry with no stale error/advice. English screenshot inspected.
- Updated the Web guide. No wording-only permanent test added; removed temporary screenshot and closed the browser.
- Lifecycle milestone exit criteria now have runtime evidence across iterations 2–4. Closed that milestone without a tag under the explicit commit-only scope.
- Next milestone: trustworthy whole-item comparison decisions. Start with a populated build and both ring slots, inspect unsupported effects and augment assumptions, then verify explicit apply agrees with preview.
- Commit: this entry accompanies `fix: guide players after rejected build imports`.

## Iteration 5 — 2026-10-11
- Exercised complete-item comparisons on the populated Frost Bomb build. Both rings compared without saved-state mutation; Ring 2 applied exact candidate text and preserved unrelated equipment. Predicted 6654.85 DPS became displayed 6655.
- An unmodeled candidate line remained visible and prevented a recommendation badge. Baseline partial-calculation limits remained visible; no new scoring or exclusion policy introduced.
- Helmet comparison inherited Perfect Iron Rune and disclosed an added socket/cost assumptions. As-listed EHP 24099.31 differed from inherited EHP 24699.76; inherited apply stored the evaluated rune/socket text and displayed 24700.
- Fixed the observed lack of post-apply feedback with a three-language item/destination confirmation. It appears only after pending work settles and the evaluated text is present in the active weapon context, including asynchronous jewel application; edits, new comparisons and navigation clear it.
- Verification: comparison/augment tests passed (39) and typecheck passed. Actual browser verified equipment, jewel and flask messages, all three locales, no message before application, and removal on edit/navigation. Before/after screenshots inspected.
- Updated the Web guide, removed temporary screenshots and closed the browser. No permanent wording-only test added.
- Whole-item milestone complete; next is support recommendation trust, exclusions and apply continuity. Commit-only scope means no milestone tag.
- Commit: this entry accompanies `feat: confirm applied item comparisons`.

## Iteration 6 — 2026-10-11
- Ran real-WASM support search on the populated Frost Bomb group, four support slots. Initial search displayed 824 complete recalculations with 8 current-build limits and 8 excluded complete edits.
- Excluded Atziri's Impatience from a recommendation and equipped Efficiency II from the pool. Both disappeared from subsequent recommendations; equipped groups remained unchanged and baseline DPS stayed 7347.1. Reload retained both exclusions and the unchecked Efficiency II control.
- Applied the top remaining four-support set. Other 12 groups were deep-equal, main group stayed index 4, and Frost Bomb stayed level 19/quality 20. Preview DPS 19799.8 became displayed hit DPS 19800. Screenshot inspected.
- Corrected two throwaway automation assumptions: ego-browser requires terminal nth selectors, and saved GemInput has no is_support flag. Neither was an application defect.
- Investigation only; no runtime code, permanent tests or dependencies changed. Existing WASM reused; no duplicate build/test run.
- Next: investigate authoritative support-effect text for recommendation details. Current chips expose names/levels and gains but do not explain effects or downsides. Do not infer descriptions or coverage from names.
- Removed temporary screenshot and closed the browser. This evidence checkpoint advances the support milestone without claiming the explanatory gap is fixed.
- Commit: this entry accompanies `chore: verify support exclusion and application continuity`.

## Iteration 7 — 2026-10-11
- Two read-only investigations established that existing snapshots do not carry a complete level-specific support description renderer. The existing trade catalog extractor already joins the canonical skill definitions, making qualitative source prose the smaller authoritative seam.
- Cloned pinned PoB2 ce566eac45ea8a86477f513c7ee65a1ebe60014e in this worktree. Added optional support-only `description` metadata and regenerated the catalog: 538/567 supports have source prose. Twenty-eight lack known compatibility; Feeding Frenzy II genuinely lacks a description.
- Semantic comparison after removing new fields exactly matched the original catalog. Regenerated the companion trade map byte-identically, repacked the manifest and synced 67 runtime files.
- Added per-plan native effect disclosures with localized names/labels, explicit English-source/qualitative/coverage limits, and visible unavailable fallback. No changes to scoring, compatibility, calculation or application behavior. Updated the existing Web contract.
- Real-WASM Frost Bomb search completed 931 recalculations. Verified source text for Considered Casting, Concentrated Area, Controlled Destruction and Atziri's Impatience; Enter toggling; 390px screenshot and no disclosure overflow; English/Traditional/Simplified labels.
- A temporary backend metadata fixture removed every description while retaining real WASM calculations; all recommendation entries showed the unavailable fallback and search still completed. Restored the backend and reloaded afterward.
- Eighteen support optimizer tests passed; the optional support benchmark was skipped. TypeScript checking passed. Independent review approved with a nonblocking language-annotation finding; fixed it and verified source paragraphs retain lang=en under Simplified Chinese UI.
- Closed the browser and removed the screenshot. No temporary hooks or scripts remain. The browser fixture's exclusions were restored to all candidates during this iteration; equipped supports were not changed.
- Support decision-path milestone complete. Next: investigate the observed Controlled Destruction/100%-Crit-Chance discrepancy using effective outputs and pinned mechanics before asserting or fixing a calculation bug.
- Commit: this entry accompanies `feat: expose sourced support effect descriptions`.

## Iteration 8 — 2026-10-11
- Reproduced Controlled Destruction's missing downside in real WASM. Minimal Frost Bomb retained 13% crit and gained 25% DPS with no unsupported diagnostic; the populated fixture retained 100% crit. Captured a failing browser assertion and failing permanent core/build regressions.
- Pinned PoB2 oracle ce566eac45ea8a86477f513c7ee65a1ebe60014e confirmed zero crit chance and multiplier. Two read-only traces found three gaps: missing implicit-stat export, rejected NeverCrit flag mapping, and no shared crit consumer.
- Fixed the existing extraction/mapping/resolver chain. NeverCrit precedes crit bonuses, returns chance/pre-effective chance/multiplier zero and average factor one, and preserves the disabling source in traced attribution. No schema or public API change.
- Regenerated overrides and manifest. Exactly five canonical implicit entries were added: Controlled Destruction plus four Elemental Storm stat sets; all earlier override content and provenance were unchanged.
- Verification: 21 critical-pipeline tests, 16 support-gating tests, two no-regression parity tests, and scoped fmt/Clippy passed. Rebuilt release WASM and synced 67 runtime files. Independent review approved with no findings.
- Browser: minimal fixture now has zero crit/multiplier and DPS32.529970128. Populated baseline changed from inflated19799.75 to4419.6; top eight recommendations omit Controlled Destruction. Applied the top plan: preview28546.6 displayed28547, crit returned to100%, other twelve groups stayed identical, and reload retained it.
- A throwaway Elemental Storm probe incorrectly required positive DPS in a direct fixture without trigger throughput. Raw inspection showed preserved Fire/Lightning/Cold hits for sets2–4 and zero crit across all four; no claim of full trigger or full-DPS oracle parity. No production workaround was added.
- Closed the browser and removed temporary XML/oracle/screenshot artifacts. Settled both prior-iteration research agents and this iteration's completed agents.
- Milestone complete for this mechanic. Next: trace source-described resource costs of Atziri's Impatience into actual consumers/diagnostics; its presence in all top plans is not proof those costs are modeled.
- Commit: this entry accompanies `fix: enforce canonical never-crit effects`.

## Iteration 9 — 2026-10-12
- Reproduced Atziri's Impatience doubling minimal Frost Bomb DPS29.40709→58.81418 while ManaCost89/LifeCost0 remained unchanged and no resource-loss output or diagnostic appeared. Captured a failing real-WASM assertion and failing permanent diagnostic regressions.
- Canonical data retains loss25; pinned stat-description entry618 specifies maximum Life/Mana/Energy Shield. Neither upstream stat mapping nor native costs/recovery implements this on-use loss. Ordinary costs and negative regeneration are not valid substitutes. The numeric oracle fixture omitted the lineage support, so its result was not used as parity evidence.
- Added a narrow raw-stat modeling-limit classification at the existing compatible-host support observer. It applies to finite nonzero Unknown loss payloads, preserves source/set/stat/value/host identity, and leaves generic Unknown semantics, numerical outputs and planner filtering unchanged. No support-name ban, schema change, data regeneration or invented sustain calculation.
- Regressions cover real receipt attribution, unchanged ordinary costs, zero payload, unrelated/incompatible groups and the same mechanic transplanted onto another support. Corrected an initial whole-output control fixture to retain the unrelated Frost Bomb group: removing the entire group legitimately changed curse-limit metadata and did not isolate the tested support.
- Verification:19 support-gating tests,5 WASM support-contract tests,18 frontend planner tests, and scoped fmt/Clippy passed. Rebuilt release WASM. Independent review approved with no findings.
- Real browser: equipped baseline shows9limits including the resource-loss receipt. Removed Atziri and searched four slots: top8plans exclude it; two excluded complete sets show the loss reason and have no Apply controls. Applied Cooldown RecoveryII: preview19670.8 displayed19671, other12groups stayed identical, reload retained it.
- Closed the browser and removed temporary XML/oracle/screenshot artifacts. Source disclosure is complete for this proven maximum-resource-loss mechanic, not for all support economics.
- Next: investigate Extraction's distinct Life-sacrifice payload and diagnostic coverage without assuming maximum/current-resource semantics.
- Commit: this entry accompanies `fix: disclose unmodeled support resource loss`.

## Iteration 10 — 2026-10-12
- Reproduced Extraction's silent Life sacrifice in real WASM: DPS29.40709→46.561230576 with ManaCost89/LifeCost0 and no diagnostic. Canonical raw payload retains sacrifice10 plus35gain-as-chaos; pinned stat-description entry764 says Life sacrifice without maximum/current qualification. No exact stat mapper/consumer exists.
- Added the distinct sacrifice key to the existing finite/nonzero Unknown-mechanic classifier. The warning names on-use Life sacrifice, not maximum-resource loss or ordinary cost. No numerical, data, schema or support-name filtering changes.
- Extended the existing three resource-effect regression tests to preserve all Atziri cases and cover Extraction's source identity, zero payload, unchanged whole output, sibling/incompatible-group boundaries and stat transplantation. Two assertions failed before the production change.
- Verification:19support-gating tests,5WASM support contracts,18planner tests and scoped fmt/Clippy passed. Rebuilt release WASM. Independent review approved with no findings.
- Browser: the exact value10 sacrifice receipt now appears while DPS/costs remain unchanged. The populated search displays two excluded Extraction complete sets with no Apply controls and no better modeled recommendation. Expanded the actual exclusion reason and inspected its screenshot. No build edits were applied.
- Closed the browser and removed the screenshot. This completes the bounded two-mechanic disclosure milestone, not exhaustive support economics or the long-term product goal.
- Next handoff: exercise realistic multi-point passive routes/refunds, preview agreement and unrelated-state preservation; earlier loop evidence covered only a one-point passive application.
- Commit: this entry accompanies `fix: disclose unmodeled support life sacrifice`.

---

<!-- Merged from autoloop/loop-20261011-1650; the content above is from autoloop/loop-20261011-1441. -->

# Iteration log

## 1 — Survey and populated passive-route verification

- Chose the long-term goal of trustworthy build decisions and the first milestone of passive reallocation integrity. Surveyed product/docs, current planner and tests, recent history and open issues (none returned). Recorded auto focus; left the paused `/goal` untouched.
- Built this branch's release WASM and ran the actual web app on port 5182. Imported the committed level-98 Frost Bomb fixture, searched a three-point route and visually inspected its tree preview and limits.
- Search evaluated 391 plans. Search/preview did not mutate saved state. Apply added exactly nodes 17796/12412/45497 (131 to 134 total), preserved every other state field and the other saved build, and produced DPS 7560.833856375 against preview 7560.83.
- Reload verification passed semantic deep equality on a fresh repeated flow. The initial raw-JSON comparison failure was object-key ordering, not a persistence defect.
- Refund search was correctly unavailable for the imported build's 23/22 weapon-exclusive nodes, with a visible explanation. Positive refunds, changed travel attributes, stale plans and excluded edits remain milestone work.
- No production-code changes or permanent tests were justified by this investigation. No full test/parity pass is claimed. Branch-local build, live WASM calculations, UI actions and screenshots are the evidence.
- Removed temporary screenshots and finished browser space 29. Retained the Vite service for the next iteration. Commit: this entry's checkpoint commit (`chore: record populated passive route verification`).
- Next: a connected positive refund/reallocation fixture with a replacement travel attribute, protected sockets and exact preview/apply/reload checks.

## 2 — Positive refunds, travel attributes and stage isolation

- Imported a synthetic connected Witch through the real encoder and normal code-import UI: 13 allocated nodes, a protected occupied socket and a three-node refundable crit branch. Reused unchanged branch-local WASM and data.
- Real refund search evaluated 228 plans. Preview did not mutate saved state. Apply replaced three crit nodes with three Strength attributes, kept 13 connected nodes, retained the complete socket path and all old attribute choices, and changed no unrelated state field.
- Recalculated DPS 75.345833303, Life 2352 and EHP 1731.785923019 agreed with the visible preview. Visually inspected both allocation and refund markers; removed the screenshot afterward.
- Copied-stage planning discarded old receipts. Three-point results refreshed to one-point results after a budget change without applying automatically. Applying in the copy preserved the original stage exactly. Reload retained the copied state; switching back restored the original refunded state.
- One smoke wait assumed a recommendation would exist after copying. The reset Intelligence travel choice legitimately produced no improving Max Life routes in 512 evaluations. Inspected completion, selected Strength explicitly and continued; no product fix was warranted.
- No production code or permanent tests changed. These live checks advance the milestone but do not prove all passive mechanics. Browser space 30 finished; Vite remains on 5182. Commit: this entry's checkpoint commit (`chore: verify passive refunds and stage isolation`).
- Next: real excluded-edit diagnostics and completed-plan invalidation after a build edit, then conclude the bounded milestone.

## 3 — Diagnostic trust closure and next product gap

- Imported the real timeless-jewel diagnostic fixture via the normal encoder/import UI. A two-point search evaluated 151 plans and excluded 26 complete edits without mutating saved state.
- Confirmed the baseline node 46819 reason stayed separate from the exact `[49512,5936]` excluded route's two new reasons. That receipt had no actions; navigation retained its associations. Visually inspected the expanded diagnostic.
- Produced ten actionable Strength-travel plans, then replaced the jewel through the socket editor after navigation paused tracking. Only the jewel changed; all old receipts and Apply controls disappeared. A new 101-plan search had ten recommendations and no stale seed-data warnings.
- Completed the bounded passive-integrity milestone using evidence from iterations 1–3. No calculation-completeness or full-suite claim; no production defect was observed and no speculative fix was added.
- Observed a concrete UX gap for the next milestone: successful Apply added nodes 42736/4203 and immediately refreshed to a 115-plan baseline, leaving no applied confirmation/status. Next milestone is clear, state-scoped feedback for ordinary and joint passive applications, with existing localization and no new notification infrastructure.
- Finished browser space 31, removed the screenshot and retained Vite on 5182. No production files or permanent tests changed. Commit: this entry's checkpoint commit (`chore: close passive integrity verification milestone`).
- Next: implement ordinary passive-route application feedback, then cover the joint planner and verify message lifetime against real state changes.

## 4 — Ordinary passive application feedback

- Added localized live-status confirmation to ordinary passive Apply, with exact node names/IDs and allocation/refund counts. It is scoped to the applied session revision and hidden during pending/failed calculation, so refreshed searches retain it but subsequent edits cannot resurrect an old success.
- Added a real-WASM browser regression for ID correctness, refresh/navigation retention, edit-and-restore invalidation and copied-stage reset. Updated the web contract.
- Passed 19 planner tests, production web build/typecheck and the new Playwright case. Corrected a test-only line-break comparison mismatch on the first browser-test run; production code did not change afterward.
- Live browser verified exact allocated and refunded nodes, automatic refresh, EN/TW/CN copy and 390px layout. Held a real result before delivery to verify pending suppression; injected a rejection after real calculation to verify failure suppression, then reloaded to remove the injection.
- Independent reviewer approved with no blocking findings. Actual screen-reader speech remains unverified. Browser space 32 finished; temporary screenshots/config removed; no Rust/WASM/data changes.
- Commit: this entry's checkpoint commit (`feat: confirm applied passive routes`). Next: equivalent feedback for joint jewel/passive plans and cross-planner lifetime verification.

## 5 — Joint feedback and application-outcome milestone closure

- Added revision-scoped joint jewel/passive success feedback, capturing the selected placement from the completed search. Shows kept jewels when appropriate and exact allocation/refund details; later candidate edits cannot rewrite it.
- Extended lifetime regression to both planners and added joint → route → joint invalidation. Passed 97 targeted unit tests, production build/typecheck and three real-WASM Playwright tests.
- Live browser verified replacement plus nodes, candidate removal/fresh-search retention, identical-jewel no-op exclusion, kept-jewel wording, EN/TW/CN and 390px layout. Scheduling/rejection fault injection confirmed pending/failed calculations cannot show success; reload removed the injection.
- Independent review approved with no findings. Updated web contract, finished browser space 33 and removed temporary artifacts. Actual screen-reader speech remains untested; no Rust/WASM/data changes.
- Closed the application-outcome milestone. Next priority is canonical support fidelity: this branch's targeted extraction/mapping/crit files lack the known never-crit symbols, and prior corrections d7863a1/94392bc/650b70c exist locally. This static lead requires current-branch reproduction and ancestry/content reconciliation before integration, not duplicated implementation.
- Commit: this entry's checkpoint commit (`feat: confirm applied jewel and passive plans`). Next: reconcile and verify the existing never-crit correction against this branch.

## 6 — Canonical never-crit integration

- Reconciled existing correction `d7863a1` against current ancestry and identical pre-fix inputs, then reused it through surgical edits and pinned-vendor regeneration. Preserved current UI/data/loop work; generated delta is exactly five implicit-stat entries and the corresponding manifest hash.
- Reproduced the missing downside in real WASM and a failing core regression before implementation. Shared crit resolution now honors `NeverCrit` before bonuses and preserves source attribution; compatible support scope remains group-local.
- Passed 21 crit, 16 support and two parity no-regression tests, scoped fmt/Clippy, release WASM build and data sync. Rebuilt WASM gave supported crit/multiplier 0 and DPS 32.529970128 instead of 36.758866244; the bare group stayed unchanged.
- Live populated Invoker UI showed 0% crit/0 multiplier with Controlled Destruction and restored 100%/4.59 after removal. The 953-recalculation support search returned eight plans without that support; removal invalidated stale results. Four Elemental Storm sets returned zero crit/multiplier, but the triggerless fixture does not prove their full DPS behavior.
- Independent review approved with no findings. Updated the web contract, closed browser space 34 and removed the temporary screenshot. No full-suite or fresh Lua-oracle claim.
- Commit: this entry's checkpoint commit (`fix: enforce canonical never-crit effects`). Next: reconcile the existing resource-loss and life-sacrifice diagnostic corrections; Atziri's Impatience still appears in the live top recommendation.

## 7 — Resource downside disclosure and support-fidelity closure

- Reconciled and combined prior fixes `94392bc` / `650b70c` against identical current source/test inputs. Added source-attributed limits for unknown finite nonzero on-use maximum-resource loss and Life sacrifice; preserved canonical semantic distinctions, existing conditional diagnostics and numerical outputs.
- Real pre-fix WASM omitted both limits; a native regression failed with zero receipts instead of one. Rebuilt WASM now emits each exact support/set/stat/value/host receipt with all stats unchanged from the before response.
- Passed 19 support tests and four WASM contract tests, then four resource tests including reviewer-requested invalid-value/recognized-mapping guards (20 unique support tests total). Scoped core/build fmt/Clippy and release WASM build passed. Corrected a test-only private lookup compile error through the existing raw catalog fixture; no production API change.
- Populated bare-baseline search: 987 recalculations, eight plans excluding both new resource downsides, 12 attributed complete-edit exclusions without Apply controls. Equipped-Atziri search: 1121 recalculations, its baseline warning remains visible while all eight plans may retain it; new Extraction remains excluded. Removal restored original DPS and invalidated plans.
- Independent review approved, low boundary-coverage finding resolved. Verified 390px receipt wrapping without horizontal overflow, finished browser space 35 and removed the screenshot. Updated web contract. No numeric/data changes, fresh parity/oracle or full-suite claim.
- Closed canonical support-fidelity milestone. Next: source-based support effect explanations; reconcile existing `22b9a63` before integration. No release tag under commit-only scope.
- Commit: this entry's checkpoint commit (`fix: disclose unmodeled support resource downsides`).

## 8 — Sourced support explanations

- Reconciled and reused `22b9a63` against identical source/catalog inputs. Added optional pinned English support prose and native localized recommendation disclosures, with explicit missing-description and modeled-coverage boundaries.
- Regeneration added descriptions to 538 support records only. Other parsed catalog fields stayed identical, the artifact byte-matched the prior generation, and only its manifest hash changed.
- Passed 35 affected frontend tests and production build/typecheck; synced data and reused unchanged WASM. Independent review approved without findings. Existing chunk-size advisory remains.
- Live browser verified Enter/Space controls, EN/TW/CN labels/names, unchanged English source text and scores, and desktop/390px layout. A catalog with all descriptions omitted produced the same eight recommendations with localized missing-source messages. Corrected two probe setup mistakes (hidden panel wait and wrong Vite module identity); final legacy smoke used the app's actual backend.
- Applied the top plan: exact gem groups, all preview stats and unrelated state preservation passed, DPS 20129.498504087. Reload removed the metadata injection and restored canonical descriptions. Finished browser space 36 and removed screenshots; the applied synthetic test build remains available.
- Closed the explanation milestone. New lead: Elemental Focus's source prohibits elemental ailments, but that applied plan reports IgniteDPS 38.352778282. Next iteration must check canonical eligibility versus potential-DPS semantics before assuming a defect.
- Commit: this entry's checkpoint commit (`feat: expose sourced support effect descriptions`).

## 9 — Enforce elemental ailment prohibitions

- Confirmed the Elemental Focus lead through minimal real WASM and a failing native regression. Its canonical implicit prohibition was missing from extraction and its five mapped flags had no consumers.
- Added attributed early gates for Ignite/Shock/Chill/Freeze/Electrocute, admitted the implemented flags and retained the implicit stat. Fixed zero-chance damaging ailments falling into assumed maximum active stacks. Positive-chance bleed/poison and separately configured enemy effects remain independent.
- Generated delta is exactly two implicit entries (Focus and Armament III) plus the override manifest hash. Passed 76 ailment tests, 21 support tests, two parity gates, scoped lint, release WASM build and data sync.
- Real WASM verified all five nonzero bare ailments become zero for supported spell and attack hosts, retaining hit damage. Populated UI no longer includes invalid Ignite while hit DPS stays 20,129. Independent review approved; updated contract, closed space 37 and removed screenshot.
- Next: Armament III's conditional damage bonus showed no response to preliminary shocked/chilled config-input probes. This is not yet a proven missing mechanic: inspect accepted keys/mode and canonical multiplier wiring before changing calculations.
- Commit: this entry's checkpoint commit (`fix: enforce elemental ailment prohibitions`).

## 10 — Disclose Armament III's unmodeled conditional benefit

- Confirmed the input works: a known chilled-enemy damage modifier responds in real WASM, while Armament III's canonical 12%-per-ailment benefit remains absent with no warning. Traced its exact unknown stat and pinned wording; no separate consumer exists.
- Extended the existing Unknown-only support receipt boundary for that mechanic. This is a coverage correction, not an invented damage multiplier; the implemented elemental-ailment prohibition remains independent.
- Regression failed before the fix and passed afterward. Passed five support JSON contract tests, 21 native support tests, scoped core/WASM lint and release WASM build. Live before/after DPS values match.
- Real planner UI proves equipped-baseline disclosure, newly introduced candidate exclusion, no Apply action on the excluded Armament set, and no stale baseline warning after removal. Restored the prior Invoker, closed space 38, removed screenshots; the isolated named test build remains available.
- Closed the source-tradeoff milestone. Next worthwhile work is authoritative conditional-ailment counting and numerical modeling, with the current diagnostic retained until implemented. No claim of full mechanical coverage or completion of the broad goal; no tag/push/PR, and paused `/goal` unchanged.
- Commit: this entry's checkpoint commit (`fix: disclose unmodeled Armament damage bonus`).

## Post-loop — Implement Armament III conditional attack damage

- Followed the user's continuation request. Implemented one source-grounded MORE Attack Damage term using the runtime percentage times five independently configured elemental ailment types, Effective-only. Real extracted mappings keep precedence over the exact code-owned missing-entry rule.
- Corrected stale Frozen→Chilled implication and explicit-false/imported-true precedence. Kept external enemy conditions independent from the support's infliction prohibition; removed the obsolete Armament limitation receipt and migrated its numerical contract.
- Passed 121 distinct targeted tests: core mapping 77, config resolver 13, support 24, parity 2, WASM support contracts 5. Scoped lint and release WASM build passed. Fixed a test's incorrect linear effective-DPS assumption (positive armour depends on hit size), retaining strict raw-hit/component and ailment assertions.
- Real WASM verified 16 paired cases, including each singleton 1.12, Frozen+Chilled 1.24, all five 1.60 and neutral/panel 1.00. Real UI showed increased damage and no obsolete baseline or candidate-exclusion receipt. Preserved prohibition and group/source boundaries.
- Independent production review approved; resolved its pending contract-doc finding. Updated memo/contract, restored prior Invoker, closed space 39 and removed screenshot. The isolated test build retains Armament plus external Chill.
- This closes the conditional-support milestone, not the broad project goal. No direct Armament Lua-oracle claim, generated-data edit or automatic push.
- Commit: this entry's checkpoint commit (`feat: model Armament conditional attack damage`).