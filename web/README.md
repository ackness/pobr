# PoBR Web

**English** | [简体中文](README.zh-CN.md)

> **⚠️ Beta.** The app and the underlying wasm/JSON API are still evolving
> and may change or break without notice.

A PoB2-style web frontend for PoBR. **Fully decoupled from the calculation
engine**: it only consumes the JSON contract of `apps/pobr-wasm`
(`web/src/api/types.ts` ↔ `apps/pobr-wasm/src/build_api/`, shapes pinned by
`apps/pobr-wasm/tests/contract_golden.rs` on the Rust side) — no Rust type imports, no
formula duplication.

## Builds and stages

Use **Manage builds** to keep separate builds for different skills. Copy the current
stage for leveling, progression or endgame; each stage preserves its own passive
allocation, equipment, skills, configuration and notes. Switching saves the current
stage, including unapplied item drafts. Existing single saves migrate automatically.
Code and single-session imports create a new build by default. To update a stage,
select **Replace current stage** and confirm. **Imported PoB loadouts** live inside
the current build's stage section; switching them reloads the source file into
that stage, with a warning before discarding edits.

**Share this build · all stages** downloads a PoBR JSON file with every stage and
its notes. Importing it adds an independent build without replacing existing ones.
Unapplied drafts stay private to local storage and full backups. PoB share codes
continue to export the current stage and its imported PoB loadouts.
**Download backup** under **Back up all builds** includes the entire local build library;
restoring a full backup replaces it after confirmation. Saves are local to this
browser; use files to share or move devices. Storage failures appear beside the
current-build strip so you can download a backup before closing the page.

Custom modifiers keep ordered groups, titles, raw text and enabled state in saves
and PoB exports. Only enabled groups affect calculations. Unchanged legacy saves
recover groups from their source code; edited flat legacy modifiers keep their
saved text because the original grouping cannot be inferred reliably.

The header shows the app release and successfully loaded data version. On Upgrades,
cancelled all-position analysis can resume completed work while the build and goal
stay unchanged. Budget/realm/league edits only change market links. Leaving the
panel or changing build, goal or category discards those temporary results.

## Quick start

Run these commands from the repository root. Use the pnpm version pinned in
`web/package.json`; local mise selects Node 26 and CI uses Node 22. Rust is not
version-pinned in this repository; reuse an installed stable toolchain.
Skip tool installation when already configured.

```bash
# one-time prerequisites (repo root)
rustup target add wasm32-unknown-unknown
cargo install wasm-pack

pnpm --dir web install --frozen-lockfile
pnpm --dir web build-wasm    # build or reuse verified src/wasm/pkg/ (gitignored)
pnpm --dir web sync-data     # data/<version>/ JSON → public/data/ (gitignored)
pnpm --dir web dev           # http://localhost:5173
```

Without wasm / data you can develop the UI standalone against the mock
backend:

```bash
VITE_POBR_BACKEND=mock pnpm --dir web dev
```

Mock fixtures are generated from the real contract (re-run and commit after
contract changes):

```bash
cargo test -p pobr-wasm --test gen_fixtures -- --ignored
```

## Development and validation

For standalone engine downloads, Browser/Node.js integration, the agent skill
and the Release CI size comparison, see [WASM packages](../docs/wasm-package.md).

Follow the change-scoped validation policy in [CLAUDE.md](../CLAUDE.md).
Run the relevant Vitest files and typecheck; add the matching Playwright spec
for interaction changes. Rebuild WASM if missing or after changes to its Rust
sources, dependencies, features, or toolchain. Sync data when source data changes
or prepared data is missing. E2E uses production dist, so build current Web code
first. `build` already includes typecheck.

`build-wasm` skips wasm-pack when sources, toolchain, build settings and verified
bindings are unchanged. Use `pnpm --dir web build-wasm --force` for an explicit
rebuild without clearing Cargo caches. See [development workflow](../docs/development-workflow.md)
for storage reporting, the shared dev/test profile and cache tradeoffs.

