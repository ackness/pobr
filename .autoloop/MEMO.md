# PoBR autonomous improvement loop

## Goal
Improve PoBR using evidenced product gaps, preserving calculation correctness and player data.

## Focus and authority
- Focus question received no selection; use evidence-led autonomous prioritization.
- Work only in this autoloop worktree. Local commits allowed; do not push.
- No autoloop controller tool is exposed in this session. This is one manually executed iteration; do not claim scheduling is active.
- Respect the repository's existing exclusion of historical local product-loop notes.

## Current milestone
Reliable imports into the build workspace and equipment library.

Exit criteria:
- Invalid inputs preserve the current valid build and usable planning state (existing contracts and regressions).
- Imported equipment remains lossless in its equipped slots; automatic library collection does not add duplicate raw texts within a batch or on reimport.
- Changed behavior is exercised in the real Web/WASM product and focused browser regressions.

## Current evidence
- Real browser reproduction: importing two equipped rings with identical raw text adds two duplicate library entries. Both ring slots must remain equipped; only automatic library collection should deduplicate.
- Fix scope: batch-local text deduplication in `mergeImportedIntoLibrary`; preserve legacy slot backfill and existing user library entries.
- Test origin is `http://127.0.0.1:5186`. Port 5173 had pre-existing browser data and was only inspected, not edited.

## Iteration 1 result
- Added batch-local exact-text deduplication without changing equipped slots, existing records, manual saves, or skill-set collection.
- Added a real-WASM workspace browser regression; it failed on duplicate library text before the fix and passed afterward.
- Verified 88 focused Vitest tests, Web production build (including typecheck), and all 10 workspace E2E tests.
- Real Ego browser smoke confirmed two equipped copies and one newly collected library entry. Screenshot capture timed out through both the high-level API and CDP; no visual artifact is claimed.
- Existing Vite >500 kB chunk warning remains outside this iteration's scope.
- Next: inspect legacy slot backfill and alternate-weapon candidate collection for user-visible gaps before choosing another change. Do not broaden into automatic cleanup of player-owned library entries.
- Milestone remains open; this iteration did not re-run the separate invalid-import/planning suites.
