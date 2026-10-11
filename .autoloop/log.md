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