# Product improvement loop

## Standing scope
- Work only in the assigned loop worktree and branch; commit only, never push.
- Focus: auto. The initial focus question received no selection.

## Long-term goal
Make PoBR a trustworthy browser workspace for comparing PoE2 upgrades: preserve player work across imports and stages, keep recommendation limitations visible, and make the next action understandable.

## First milestone: trustworthy build-workspace lifecycle
Exit criteria:
1. Player-facing English and Chinese onboarding accurately distinguishes adding builds, replacing a stage, sharing a build and restoring a workspace backup.
2. Exercise actual import, stage switching, reload and backup/restore with real WASM; investigate any observed loss of notes, drafts, selection or planning context.
3. Address evidenced lifecycle defects with focused regressions, retaining existing calculation and unsupported-effect boundaries.
4. Review import/planning feedback for actionable errors without discarding prior results.

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

## Operational notes
- Driver state reports PR publication failure resolving base branch `autoloop-base`. No publishing commands were run; repair remains outside commit-only scope.
- Browser confirmation handling worked when upload and acceptDialog occurred in one ego-browser invocation; splitting across rounds did not restore. Reported tooling discrepancy, not an application defect.

## Priority queue
1. Improve the observed raw import-error feedback with localized, actionable recovery guidance while retaining the original engine diagnostic and preserving valid plans.
2. Verify that feedback for pasted PoBR saves versus external build input explains the correct import path without changing accepted formats.
3. Then close the lifecycle milestone and select the next product gap from actual upgrade-comparison use.
