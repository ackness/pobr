# Remaining-work reconciliation

Reviewed 2026-09-25 against `refactor/pobr-build-optimization`, starting at
`b6baa72`, active data `4.5.5.2`, PoB2 `ce566eac45ea8a86477f513c7ee65a1ebe60014e`.
Historical TODO markers are not an authoritative implementation inventory.

## Fixes in this change

- Caching: opt-in bounded `DataCalcCache` borrows immutable data/options and
  compares full Build keys, including config placeholders. Trigger-source
  memoization preserves depth/failure guards and bypasses Compare mode. The
  text `CalcCache` is bounded and structurally collision-safe for computation;
  legacy `peek(u64)` remains hash-only. No existing application automatically
  uses the new cache, and no benchmarked speedup is claimed. Explicit manual
  key projections use exhaustive destructuring to flag newly added fields.
- Support ingest: caller-provided level/quality MORE and less modifiers now
  receive the same optional skill-type restriction as parsed support MORE.
  Mana multipliers and level/quality source nodes already existed. Type
  restriction is not unique skill-instance isolation; the full Build path
  uses its own per-skill support ingestion.
- Weapons: convert the legacy unarmed crit fraction to percentage points at
  the calculation boundary, retaining stored snapshots; derive supported
  main/offhand conditions from the vendor weapon table rather than legacy
  Bow/Crossbow/Talisman/FishingRod exceptions.
- Numeric parity: enemy numeric IgnoreArmour reduces positive armour; aura
  DoT consumes AuraEffect and Magnitude in their proper scopes. Repeated
  DOUBLED forms share a global limit, without adding an inert multiplier
  indirection unsupported by the current runtime.
- Intimidating Cry: explicit active-cry uptime feeds Double Damage before
  Triple precedence, including the active-zero/WarcryMaxHit distinction.
  This does not supply a missing Exerts producer (see below).
- Enemy setup: boss Unique/RareOrUnique/PinnacleBoss and common Poise
  modifiers are Effective-only. The full Build configuration bridge applies
  the same gate to player-side Unique/RareOrUnique conditions. Legacy preset
  JSON is not rewritten.
- Special rules: correct leading literal `+` in the producer and regenerate
  the two affected active-version suppress rules against their pinned source.
- BuildData: move existing lookup implementations into private skill,
  equipment, passive and rule domains while preserving flat fields,
  constructors, trait contracts and computation order. This is source
  organization, not a memory-use reduction.

## Already implemented or superseded before this change

| Reported gap | Current evidence / boundary |
| --- | --- |
| Grenade 159 overflows a u64 | `pobr-data/src/skill.rs` uses five u64 words; the generated name table includes Grenade. No representation rewrite is needed. |
| No minion actor dimension | `Env.minions` contains independent Actors and `perform` runs minion passes. Specific buff/actor-scope gaps still need concrete cases. |
| Controlled Metamorphosis / From Nothing | `jewel_tree.rs` and `tests/skills/passive_jewels.rs` already cover ring allocation and extra roots/keystone centers. |
| No conqueror transformations | Fixed keystone, small-node and attribute transformations exist; seed-dependent results are a different, still-unavailable input. |
| GemEffects not exported | The table is in `pipeline/config.json` and the vendor-derived overlay is loaded and merged at runtime. The direct official-table adapter still lacks the join; do not remove the working overlay. |
| 4.5.4.3 decode blocks current data | npm still reports pathofexile-dat 15.2.0, but bounded replay of three current 4.5.5.2 tables succeeded and matched the versioned quality receipt. No proof of the historical failure's root cause or future patch compatibility. |
| EHP rounding / boss penetration | Current EHP placeholders already round and use enemy per-element penetration; old schema commentary incorrectly described offensive player penetration. |
| cross_type_source_hit interpolation | `cross_type_source_hit_at_roll` and its interpolation regressions already exist in the ailment path. This is not the exposure source-scope TODO in setup_env.rs. |

## Still open: exact limits and next steps

