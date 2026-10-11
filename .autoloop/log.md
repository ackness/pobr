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
