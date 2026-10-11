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

## Iteration 2 — legacy library recovery coverage

- Confirmed iteration 1 is committed as `c9340ed`; worktree started clean.
- Exercised the next legacy-slot boundary with real WASM: an entry without a slot is initially excluded from slot-filtered candidates; importing matching equipment restores eligibility while preserving its ID and custom name.
- Added a regression covering recovery, preservation of an existing manual duplicate and its slot, actual Equip into the other ring position, and reload persistence.
- Result: existing production behavior is correct for this scenario. No runtime change or new bug fix claimed; value is permanent coverage of the player-data preservation boundary.
- Verification: Web typecheck and the new targeted Playwright scenario passed (1/1). Reused unchanged production dist/WASM from iteration 1.
- Commit: `test: cover legacy library recovery on import`; no push.
- Next: alternate-weapon candidate collection remains unexamined. Milestone stays open.

## Iteration 3 — alternate weapon candidates

- Started clean at `6bf0d08`. Exercised real PoB XML import containing distinct primary/alternate bows, a primary quiver and an empty alternate offhand.
- Added behavioral coverage proving both bows are available as main-hand candidates, the quiver is excluded there, and equipping the alternate bow only changes the active pair.
- Switching to set 2 preserves the empty offhand rather than inheriting set 1's quiver; active set and equipment survive reload.
- Existing runtime behavior passes this scenario; no production defect or runtime fix claimed.
- Verification: Web typecheck and targeted real-WASM Playwright scenario passed (1/1), using unchanged production dist.
- Commit: `test: cover alternate weapon library candidates`; no push.
- Next: stop adding adjacent coverage without a demonstrated gap. Re-evaluate product priorities through a fresh player workflow; identical dual-wield texts across different hand roles remain outside this scenario's evidence.