| Item | Why it is not marked complete | Required next implementation/evidence |
| --- | --- | --- |
| Calcs GUI walkthrough (4.3) | Source comparison is not GUI observation. | Run the same-build visual checklist in [Calcs display comparison](calcs-display-comparison.md). |
| True dependency-driven incremental calculation | TraceGraph records arithmetic contributions, not exhaustive stage reads, negative lookups or invalidation dependencies. | Instrument input-to-stage read sets, define immutable stage outputs and dependency invalidation, then prove incremental/uncached output and diagnostic equivalence over mutation sequences before enabling reuse. Full-result memoization is not this feature. |
| Per-source exposure effect | The flat player ModDb still merges skill-local exposure effects before strongest-source selection. | Preserve group/source association for exposure and its local effects, scale each source before max, and test multiple hosts plus generic Config exposure. Do not infer host identity from identical support names. |
| Skill weaponTypes restrictions on offhand | GrantedEffect schema/lookup has no source allowlist. | Add optional pinned-vendor extraction and backwards-compatible schema/lookup, then gate individual hand passes; no inference from coarse skill types. |
| Intimidating Exerts source | No production `NumIntimidatingExerts` producer was established. Current explicit-input consumer must not substitute Infernal Empowers. | Establish pinned data/source semantics and add end-to-end active-skill fixture before claiming full warcry parity. |
| General Warcry keyword | Unsupported keyword scopes are intentionally rejected; dropping a flag would apply a modifier globally. | Add typed keyword/config support and the vendor WarcryDamageAppliesToSkill transfer, with positive and negative scope tests. |
| Catalyst quality | Editing preserves catalyst metadata, but calculation lacks matching affix-tag provenance. | Preserve official tag identity and catalyst metadata into the calculation view, then scale only matching, scalable affixes with default/explicit quality tests. Never apply a blanket quality MORE. |
| Official GemEffects adapter | Runtime overlay works, but base adapter does not join official GemEffects foreign keys. | Verify versioned input receipts/columns and joins; preserve overlay precedence and old snapshot compatibility. |
| Seed-dependent jewel transformations | Pinned PoB2 also lacks the seed result tables. | Await a verified source; preserve unresolved diagnostics and planner protection instead of guessing a PoE1 algorithm. |
| Historical tree without coordinates | Borrowing current geometry would fabricate radius effects. | Keep conservative no-geometry behavior; separately audit unknown-version fallback/diagnostics. |
| Granted special socket search | Import/calculation supports grants; trade search enumerates static allocated sockets only. | Define active-tree-plus-grants socket availability and connect search, including empty sockets, removal and version-change tests. |
| Historical suppress snapshots | Active output fixed. Exact pinned historical extraction was attempted, but changed entry counts/other rules, not just the two patterns. | Review/reconcile historical producer drift before accepting regeneration; do not hand-patch generated rules or apply active-vendor output to old versions. |
| Remaining enemy preset entries | Knockback, minimum movement speed and additional Map Boss/Xesht Poise entries are not yet injected by setup_env. | Implement only with corresponding consumers and scoped regression coverage. |
| Remaining physical mitigation branches | ChanceToIgnoreEnemyArmour, ChanceToIgnoreEnemyPhysicalDamageReduction and PartialIgnoreEnemyPhysicalDamageReduction are separate from numeric IgnoreArmour. | Establish actual producers and implement the vendor probability/config semantics with numeric fixtures. |
| Rule-by-rule acceptance | A generator regression does not verify every rule's numeric/condition/tag consumers. | Continue bounded source-and-consumer acceptance; compilation and parse success are not full mechanic support. |

Historical regeneration was attempted in temporary independent source trees
using each artifact's own pin: `4.5.4.8` → `7d6f530c`, `4.5.4.3` → `ce8bffab`,
`4.5.0.3.4` → `a82a33b4`. All producers ran; broad decoded differences were
rejected and no historical generated file was changed. The shared vendor was
not checked out or modified.

## Deliberately unchanged

`resolve_main_skill` and Build orchestration remain in pobr-build.
No consumer was added for offline `parsed_mods.json`; no hot-swappable rule
packs, Rhai engine, new stat-ID input domain or speculative official data
domain was introduced. These are not defects merely because a historical
plan mentions them.

## Integrated validation (2026-09-25)

Component worktrees used scoped checks and small tests. Three independent
read-only reviews followed; the parent fixed the missing config-placeholder
cache key and the obsolete panel-mode boss assertion before integration.
Exhaustive key destructuring and a real placeholder-mutation regression were
added. No parity baseline or historical generated snapshot was relaxed.

| Check | Result |
| --- | --- |
| rustfmt and workspace Clippy, all targets, warnings denied | Passed. One collapsible-if warning was fixed and the failed Clippy stage resumed. |
| `cargo nextest run --workspace` | 2307 passed, 10 skipped; includes `parity_no_regression` and WASM JSON contracts. |
| Workspace doctests | Passed: 1 executed, 2 ignored. |
| `cargo run -p lint-i18n` | Passed; 54 existing zh-TW missing-key warnings, zero extra keys. |
| WASM release rebuild and data sync | Passed; synchronized active 4.5.5.2 data. |
| Web Vitest | 223 passed, 3 opt-in benchmark tests skipped. |
| Web typecheck + production build | Passed. |
| Real-WASM Playwright: build-roundtrip and config-controls | 6 passed, including trigger selection, gem quality, loadout edits and all catalog config controls. |

The full gate was invoked once. Its first run stopped at Clippy; after that
fix, test compilation completed but the command reached its 30-minute
limit without reporting any executed test. No process remained. Only the
unfinished test/doctest/i18n stages were resumed; the successful test run took
136 seconds. Already passing workspace stages were not restarted. Web tests
used the newly rebuilt WASM and current production dist, not stale artifacts.

The first delegation workflow failed during result-state serialization after
all eight component tasks completed. Patches were preserved and only the
unexecuted read-only reviews were relaunched through the same protocol. No
implementation lane was duplicated. Recovery/check logs were temporary local
evidence, not a repository prerequisite. Full browser-suite/Worker tests and
the PoB2 desktop GUI walkthrough were not run; no GUI-comparison claim follows
from the six E2E tests.

### Follow-up review

An end-to-end regression exposed an unconditional boss-rarity bridge in
`calc_orchestrator/prepare.rs`, bypassing the enemy ModDb's Effective gate.
The bridge now requires Effective mode. The regression covers both rarity
conditions, all four enemy tiers, panel/Effective modes, and option versus
explicit Build tier selection. It failed on the old implementation before
the fix. The integrated Web/WASM results above predate this follow-up.

Follow-up validation passed: `cargo test -p pobr-build --lib` (214 tests),
`cargo test -p pobr-build --test parity parity_no_regression` (unchanged
baseline), and `driver.sh lint -p pobr-build --lib` (rustfmt and Clippy).