`src/api/wasmBackend.ts` invokes WASM in the browser; planners reuse calculation
results. `public/_worker.js` is the Cloudflare Pages HTTP adapter for WeGame and
market endpoints, also used by Vite locally. It does not execute Rust
calculations. Validate Worker changes in the actual workerd runtime.

## Commands

| Command | Description |
|---------|-------------|
| `pnpm --dir web dev` | Vite dev server |
| `pnpm --dir web build` | tsc + production build (dist/) |
| `pnpm --dir web typecheck` | type check only |
| `pnpm --dir web test src/lib/mainSkill.test.ts` | selected Vitest unit tests |
| `pnpm --dir web test:worker` | Worker tests in the actual workerd runtime |
| `pnpm --dir web exec playwright test e2e/build-roundtrip.spec.ts` | Selected E2E spec; requires current dist and prepared WASM/data |
| `pnpm --dir web build-wasm` | build or reuse verified WASM; `--force` rebuilds |
| `pnpm --dir web package-wasm` | package prepared WASM/data and report size in `.cache/wasm-release/` |
| `pnpm --dir web smoke-wasm-package` | extract and calculate using the standalone archive |
| `pnpm --dir web sync-data` | re-sync game data into public/ |
| `pnpm --dir web build-tree-art` | regenerate committed PoB2 tree node icons/frames in public/tree-art/ from vendor DDS — only after a data/vendor bump (needs vendor checkout + zstd + ImageMagick) |

## Layout

```
web/src/
├── api/          # single backend entry: types.ts (contract) + wasm/mock backends
├── hooks/        # useBuildSession (import / recalc / attribution orchestration)
├── components/   # by feature: shell/import/sidebar/items/skills/calcs/tree/config/trade/guidance/shared
├── lib/          # statDisplay, i18n, trade, optimize, annotations, …
├── fixtures/     # mock backend data (generated by gen_fixtures)
└── styles/       # tokens.css (design variables) + global.css
```

## Language support

- **UI**: en-US / Traditional Chinese / Simplified Chinese (top-bar switcher;
  UI strings live in `src/lib/i18n.ts`).
- **Game nouns**: both zh-TW and zh-CN sidecars are committed (Simplified
  Chinese transcribed from the CN-realm dictionary; regenerate with
  `node pipeline/gen-zh-cn.mjs`); the gem picker searches all three.
- **Chinese item input**: item mod lines and base names may be entered
  directly in Simplified Chinese (CN-realm text); the wasm side
  reverse-translates them to English canonical via templates before the
  existing parser. Structural lines (`Rarity:`) stay in PoB format; unknown
  Chinese lines retain their original text and produce unsupported diagnostics,
  just like unknown English lines; unmodeled effects are excluded from results.
  zh-TW only has noun sidecars (no mod-line templates).

## Features

- **New build** (PoB2 semantics): a default character exists on startup —
  pick class/ascendancy/level on the Build tab, allocate on the Tree tab,
  everything recalculates live without any code.
- **One-shot import**: paste a PoB2 build code or a WeGame PoE2 share URL on the Build tab to replace
  everything (items / skills / tree / config); the editing state can be
  exported back to a shareable PoB2 code.
  See [WeGame import](../docs/wegame-import.md) for deployment and calculation limits.
- **Items**: paper-doll slot layout with full PoB-text editing, rune sockets,
  flask/charm slots, an item library with compare/equip, and per-slot notes.
- **Skills**: socket-group editing (gems, levels, quality, supports),
  main-skill selection with a live damage breakdown.
- **Tree**: allocation, attribute picks, jewel sockets, search, and a
  node-power heatmap.
- **Optimizers & trade**: objective-driven gem/item/tree suggestions and a
  category-constrained official market links with local affix scores and budget filters.

## Trade upgrade planning

On the Trade tab, choose a position, goal, budget and market. The current item's type is detected automatically; you do not need to select a base. Equipment, accessories, allocated passive jewels, flasks/charms and skill/support gems have dedicated positions.

