# PoBR Web

**English** | [简体中文](README.zh-CN.md)

> **⚠️ Beta.** The app and the underlying wasm/JSON API are still evolving
> and may change or break without notice.

A PoB2-style web frontend for PoBR. **Fully decoupled from the calculation
engine**: it only consumes the JSON contract of `apps/pobr-wasm`
(`web/src/api/types.ts` ↔ `apps/pobr-wasm/src/build_api/`, shapes pinned by
`apps/pobr-wasm/tests/contract_golden.rs` on the Rust side) — no Rust type imports, no
formula duplication.

Seven shared request DTOs are generated from Rust into
`src/api/generated/request.ts` by the pinned, host-only `ts-rs` dev dependency.
After changing those DTOs, run
`cargo test -p pobr-wasm --lib export_typescript_contract -- --ignored` from the
repository root and commit the result. The ordinary Rust library tests and PR CI
check for drift without rewriting files. Remaining response/view types stay in
`src/api/types.ts` and retain their Rust contract assertions.
`pnpm --dir web test:wasm-contract` uses the real frontend materialization and
request conversion against built WASM; prepare current WASM and synced data first.
Full CI runs this regression after those preparation steps.

## Builds and stages

Use **Manage builds** to keep separate builds for different skills. Copy the current
stage for leveling, progression or endgame; each stage preserves its own passive
allocation, equipment, skills, configuration and notes. Switching saves the current
stage, including unapplied item drafts. Existing single saves migrate automatically.
Code and single-session imports create a new build by default. To update a stage,
select **Replace current stage** and confirm. **Imported PoB loadouts** live inside
the current build's stage section; switching them reloads the source file into
that stage, with a warning before discarding edits.

The pasted JSON input accepts recognized China-server `.build` and WeGame
sources, not arbitrary JSON or PoBR saved-session envelopes. A legacy `.build`
must include typed `passives`, `items` and `skills` arrays, even when empty.
Rejected imports report an error without changing saved builds, stages or the
item library. The unchanged valid calculation remains available for fresh passive
and joint-jewel planning; compatible completed plans and their settings remain
usable. Import feedback does not clear a genuine calculation failure: failed or
pending calculations still block fresh passive planning, even when older stats
remain visible. Use **Import from file…** for PoBR build files and backups.
After a rejected paste, localized recovery advice appears beside the input while
the original error remains available. Recognized PoBR save/backup envelopes point
to file import and warn about full-workspace replacement; other inputs point back
to the complete supported source. Editing the input or starting another import
clears the advice. This guidance does not broaden accepted formats.

**Share this build · all stages** downloads a PoBR JSON file with every stage and
its notes. Importing it adds an independent build without replacing existing ones.
Unapplied drafts stay private to local storage and full backups. The editor's
draft notice distinguishes this persistence from applying changes to calculations
and shared builds. Cancel discards the draft.
PoB share codes continue to export the current stage and its imported PoB loadouts.
**Download backup** under **Back up all builds** includes the entire local build library;
restoring a full backup replaces it after confirmation. Saves are local to this
browser; use files to share or move devices. Storage failures appear beside the
current-build strip so you can download a backup before closing the page.

Imported gem slots without a stable skill ID remain in local saves, all-stage
build files and backups, including imported level-zero slots. Skills shows their
raw source name with **Unresolved gem**; they are excluded from the stage calculation.
PoB share codes retain named unresolved slots at their current positions and
preserve their edited level, quality and stat set. Legacy saves recover a missing
source name only when the original slot can be matched unambiguously. If no name
can be recovered safely, PoB export reports an error rather than dropping the
slot or inventing its identity; PoBR JSON sharing and backups remain lossless.

Whether a gem is corrupted is kept with the gem through import, saves, stage
calculation and PoB export, because modifiers such as "+1 to Level of all
Corrupted Skill Gems" test it. A gem's level already includes what corruption
did to it. Skills shows the state as a checkbox beside level and quality;
toggling it does not change the level.

Custom modifiers keep ordered groups, titles, raw text and enabled state in saves
and PoB exports. Only enabled groups affect calculations. Unchanged legacy saves
recover groups from their source code; edited flat legacy modifiers keep their
saved text because the original grouping cannot be inferred reliably.

