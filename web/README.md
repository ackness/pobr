# PoBR Web

**English** | [简体中文](README.zh-CN.md)

Use [pobr.win](https://pobr.win) (fallback: [pobr-web.pages.dev](https://pobr-web.pages.dev)).
PoBR Web edits and calculates Path of Exile 2 builds in your browser. The app and
WASM/JSON API are in beta and may change.

The React frontend calls `apps/pobr-wasm` through JSON. It does not duplicate the
engine's formulas. [Web contracts](../docs/web-contracts.md) describe the behavior;
[calculation outputs](../docs/calc-outputs.md) define the numbers and explanations.

## Builds and stages

Open **Build library** to create separate builds and copy stages for leveling,
progression or endgame. Each stage saves equipment, passive allocations, skills,
configuration, notes and unapplied editor drafts. Switching saves the current stage.
A source PoB loadout is a selection inside the imported file; switching it reloads
that source into the current stage and warns before discarding edits.

Import a PoB2 code, a supported share link, WeGame data or a `.build` file. Code and
single-session imports add a build by default; **Replace current stage** is explicit.
Supported links include pobb.in, poe.ninja `/pob/` shares, Pastebin, Rentry, poe2db
and Maxroll. A poe.ninja character page is not a build-code link. Planner `.build`
files carry a plan, not full equipment. Check defaults and missing inputs in
[import formats](../docs/import-formats.md) and [WeGame import](../docs/wegame-import.md).

The pasted JSON field accepts recognized `.build`/WeGame sources. Use **Import from
file…** for PoBR sessions, all-stage build files and workspace backups. Rejected
imports leave the current build, stages and item library intact and show recovery
advice beside the error. They do not invalidate an otherwise valid calculation or
compatible completed plans; a real pending or failed calculation still blocks new
passive planning.

**Share this build · all stages** downloads one build with its stages and notes.
Importing it adds an independent build. Unapplied drafts stay in local saves and
full backups; Cancel discards a draft. PoB codes export the current stage and its
source loadouts. **Download backup** includes all local builds and drafts;
restoring it replaces the collection after confirmation.

Delete a build or stage from Build library. The confirmation names what will be
removed and recommends downloading a backup. At least one build and one stage per
build must remain. Deletion opens a neighboring entry and cannot be undone.
Undo/redo keeps up to 50 edits of the current stage; importing or changing stages,
builds or source loadouts starts a new history. Tree edits also have their own
30-step history. Neither history survives a reload.

Saves are local to this browser. Tabs share builds, stages, saved items, jewels
and reusable skill sets. Independent edits merge. If both tabs change the same
stage, the writing tab keeps its version and reports a conflict; an edit survives
a concurrent deletion. Each tab keeps its own selection where possible. This is
browser-tab synchronization, not a cloud backup. Download files to move devices.
See [workspace contracts](../docs/web-contracts.md#web-build-workspace).

## Features

| Page | What you can do |
|---|---|
| Overview `/` | Inspect key values, issues, independent skill rankings and per-source contributions. |
| Build library `/build` | Manage builds/stages, character identity, imports, exports and backups. |
| Skills `/skills` | Edit gems, levels, quality, corruption, supports, forms and weapon bindings; choose minion monsters and skills; plan supports and Full DPS. |
| Items `/items` | Edit exact item text, runes, flasks/charms and slot notes; compare/equip library items; construct hypothetical items or browse unique variants and rolls. |
| Tree `/tree` | Allocate/refund shared or weapon-set nodes, choose attributes, edit jewels, search nodes, inspect a heat map and plan routes. |
| Config `/config` | Manage named configuration sets, enemy assumptions, charges, custom modifier groups and party members. |
| Upgrades `/trade` | Score affixes locally, open official market searches, compare pasted items and plan equipment or gem upgrades. |
| Compare `/compare` | Compare two saved builds/stages and copy an item, skill group or configuration value from B to A. |
| Calcs `/calcs` | Read stat explanations, skill metrics, Full DPS contributions and the per-line usage ledger for the player or minions. |

The app uses site-root routes; there is no separate `/v2` interface or redirect.
Browser Back/Forward restores navigation. The header links the app version to its
source; the results footer identifies the engine and loaded data versions.

### Skills, configuration and calculations

Skill metrics evaluate each enabled active effect independently, including effects
granted by supports and utility skills with zero damage. Rows are not summed.
Selecting one preserves its group, exact effect and weapon binding. Without a saved
main-skill selection, the app chooses the highest combined-DPS effect once; it keeps
an imported selection. Forms are stat sets of an effect, not additional effects.
Flameblast, staged channelled attacks and Incinerate offer the stage inputs the
engine actually reads. Full DPS adds selected sources as if active simultaneously;
it does not simulate a rotation.

Disabling a gem keeps it in saves and exports but removes it from calculation.
Corruption affects modifiers that require it; toggling the checkbox does not change
the gem's level. Unresolved gem slots, including imported level-zero slots, stay in
PoBR saves and backups and are excluded from calculation. PoB export needs a safe
source name; it reports an error if one cannot be recovered rather than dropping a
slot. Named skill/configuration sets, inactive PoB loadouts and disabled custom
modifier blocks round-trip; only enabled custom blocks contribute.

Config starts with applicable options. Imported values remain visible, and unknown
applicability stays visible too. Applicable does not mean implemented. Quick access
links to the inspected hit's mitigation settings without editing values. Player and
minion charge controls are separate. Reset restores the imported typed value or the
catalog default. [Configuration contracts](../docs/web-contracts.md#web-configuration)
cover defaults, filtering and preserved raw inputs.

Stat details load only when opened. They show calculation steps, source queries,
formula inputs or an explicit unavailable state. Player and minion values remain
separate; missing results are not measured zeroes. Pending results may keep dimmed
previous values within the same stage; failed calculations are not valid baselines.

### Upgrade planning and limits

Choose a goal and compare complete recalculations before applying a plan.
Damage goals require a valid inspected skill; Life/EHP goals can work without one.
Recommendations show the baseline's limits and why candidates were excluded.
A gain does not establish complete mechanic coverage. Newly introduced unsupported
effects or invalid results block automatic application where the planner requires it.

- **Market:** local affix scores generate category, level, budget and realm filters.
  Open the official site, sign in and choose listings there. CN uses instant-buy
  stock. Click a listing's **Sum** to sort by score. A partly recognized current
  item retains a minimum based on recognized weighted stats, with a visible warning;
  **Broaden search** removes it. Affix percentages are estimates, not whole-item
  gains, and cannot be added. Debuffs and projectile/radius utility can remain
  unquantified. The app does not automatically fetch listings or prove a best purchase.
- **Pasted items:** compare complete text at compatible occupied slots or allocated
  jewel sockets. The preview shows retained, inherited or custom augments. Apply
  equips exactly the evaluated text. The market-copy userscript copies only a
  clicked listing's item, without seller data.
- **Equipment combinations:** up to 16 placements and 1–10 replacements (default 2).
  Singles and pairs are fully evaluated; larger sets are exhaustive within 512
  evaluations, otherwise bounded. Results show up to five plans plus current gear.
  A copied item cannot be used twice. Final augment limits are checked. The search
  uses supplied candidates, without market prices or a total purchase budget;
  attribute requirements and special equip restrictions still need review.
- **Supports:** bounded complete-set comparisons preserve the inspected effect.
  Set actual unlocked capacity; imported capacity is unknown. Exclusions, family
  conflicts and the conservative one-copy lineage limit apply. Ordinary supports
  are configuration changes; active/lineage gems have separate purchase plans.
  New support limits are excluded, while already-equipped limits remain visible.
- **Passives and jewels:** routes include travel cost and attribute choices; refunds
  protect prerequisites and filled sockets. Joint search accepts up to 8 jewel
  placements, up to 8 points and 512 route evaluations per placement/current jewels.
  It changes one candidate jewel placement per plan on the shared tree. It excludes
  weapon-exclusive routes, simultaneous multiple-jewel swaps and joint equipment/tree
  search. Unresolved seed transformations and unsafe edits stay diagnosed.

Completed compatible support, item and passive plans survive same-stage navigation.
Leaving cancels running work; returning does not restart it. Relevant build/goal
changes invalidate results, and stage changes or reloads reset temporary work.
Heat maps run in cancellable batches of 64 candidates; completed partial results
become stale after an edit. Manual tree budgets assume all 24 campaign reward points.
Historical trees calculate/export as imported; convert explicitly before current-tree
editing. Comparisons use each side's own config by default, optionally A's for both;
applying B to A never edits B. [Web contracts](../docs/web-contracts.md) cover exact
search, application, undo and diagnostic boundaries; [jewel search](../docs/jewel-search.md)
covers Time-Lost jewels and unique-jewel limits. Build-reference guidance remains
hidden from navigation.

## Workspace

**Display settings** offers **Grouped details**, **Compact tables** and **Game cards**,
globally or per page. Resetting page overrides restores the global mode. Old
`itemDisplay` choices migrate to the global setting. Equipment layouts share the
same classified lines and exact source text; unknown affixes keep no guessed tiers.

Settings also offer dark (default), light or system theme, three languages,
system/sans/serif fonts, 14–32px text (default 16px) and comfortable/compact spacing.
Density changes spacing independently of text size. These choices preserve drafts,
selected skills and planner state without recalculating the build. Narrow screens
use navigation/results sheets; wide screens can dock the stat inspector. No external
font service is required. `⌘K`/`Ctrl K` opens commands; `?` outside a text field lists
shortcuts. Up to 12 stats can be pinned to the results panel.

Saved data retains the `pobr-v2-*` local-storage prefix for compatibility. On first
open, legacy `pobr-build-state`, `pobr-library` and `pobr-notes` values are copied once,
with exact originals in `pobr-v2-original-backup`; the marker is written last so
failed migration can retry. The old keys are not written afterwards. A legacy-only
browser keeps `pobr-lang` until preferences are saved. An unreadable save is preserved
for recovery; workspace persistence failures offer a backup download.

## Language support

UI supports English, Simplified Chinese and Traditional Chinese. Game noun dictionaries
are versioned; Simplified Chinese comes from the CN dictionary (`node pipeline/gen-zh-cn.mjs`).
The gem picker searches all three languages. Simplified Chinese item bases and mod
lines are translated through templates to canonical English before parsing; PoB
structural lines such as `Rarity:` keep their format. Unknown text remains visible
with diagnostics. Traditional Chinese has noun dictionaries but no native mod-line
templates; display can fall back through script conversion, not official translations.
Rendering never rewrites imported text or stable calculation ids.

## Quick start

Run from the repository root. Use the pnpm version in `web/package.json`, Node 26
(as in mise/CI), and the Rust toolchain pinned in `rust-toolchain.toml`.

```bash
mise run setup
pnpm --dir web build-wasm
pnpm --dir web sync-data
pnpm --dir web dev
```

`setup` installs prerequisites, including wasm32, wasm-pack and Web dependencies.
Without mise, add `wasm32-unknown-unknown`, install wasm-pack 0.15.0 and run
`pnpm --dir web install --frozen-lockfile` with the pinned tools. Dev serves
`http://localhost:5173`. For UI-only work without WASM/data:

```bash
VITE_POBR_BACKEND=mock pnpm --dir web dev
```

## Development and validation

Follow [scoped validation](../docs/maintainer-guide.md#验证分层). Run relevant Vitest
files and typecheck; use the matching Playwright spec for interactions. Rebuild WASM
when missing or when its Rust sources, dependencies, features or toolchain change.
Sync missing/changed data. E2E requires current production `dist`; `build` includes
typecheck. `build-wasm` reuses verified unchanged output; `--force` rebuilds without
clearing Cargo caches.

| Command | Purpose |
|---|---|
| `pnpm --dir web dev` / `pnpm --dir web preview` | Dev server / preview built dist, both with the HTTP adapter. |
| `pnpm --dir web build` / `pnpm --dir web typecheck` | Production build / types only. |
| `pnpm --dir web test src/lib/skillSelection.test.ts` | Focused unit tests. |
| `pnpm --dir web test:wasm-contract` | Frontend request conversion against prepared real WASM/data. |
| `pnpm --dir web test:worker` | HTTP Worker tests in workerd. |
| `pnpm --dir web exec playwright test e2e/build-roundtrip.spec.ts` | Focused browser regression with current dist/WASM/data. |
| `pnpm --dir web build-wasm` / `pnpm --dir web sync-data` | Prepare WASM / copy versioned data into public. |
| `pnpm --dir web package-wasm` / `pnpm --dir web smoke-wasm-package` | Package / verify the standalone archive in `.cache/wasm-release/`. |
| `pnpm --dir web build-tree-art` | Regenerate tree art after a data/vendor bump; requires vendor, zstd and ImageMagick. |

Seven shared request DTOs are generated from Rust into `src/api/generated/request.ts`
using the pinned host-only `ts-rs` dev dependency. After changing them, run
`cargo test -p pobr-wasm --lib export_typescript_contract -- --ignored` and commit the
output. Ordinary Rust library tests and PR CI check drift. Other types remain in
`src/api/types.ts`, with assertions in `apps/pobr-wasm/tests/contract_golden.rs`.
Regenerate mock fixtures after contract changes with
`cargo test -p pobr-wasm --features diagnostics --test gen_fixtures -- --ignored`.
Mocks support UI work; they do not verify engine behavior.

## Layout and data flow

`src/app/` owns routes, settings and the shell; `src/components/` contains feature
panels and shared shadcn/Radix controls. `src/hooks/useBuildSession.ts` manages edits,
imports and calculations. `src/api/` contains JSON types, generated requests and
WASM/mock backends; `src/lib/` holds helpers, `src/fixtures/` mock data and
`src/styles/` shared styles. Use shared controls and application dialogs for new UI.

The backend normally runs in a dedicated module Worker with one engine and ordered
calls. Startup failure falls back to the page thread; `?engine=main` or build-time
`VITE_POBR_WORKER=0` forces it. Startup streams files from the data manifest, verifies
SHA-256 with the browser, stages bytes and releases copies no longer needed. Without
native digest support or a digest inventory, WASM performs hashing. Content-addressed
`/data/h/*` and `/assets/*` files have immutable caches; missing files return 404,
while page routes receive the app shell. Calculations use the initialized data snapshot.

Ordinary edits request `breakdowns: []`; opening a stat requests one explanation,
and Config requests `config_context` only while open. Attribution removes and
recalculates each source on demand (one baseline plus one calculation per source).
Tree display loads the current or historical tree JSON directly. The Pages Worker
handles restricted import/market HTTP requests and never runs Rust calculations.
See [runtime details](../docs/build-model.md#web-运行时与天赋树规划),
[development workflow](../docs/development-workflow.md) and
[standalone WASM packages](../docs/wasm-package.md).
