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
