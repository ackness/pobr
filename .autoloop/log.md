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