**Calcs → Skill metrics** (also linked from the sidebar) lists every enabled active
effect independently, including support-granted effects and zero-damage utility.
A build without a selected skill defaults, once, to the effect with the highest
combined DPS; that row is marked, and a selection carried by an import is kept. Rows show combined DPS, average hit, action
rate, mana/life costs and cooldown; unavailable rows show errors instead of zero,
and unsupported modifiers and item issues remain visible. Values are not summed.
Selecting a row atomically selects its original group and exact 1-based active
effect ordinal for inspection and upgrade comparisons, using its weapon set.
The ordinal includes support-granted effects, not display aliases or stat sets.
Removing a gem preserves any remaining inspected effect; support planning keeps
the same effect across reordered supports and excludes sets that remove it.
Skill selectors show each group's saved active effect, including secondary effects.
Damage-based upgrade analysis requires an explicit, valid inspected skill; Life/EHP-only
analysis remains available without one. Opening support planning explicitly selects
its group. Applied gem recommendations retain the exact ordinal used for evaluation.
Baseline stats and breakdowns are published only for the current build revision.
Overview row controls retain keyboard focus while refreshing; old metric values and
diagnostics are hidden and selection controls are temporarily unavailable.

The header shows the app release and successfully loaded data version. On Upgrades,
completed affix analyses and position selections survive page navigation within
the current stage. Leaving pauses active scoring; cancelled all-position analysis
can resume completed positions while the build and goal stay unchanged.
Budget/realm/league edits only change market links. Build, goal, category or stage
changes discard temporary analysis results; reloading also starts fresh.
Complete-item text, selected position, augment choices, completed comparisons and
equipment candidates also survive same-stage navigation. Leaving cancels their
work and paste debounce; returning does not restart scoring. Changed calculation
inputs invalidate evaluated results; switching stages resets the workspace.
Applying a complete-item comparison shows a localized confirmation naming the
item and destination after that evaluated text is present in the active weapon
set and pending work has settled. This confirms the equipment edit, not complete
mechanic coverage. Editing or repasting the candidate, starting a new comparison
or leaving the panel clears the confirmation.

On Config, **Quick access** explains optional shock state/magnitude assumptions and
links to enemy mitigation settings matching the inspected skill's non-zero
hit damage. **Open setting** clears catalog filters and focuses the original
control without changing the build. The full catalog, search, configured-only
filter and advanced imported inputs remain available. This hit-only shortlist
does not establish DoT/minion relevance, mechanic coverage or a guaranteed gain;
configured does not mean effective. Reset restores the imported value, or the
catalog default when no value was imported.

Config opens on **Applicable to this build**, filtered by the skills, conditions
and modifiers the current calculation reads; **Show all options** lists the whole
catalog. Imported or saved values always stay visible, marked when they do not
apply, and an option whose relevance cannot be decided stays visible. Rows show
the catalog's help text with the raw key beside it. Applicable does not mean the
option is modelled. The extra calculation context is requested only while Config
is open.

The Power/Frenzy/Endurance count controls distinguish **Player** from **Minions**
in visible labels, accessible input/reset names and localized search. This is
actor identity, not a claim that all minion charge effects are calculated.
Both actors remain available in the full catalog with their original save/export
keys. An untouched imported numeric string remains a string; a deliberate numeric
edit changes only the selected actor's setting, and Reset restores its imported
value without resetting the other actor.

## Quick start

Run these commands from the repository root. Use the pnpm version pinned in
`web/package.json`; local mise and CI both use Node 26. `rust-toolchain.toml` pins the Rust
version. Skip tool installation when already configured.

```bash
# one-time prerequisites (repo root): the pinned Rust + wasm32 target, wasm-pack,
# Node/pnpm and the Web dependencies (`pnpm --dir web install --frozen-lockfile`)
mise run setup
# Without mise: `rustup target add wasm32-unknown-unknown`, install wasm-pack 0.15.0,
# then `pnpm --dir web install --frozen-lockfile`.

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
cargo test -p pobr-wasm --features diagnostics --test gen_fixtures -- --ignored
```

## Development and validation

For standalone engine downloads, Browser/Node.js integration, the agent skill
and the Release CI size comparison, see [WASM packages](../docs/wasm-package.md).

Follow the change-scoped validation policy in [maintainer guide](../docs/maintainer-guide.md).
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
results. By default it runs in a dedicated module worker (`wasmWorker.ts`, reached
through `workerBackend.ts` and the message protocol in `rpc.ts`), so a calculation,
a heat map or an optimization never freezes the interface. If the worker cannot start
the engine runs on the page's thread instead. `?engine=main` in the page address or
`VITE_POBR_WORKER=0` at build time forces that, for debugging a worker-only problem.
Calls are answered in the order they arrive; there is one engine and one state. `public/_worker.js` is the Cloudflare Pages HTTP adapter for WeGame and
market endpoints, also used by Vite locally. It does not execute Rust
calculations. Validate Worker changes in the actual workerd runtime.

