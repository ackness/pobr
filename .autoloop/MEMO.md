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

## Priority queue
1. Correct bilingual onboarding for workspace semantics.
2. Establish the local real-WASM browser run and exercise lifecycle acceptance scenarios.
3. Use runtime findings to select the next implementation change; do not invent defects or expand mechanic coverage without parity evidence.
