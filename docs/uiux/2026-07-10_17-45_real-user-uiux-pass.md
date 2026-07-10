# Real-user UI/UX pass

## Goal

Improve the highest-impact usability and responsiveness problems found in the Skills, Tree, and Items workflows while preserving the current WASM contract boundary.

## Scope

- Expose trustworthy structured gem context: type tags, minimum level, attribute tendency, and white-gem colour.
- Make gem search complete and make selected gem level/quality controls self-explanatory.
- Remove pointer-move-driven full-tree renders and guard asynchronous hover previews against stale results.
- Add deterministic shortest-path allocation and disconnected-node cleanup for the normal tree. Alternate starts and jewel-created discrete paths remain deferred.
- Improve item-detail placement, responsive wrapping, text contrast, and tree status feedback.

## Assumptions

- `GemCatalogEntry` remains a lightweight startup catalog; full level-scaled prose needs a separate, authoritative data pipeline.
- Existing allocated nodes are valid roots for pathfinding. Known class-start node IDs provide the root for a new build.
- The normal and selected ascendancy graphs remain separate.

## Risks

- Class-start IDs are inferred from the current PoE2 tree data and are covered by focused graph tests.
- Exact per-node preview is still a full calculation; this pass keeps it off pointer movement and prevents stale UI writes. Moving WASM into a Worker is a later engine-boundary change.
- Support compatibility depends on the whole socket group and requires a dedicated backend endpoint; this pass does not reproduce that logic in TypeScript.

## Steps

1. Extend the Rust/TypeScript gem catalog contract and fixtures.
2. Add structured gem summaries and complete result navigation.
3. Extract/test passive graph path and connectivity helpers, then wire path allocation into the tree.
4. Decouple tooltip placement from pointer movement and harden preview caching/generation checks.
5. Refine item/tree responsive layout and accessibility labels.

## Validation

- `pnpm typecheck`
- `pnpm test`
- `cargo check -p pobr-wasm --features wasm`
- `cargo nextest run -p pobr-wasm --test contract_golden`
- Browser interaction review when an in-app browser instance is available.

## Rollback

Revert the isolated `codex/uiux-real-user-20260710` branch or revert individual web/API commits; no persisted data format changes are introduced.

## Outcome (2026-07-10 verification pass)

All five steps were implemented and verified in-browser (mock + real WASM backend,
imported real poe.ninja builds from `examples/poe-ninja/`). Validation: `pnpm typecheck`,
`pnpm test` (22), `cargo check -p pobr-wasm --features wasm`, `contract_golden` (14/14),
`cargo fmt --check`, `clippy -D warnings`.

Issues found and fixed during verification:

- `web/src/fixtures/*` were stale — regenerated via `gen_fixtures` so the mock backend
  carries the new gem-catalog fields (and current decode contract); removed the now-dead
  fallback mapping in `mockBackend.ts`.
- Tree tooltip never dismissed: node `onPointerLeave` was dropped when the tooltip became
  interactive. Fixed with a 150 ms delayed clear cancelled by tooltip pointer-enter, plus
  Escape to close.
- **Deallocation destroyed imported builds** (142 → 9 nodes): real builds are not fully
  root-connected in the modelled graph (class-start attach edges missing, weapon-set
  points), so `connectedAllocation` from the class root swept almost everything.
  `deallocateNode` now cascades only when the model explains the entire allocation and
  falls back to single-node removal otherwise; an allocated root is also kept in the
  connected set.
- Gem tags were raw engine SkillTypes (`AreaSpell`, 177-word vocabulary) and untranslated:
  added a curated, priority-ordered whitelist with en/zh-TW/zh-CN labels
  (`web/src/lib/gemTags.ts`); search matches the raw word and every locale label, and gem
  name search already indexes all three languages regardless of UI language.
- Node `aria-label` now strips `[a|b]` markup (shared `stripMarkup` helper).

Known non-goals kept: exact hover preview stays click-triggered (Worker migration later);
support-compat filtering still needs a backend endpoint; alternate starts / jewel paths
remain deferred.