## Commands

| Command | Description |
|---------|-------------|
| `pnpm --dir web dev` | Vite dev server |
| `pnpm --dir web build` | tsc + production build (dist/) |
| `pnpm --dir web typecheck` | type check only |
| `pnpm --dir web test src/lib/skillSelection.test.ts` | selected Vitest unit tests |
| `pnpm --dir web test:wasm-contract` | real request conversion against current WASM/data |
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
- **One-shot import**: paste a PoB2 build code or a WeGame PoE2 share URL on the Build tab to add
  a build with its items, skills, tree and config (or replace the current stage on request); the editing state can be
  exported back to a shareable PoB2 code.
  See [WeGame import](../docs/wegame-import.md) for deployment and calculation limits.
- **Items**: paper-doll slot layout with full PoB-text editing, rune sockets,
  flask/charm slots, an item library with compare/equip, and per-slot notes.
  **Craft item** builds a hypothetical item from a game base with legal affix
  tiers and rolls; **Unique catalogue** searches every unique with its versions,
  variants and roll ranges. Both preview against the equipped item (empty slots
  included) and save to the library without changing the build until you apply.
- **Skills**: socket-group editing (gems, levels, quality, corruption, supports),
  main-skill selection with a live damage breakdown. Choose an effect's form
  (stat set) and, for Flameblast, staged channelled attacks and Incinerate, its
  stages; disable a gem without removing it; pick a Companion's or Spectre's
  monster and the skill it uses; include groups in Full DPS with group and gem
  counts. Full DPS adds the included skills as if used at once, not a rotation.
- **Tree**: allocation, attribute picks, jewel sockets, search, and a
  node-power heatmap. Search and choose **Next** to focus a node-details panel
  with its current modifiers and allocation state. Searching, cycling, closing
  details and changing an Attribute draft do not edit the build. Use **Allocate**,
  **Refund** or **Apply choice** explicitly; jewel sockets open the existing editor.
  Ordinary allocation follows the existing shortest path and leaves travel
  Attributes unassigned. Escape closes details and returns focus to search controls.
  Allocate in **Shared**, **Weapon set 1** or **Weapon set 2** mode: every edit
  follows a legal path, previews what it allocates, promotes or refunds, and shows
  the per-set and total point budgets (planned for all 24 campaign points).
- **Compare**: pick any two saved builds or stages as A and B to see stat
  differences, item text, skills, configuration and both passive trees on one
  canvas, then copy a single item, skill group or configuration value from B to A.
  B and the current selection never change.
