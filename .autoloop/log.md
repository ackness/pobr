# Iteration log

## Iteration 1 — import library deduplication

- Baseline: `e791cb1` on `autoloop`; initially clean.
- Plan: inspect recent import/workspace behavior and make one evidenced, bounded improvement.
- Reproduction: importing two identical rings produced two library candidates even though automatic collection intends text deduplication.
- Change: add each accepted text to the existing `seen` set during batch filtering; preserve both equipped slots and distinct rolls. Update the workspace contract.
- Regression: `automatic import collection deduplicates raw text without losing equipped copies or distinct rolls` failed before the fix at its exact-text library assertion, then passed. Also checks reimport identity, per-slot candidate usability and reload persistence.
- Verification: 88 focused Vitest tests; production Web build including TypeScript; 10/10 workspace Playwright tests with real WASM. Browser smoke confirmed two equipped copies and one new library entry. Screenshot API and direct CDP capture timed out; DOM/state evidence only.
- Limits: old/manual duplicates intentionally retained; calculation logic unchanged; existing bundle-size warning not addressed. No controller tool exposed, so no automatic scheduling or checkpoint API call is claimed.
- Checkpoint: this log and the implementation are committed together as `fix: deduplicate items within build imports`; no push.
- Next: inspect legacy missing-slot backfill and alternate-weapon candidate collection, requiring reproduction before another fix.