1. **Calculate affix scores** to see useful stats, relative priority, estimated contribution and market weights. The visible skill selector determines which skill is scored.
2. **Browse matching items** opens the official market with category, budget, character level and weights already filled in. CN uses instant-buy stock. Sign in and select items there; no JSON export or import is needed.
3. Click a listing's **Sum** on the official site to sort by weighted score. If too few items match, enable **Broaden search** to remove the score threshold.

Affix estimates average probes on a reference item and current gear. They describe a stat's contribution, not the gain from buying an entire replacement; percentages cannot be added together. Gem cards show evaluated level/quality plans using PoB2 character-level requirements. Unsupported mechanics remain visible. Market weights guide selection and cannot guarantee the best purchase or account for every interaction.

Changing the build, skill or goal clears scores. Changing budget, server or league updates search links immediately without repeating calculations.

### Compare equipment combinations

After comparing a pasted item, choose a destination and **Add to combinations**.
Add other items the same way, then choose **Maximum replacements** (1–10, default 2).
The planner fully recalculates keeping the current equipment, every single replacement, and
every pair within the selected limit, using different slots and different copied items. A ring can be added
at both positions to compare placements, but the same copied item is never equipped
twice in one plan. Up to 16 candidate placements are retained while the panel is open.

The results show up to five alternatives plus the current equipment. Individual
losses are not pruned before evaluating pairs. Larger combinations are exhaustive
when they fit within 512 evaluations; otherwise a goal-guided search reserves evaluations
for each reachable combination size and reports the limit. The shared goal and constraints rank
the complete results; changing the goal re-ranks one-/two-item searches and invalidates larger searches, while changing the build,
weapon context or candidate list invalidates them. **Apply equipment plan** commits
all evaluated replacement texts together, preserving other equipment and the
inactive weapon set. Prepared rune/socket choices are retained exactly and equipped
augment limits are checked against each final combination.

Plans with calculation errors, unsupported effects, uncertain augment limits or
unmet goal constraints cannot be applied from this planner. This compares only the
supplied candidates and destinations, without market prices or a total purchase
budget. Attribute requirements and special equip restrictions still need player
review. Flask and charm candidates keep their separate comparison flows.

### Compare jewels with passive routes

Expand **Plan passive upgrades** on the Tree tab. Under **Jewels and passive routes**,
paste a jewel and add one or more socket placements. The search compares keeping
current jewels with each candidate placement, jointly evaluating connected passive
routes using the selected goal, travel attributes and up-to-8-point budget. Reaching
an empty socket costs points. Refund mode counts points disconnected by replacing
an allocation jewel toward its refund budget and retains the original total point limit.

The calculation engine supplies each candidate's topology for the selected tree version.
Up to 8 placements are accepted, with 512 route evaluations per placement and for the
current jewels, followed by exact final-state checks. New unsupported effects,
unresolved seed nodes and unsafe allocations are excluded with diagnostics.
**Apply jewel and passive plan** writes the exact jewels, nodes and attribute choices
in one edit; build, goal or budget changes invalidate previews. This bounded search
handles one candidate jewel placement per plan on the shared tree; weapon-exclusive
passives, simultaneous multiple-jewel swaps and equipment-plus-tree searches are excluded.

## Data flow

1. Startup: JS fetches every JSON listed in `public/data/manifest.json` →
   `stageDataFile` injects into wasm → `initStagedData()` builds `BuildData`
   over in-memory `GameData`; subsequent calculations read that memory.
2. Import: `decodeBuildJson(code)` → structured build (character / tree / item
   text / socket groups / config).
3. Calculation: `useBuildSession` materializes editable state into a request →
   `calculateBuildJson(request)` → full
   display-catalog key-values + unsupported mod lines + aggregated stat
   breakdowns.
4. Attribution: `attributionJson(request)` → per-source
   remove-and-recalculate marginal contributions (click-triggered; cost =
   1 + number of sources).
5. Passive tree: `public/data/<version>/base/passive_tree.json` is loaded
   statically (not through wasm).

时迭珠宝搜索的使用方式、PoB2 对照及传奇珠宝支持边界见 [珠宝搜索](../docs/jewel-search.md)。