- **Calcs**: the skill overview, Full DPS contributions, and an explanation for
  each stat (steps, sources or the formula's inputs), loaded when you open it;
  switch between the character and each minion.
- **Optimizers & trade**: objective-driven gem/item/tree suggestions and a
  category-constrained official market links with local affix scores and budget filters.

The selected support planner retains its completed recommendations, socket capacity,
lineage choice and filters while you visit other pages in the same build stage.
Leaving Skills cancels active analysis; returning does not restart it automatically.
Build or goal edits invalidate recommendations, and **Apply** equips only the
evaluated complete support set. Selecting another group, replacing a skill set,
deleting a group or switching stages resets the selected workspace; it is not a
per-group or reload-persistent cache.

Support results disclose verbatim engine-reported baseline limits and identify
excluded complete support sets with their new limits, raw errors and invalid
statistics. Excluded sets are separate from manual filters and recommendations
and have no Apply action. Recommendations show evaluated gem level/quality and
stat-set choices. A gain does not repair baseline limits; missing diagnostics
do not establish that every support mechanic is modeled.

Normal calculations disclose confirmed skipped conditional/scaling numerical
support effects from the compatible, selected payload. Receipts identify the
support effect/set, stat/value and calculated host/form; trigger-source limits
retain their own provenance. For example, Fork's unmodeled forked-projectile
penalty is disclosed, not applied to every primary hit. Zero, unselected,
incompatible and separately handled metadata/control effects are not blanket
rejections. This bounded diagnostic path is not a complete coverage audit.

Passive-route and joint-jewel planning likewise retain compatible completed
receipts and constraints in the current stage. Leaving unmounts the tree canvas
and stops tracking, scoring, hover/keyboard and heatmap work. Input changes
invalidate incompatible results; stages reset ownership. Route receipts disclose
baseline limits and pair excluded complete allocation/refund edits with exact
new reasons or errors.

## Trade upgrade planning

On the Upgrades tab, choose a position, goal, budget and market. The current item's type is detected automatically; you do not need to select a base. Equipment, accessories, allocated passive jewels, flasks/charms and skill/support gems have dedicated positions.

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
twice in one plan. Up to 16 candidate placements survive page navigation within the current stage.

The results show up to five alternatives plus the current equipment. Individual
losses are not pruned before evaluating pairs. Larger combinations are exhaustive
when they fit within 512 evaluations; otherwise a goal-guided search reserves evaluations
for each reachable combination size and reports the limit. The shared goal and constraints rank
the complete results. Changing the goal invalidates evaluated results at every
replacement limit; recalculate under the new goal. Build, weapon context or candidate
changes also invalidate results. **Apply equipment plan** commits
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

1. Startup: JS fetches every JSON listed in `public/data/manifest.json` as bytes →
   `stageDataBytes` injects into wasm → the page compares each file the snapshot
   manifest lists with its SHA-256 digest using the browser's native digest
   (`api/dataIntegrity.ts`; hashing the same 57 MB inside wasm took about 180 ms) →
   `initStagedDataVerified()` builds `BuildData` over in-memory `GameData`, still
   validating everything but the byte comparison; subsequent calculations read that
   memory. Where the browser has no native digest (not a secure context) or the
   manifest has no digest inventory, `initStagedData()` hashes inside wasm instead.
   `stageDataFile` (text) and `initStagedData()` remain for other callers.
2. Import: `decodeBuildJson(code)` → structured build (character / tree / item
   text / socket groups / config).
3. Calculation: `useBuildSession` materializes editable state into a request →
   `calculateBuildJson(request)` → full
   display-catalog key-values + unsupported mod lines + created minions and
   per-stat explanation kinds. Ordinary recalculation sends `breakdowns: []`;
   opening a stat requests that one explanation for the player or a minion, and
   Config adds `config_context: true` while it is open.
4. Attribution: `attributionJson(request)` → per-source
   remove-and-recalculate marginal contributions (click-triggered; cost =
   1 + number of sources).
5. Passive tree: `public/data/<version>/base/passive_tree.json`, or
   `base/passive_trees/<tree>.json` for a build on an older tree, is loaded
   statically (not through wasm).

For Time-Lost jewel search, the PoB2 comparison and unique jewel limits, see
[jewel search](../docs/jewel-search.md) (Chinese).

## Workspace

The app is a Radix-based shadcn/ui workspace. Every page has its own URL: `/`
(overview), `/build`, `/skills`, `/items`, `/tree`, `/config`, `/trade`,
`/compare` and `/calcs`. Browser Back/Forward restores navigation.

It defaults to a dark stone/gold theme. **Display settings** offers grouped details, compact tables and game cards globally,
with optional overrides per page. It also offers light
and system themes, three language choices, local system/sans/serif font stacks,
14–32px base text (16px by default), and comfortable/compact spacing density. Previous equipment display choices migrate
to the global mode. These
preferences do not reset drafts or calculations. Narrow screens use a navigation
sheet and a separate stats sheet; wide screens expand the working area.
No external font service is required.

Saved data lives in `pobr-v2-*` local-storage keys. The prefix predates the
current names and is kept so that existing saves stay readable
(`web/src/lib/sessionStorage.ts` is the one place that spells it). A browser that
still holds the saves of the first interface (`pobr-build-state`, `pobr-library`,
`pobr-notes`) gets them copied once on first open; the exact original strings are
also retained in `pobr-v2-original-backup`. The completion marker is written last,
so a storage failure can retry from the snapshot, and the first interface's keys
are never written afterwards. A browser that only used the first interface also
keeps the language chosen there (`pobr-lang`) until display preferences are
saved. A storage failure blocks migration or offers a backup download; it must
not silently replace an unreadable save.

The feature panels keep their session and planner semantics. `UiProvider` carries
the language, shared control adapters wrap the Radix components, and
`AppDialogsProvider` scopes asynchronous confirmations to build/stage identity.
Visited Tree, Skills and Upgrades retain compatible state within a stage, cancel
active work when hidden, and remount when the owning stage changes. Combat setup
requests relevance context only while open. Original imported text, technical
identifiers and unknown diagnostics remain available for inspection.

The official component source lives in `src/components/ui`, with Radix Rhea
configuration in `components.json`. `src/app/styles.css` defines shared semantic
colors, typography and layout. Vite wraps each feature panel's own stylesheet in a
lower-priority cascade layer (`panels`); the shell's styles live in `src/app`.
When adding UI, use the adapters or shadcn components instead of browser prompts
or `title` tooltips, and keep rendering-only translations out of calculation
inputs.
