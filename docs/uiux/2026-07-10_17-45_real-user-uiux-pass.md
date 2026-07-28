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

## Follow-up (2026-07-11): negative Spirit Unreserved discrepancy

The imported build showed `SpiritUnreserved = -205` while the current vendored PoB2
computes **-305** (the ninja export's -197 comes from an older PoB2 version; negative is
the build's true over-reserved state in every implementation). PoBR was missing one
Trinity reservation (100): the amulet-granted skill (`<Skill source="Item:3:Soul Torc">`)
was deduplicated against the manual socket group by the reservation loop's `seen` set.

Fix: `SocketGroup` now carries the PoB `source` attribute, and the reservation dedup key
is `(is_granted_group, skill_id)` — duplicate manual groups still count once (the
coiling/gemling behaviour the dedup was added for), while item/tree-granted instances
reserve separately, matching the vendor per-active-skill loop (oracle
`spiritReservedBreakdown` shows both Trinity entries at 100).

Verified: skills suite 41/41 (new `item_granted_group_reserves_separately_from_manual_group`),
parity 58/58 (`parity_no_regression` green), dualrun green.

Threading the fix to the web surfaced two more contract gaps, both fixed:

- The decode/calculate contract dropped `source`, so the web's request path rebuilt all
  groups as manual and re-deduplicated the granted Trinity. `source` now flows
  decode JSON → web state → calculate request → `SocketGroup`, and share-code encode
  writes the attribute back (round-trip fidelity).
- Share-code export stamped explicit `boolean="false"` for every unset
  `defaultState=true` config key, silently renouncing the quest rewards
  (+100 Spirit, +20 Life, resistances…) on re-import — an exported build lost 100 Spirit.
  Encode now writes only explicit values (PoB2 behaviour), and the scratch-build request
  path applies the same defaults via `apply_default_on_config` (a fresh PoBR build now
  starts with quest bonuses granted, like a fresh PoB2 build). Two contract tests updated
  for the new baseline (+50 flat Life ring now yields 52.5 with the quest 5% inc).

End-to-end (browser, real WASM): import `examples/poe-ninja/1.txt` → SpiritUnreserved
-305; generate share code → re-import → still -305. Contract golden 14/14 green.
