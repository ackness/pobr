# WASM performance and build analysis

**English** | [简体中文](wasm-performance.zh-CN.md)

The opt-in benchmark runs **real PoBR Rust WASM** and
[pob-web's Lua WASM](https://github.com/krauthaufen/pob-web), using the same
committed PoB2 build codes. It measures **code import + calculation + JSON
response**, not a cached stat read or an isolated calculation kernel.

For the later diagnosis and version check, see
[Import and calculation investigation](#import-and-calculation-investigation-2026-10-12).

Measured at **2026-10-12T16:13Z** on an Apple M4 / macOS 27.0 arm64 host, Node **26.11.1**.
PoBR is the release WASM built from `18b6a03` (rustc 1.99.0, wasm-pack 0.15.0), schema 8,
data `0.5.5.4.2`. When it was built the only uncommitted change was `mise.toml`, so the build
receipt says `dirty: true`; its source fingerprint `02320289…` covers the compiled sources.
The Lua side is the deployed pob-web bundle (PoB2 runtime `v0.23.0`), hash-verified.
Each build gets two untimed warmups and nine measured samples.

## Latest season: Forbidden Rites

Eight public builds were captured from [poe.ninja's latest season](https://poe.ninja/poe2/builds/forbiddenrites)
on **2026-10-11**, snapshot `0632-20261011-47556`, one per base class.
[Selection and source evidence](../examples/wasm-benchmark/selection.json) records the
index URLs, characters and hashes. Candidates were chosen before timing: one
declared ascendancy filter per class, then the lexicographically first public
account/character among that filter's first 100 index rows. Failed candidates
were not replaced. The [new fixtures](../examples/wasm-benchmark/builds/) are separate
from the older parity corpus and are not new numerical goldens.

Times are milliseconds; speedup is **Lua / Rust time**. Each completed build has
nine measured samples; p95 uses nearest rank (the maximum of nine samples).

| Class / ascendancy / main skill | PoBR median | Lua median | Speedup | PoBR p95 | Lua p95 | p95 ratio |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Druid / Oracle / Lightning Bolt † | 10.07 | Lua error | — | 10.43 | — | — |
| Huntress / Amazon / Ice Shot | 8.90 | 515.99 | 57.98× | 9.29 | 890.59 | 95.91× |
| Mercenary / Gemling Legionnaire / Comet † | 10.86 | Lua error | — | 11.18 | — | — |
| Monk / Martial Artist / Ice Shot | 9.57 | 529.74 | 55.33× | 10.23 | 547.08 | 53.47× |
| Ranger / Deadeye / Ice Shot † | 7.85 | Lua error | — | 8.00 | — | — |
| Sorceress / Stormweaver / Comet † | 9.27 | Lua error | — | 10.19 | — | — |
| Warrior / Titan / His Scattering Calamity † | 8.07 | Lua error | — | 8.30 | — | — |
| Witch / Blood Mage / Impending Doom † | 11.86 | Lua error | — | 12.00 | — | — |

† PoBR-only measurement from the same session (`bench-pob-web.mjs --pobr-only`, 2026-10-12T16:15Z)
with identical warmup, sample count and stability checks. These calls are not interleaved with
Lua and are not a paired result.
[Raw samples, selected skills, diagnostics and hashes](../devs/ci/pobr-only-performance-2026-10-12-m4.json).

**The six Lua errors are a pob-web Lua WASM compatibility problem**, not PoBR
import failures: `CalcDefence.lua` passes infinity to integer `s_format` formatting
(five at line 2789, Deadeye at line 3844). PoBR timings remain visible, but no Lua time
or speed ratio is published for an aborted calculation. **The completed pairs also have
materially different outputs and incomplete PoBR coverage**, so these are API-workload
timings—not an equivalent-mechanics or universal engine-speed comparison:

| Completed latest-season build | PoBR TotalDPS | Lua TotalDPS | PoBR unsupported modifiers |
| --- | ---: | ---: | ---: |
| Amazon / Ice Shot | 88,117.54 | 237,926.57 | 26 |
| Martial Artist / Ice Shot | 2,900,388.52 | 5,173,634.32 | 13 |

All 15 builds report zero item parse errors.

> Later correction: the measured version did not report an item it could not read for a
> `{ pob_code }` request. The Stormweaver's body armour (`Rarity: RELIC`) was dropped at the
> time, so its defences and damage are those of a character without it. That rarity is now
> read, and any other unreadable item appears in `item_errors`. The figures in this section
> are the original measurements.

<details>
<summary>Historical comparison cohort: 7 builds</summary>

These are older Runes of Aldur captures, not latest-season builds. At the user's
request, 11 previously failing/unstable historical builds were removed from the
performance cohort, not from the shared parity regression fixtures. This cohort
is selected after observing an earlier run and is not an unbiased historical sample.
Their `TotalDPS` agrees with Lua within 1.8% (largest: Pathfinder, 1.73%), which is an
observation, not a parity claim.

| Class / ascendancy / main skill | PoBR median | Lua median | Speedup | PoBR p95 | Lua p95 | p95 ratio |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Huntress / Spirit Walker / Twister | 10.69 | 501.93 | 46.96× | 11.27 | 642.42 | 57.02× |
| Mercenary / Gemling Legionnaire / Explosive Grenade | 8.49 | 547.25 | 64.46× | 8.95 | 834.05 | 93.19× |
| Ranger / Deadeye / Explosive Grenade | 7.38 | 435.78 | 59.06× | 7.79 | 874.92 | 112.37× |
| Ranger / Pathfinder / Ice Shot | 9.01 | 373.88 | 41.47× | 9.19 | 375.55 | 40.88× |
| Warrior / Smith of Kitava / Shield Wall | 8.08 | 658.55 | 81.46× | 8.60 | 1471.68 | 171.03× |
| Warrior / Titan / Shield Wall | 9.38 | 444.76 | 47.44× | 10.17 | 450.95 | 44.35× |
| Witch / Abyssal Lich / Detonate Dead | 13.48 | 426.56 | 31.65× | 14.44 | 445.93 | 30.88× |

</details>

| Initialization: single observation | PoBR (ms) | Lua (ms) | Lua / Rust |
| --- | ---: | ---: | ---: |
| Engine and data boot | 463.10 | 1898.10 | 4.10× |

Across the nine paired builds the speed ratio is **31.65×–81.46×**; PoBR medians over all 15
builds are 6.83–13.13 ms. A second complete paired run four minutes later agreed within
6.8% on every speed ratio and within 11.0% on every PoBR median (Lua: 10.7%), so the
recorded run is representative of this host, though it is still one host.
[Paired samples, diagnostics and artifact hashes](../devs/ci/pob-web-performance-2026-10-12-m4.json).

## Measurement limits

In the paired run, each build gets two untimed warmups; measured calls alternate engine order.
Unique legal JSON whitespace bypasses PoBR's exact-request response cache.
Code inflation is timed on both sides. Lua timings include Node Worker transport,
adapter checks and headless UI-state construction; PoBR returns a different,
more detailed JSON contract. Initialization excludes artifact reads/hash checks
and Lua host-worker creation, but includes module import, data staging and boot.
There is no browser rendering, download, incremental-edit or runtime-memory test.

All **15 selected builds** are retained in the report: **9 completed comparisons**
and **6 rejected latest-season cases**. The benchmark rejects swallowed Lua errors, partial results
and unstable repeated outputs rather than scoring them as fast calculations.
It exits nonzero if any fixture fails. Performance does **not** prove mechanic
coverage or numerical parity; unsupported-modifier and item diagnostics are preserved.

The deployed Lua/JS/data were downloaded from `pob.awx.at`; their source revision is
unknown, and their SHA-256 values are recorded in the paired record. Wrapper/shims/loading
patches are pinned separately to upstream `ee33ce6`. Keep the hash-verified reference
snapshot to rerun the same artifacts; a fresh download may measure a newer deployment.

## Package size

Actual byte measurements of the same engine/data artifacts; **MiB = 1,048,576 bytes**.
Data is gzip level 9. The transfer estimate also gzip-compresses WASM and JS,
without recompressing the data gzip. PoBR figures are this checkout's `package-wasm`
report (v0.0.36, schema 8, data `0.5.5.4.2`).

| Component / scope (MiB) | PoBR | pob-web | PoBR / Lua size |
| --- | ---: | ---: | ---: |
| WASM, raw | 5.057 | 0.250 | 20.23× |
| JavaScript, raw | 0.023 | 0.066 | 0.34× |
| Compressed data bundle | 4.340 | 4.633 | 0.94× |
| WASM + JS + compressed data | 9.420 | 4.949 | 1.90× |
| gzip transfer estimate | 6.191 | 4.763 | 1.30× |

**PoBR's current runtime bundle is larger, not smaller.** It compiles the Rust
engine into WASM; Lua's smaller WASM is an interpreter whose calculation code
travels in the data bundle. Dataset contents, localization and supported
mechanics differ. These sizes do not measure runtime memory.
The complete PoBR release archive is approximately **6.182 MiB**, including
types, docs, a demo and the agent skill; the reference deployment has no matching
release archive to compare.

[Measured Lua size baseline](../devs/ci/pob-web-wasm-size-baseline.json) records hashes,
dates and compression settings. Generate the size reports and exercise the package:

```bash
node web/scripts/measure-pob-web-size.mjs --output devs/ci/pob-web-wasm-size-baseline.json
pnpm --dir web package-wasm
pnpm --dir web smoke-wasm-package
node --test web/scripts/measure-pob-web-size.test.mjs web/scripts/package-wasm.test.mjs
# .cache/wasm-release/wasm-size.json and wasm-size.md
```

## Reproduce and test

From the repository root. `mise run setup` installs everything this needs: the Rust version
pinned by `rust-toolchain.toml` with the `wasm32-unknown-unknown` target, wasm-pack, Node 26,
pnpm 12 and the Web dependencies. Run the commands below in a shell that has mise activated,
or prefix each with `mise exec --`. The first `build-wasm` also compiles `wasm-bindgen-cli`
once (wasm-pack caches it), so expect a few extra minutes.

```bash
mise run setup
pnpm --dir web build-wasm
pnpm --dir web sync-data
node web/scripts/benchmark-pob-web.mjs --prepare
node web/scripts/bench-pob-web.mjs --iterations 9 --warmup 2
# The recorded run has rejected cases: exit 1, but a complete report is written.
# Raw samples, full responses, diagnostics, host and provenance:
# .cache/pob-web-performance.json
node web/scripts/bench-pob-web.mjs --pobr-only --iterations 9 --warmup 2
# PoBR alone on all 15 builds, no reference snapshot needed (rows marked †).
# Report: .cache/pobr-performance.json, never the paired report above.
node web/scripts/snapshot-pob-web-benchmark.mjs --output devs/ci/pob-web-performance-<date>.json
node web/scripts/snapshot-pob-web-benchmark.mjs --input .cache/pobr-performance.json \
  --output devs/ci/pobr-only-performance-<date>.json
# Compact committed records (raw samples and diagnostics, without the full breakdown
# responses) plus Markdown rows printed to stdout. --output is required.

# Network-independent regression tests (no Rust build or downloaded Lua needed):
node --test web/scripts/benchmark-comparison.test.mjs web/scripts/benchmark-builds.test.mjs web/scripts/benchmark-pob-web.test.mjs
```

The committed builds can be benchmarked offline once WASM/data and the Lua
reference snapshot are prepared. For online acquisition or replay, Python 3
and curl are also required:

```bash
# Discover the site's latest season and preselect one public build per base class.
python3 web/scripts/fetch-ninja-benchmarks.py --selection base-class-first-v1
# Replay the recorded season/snapshot/candidates into a separate capture directory.
python3 web/scripts/fetch-ninja-benchmarks.py \
  --selection-file examples/wasm-benchmark/selection.json \
  --output .cache/ninja-replay/builds --manifest .cache/ninja-replay/selection.json
python3 -m unittest discover -s web/scripts -p 'fetch_ninja_benchmarks_test.py'
```

The collector preserves source URLs, snapshot, fetch date, exported XML identities
and code hashes; retired upstream snapshots cause a visible replay failure.
It never adds captures to the parity golden corpus or replaces a failed candidate.

The [reference preparation/adapter](../web/scripts/benchmark-pob-web.mjs),
[measurement harness](../web/scripts/benchmark-comparison.mjs) and
[executable runner](../web/scripts/bench-pob-web.mjs) are complete source scripts.
Preparation downloads actual engine/data plus the pinned upstream source and
license, then verifies cached hashes on subsequent offline runs. Partial or
tampered snapshots fail rather than being silently repaired. Use
`--directory <new-dir>` when preparing a different snapshot, and
`--reference-dir <new-dir>` when measuring it; `--help` lists all options.
Tests cover statistics, option boundaries, cache-busting call identity,
repeat stability, error/partial-result rejection, safe template extraction and
provenance tampering. Benchmarks remain opt-in; wall-clock speed is not a CI gate.

## Import and calculation investigation (2026-10-12)

This investigation uses the committed benchmark cohort, not an additional
user-supplied failing build.

### PoBR: import succeeds; coverage is incomplete

All **15 builds** complete `calculateBuildJson` and output validation with zero
`item_errors`, using the current release WASM (schema 8, data `0.5.5.4.2`). These are
runtime results, not a claim of numerical parity.

| Latest-season build | Unsupported diagnostic entries | Item parse errors |
| --- | ---: | ---: |
| Oracle / Lightning Bolt | 23 | 0 |
| Amazon / Ice Shot | 26 | 0 |
| Gemling Legionnaire / Comet | 23 | 0 |
| Martial Artist / Ice Shot | 13 | 0 |
| Deadeye / Ice Shot | 8 | 0 |
| Stormweaver / Comet | 15 | 0 |
| Titan / His Scattering Calamity | 17 | 0 |
| Blood Mage / Impending Doom | 17 | 0 |

The Blood Mage build selects `ChaosFrogExplosionPlayer` (support-granted effect 2,
`TotalDPS` 1,787.28). Doom Blast is effect 3 and is reported separately by
`skillMetricsJson` (see `crates/pobr-build/tests/skills/skill_metrics.rs` and
[web contracts](web-contracts.md)). The timing does not assert which effect the build's
owner treats as its main damage skill.

Observed gaps in the current diagnostics include Bonded effects, conditional repeats,
constant flask effects and additional-projectile mechanics. The Pathfinder diagnostic
explicitly reports Fork's `support_fork_forked_projectile_damage_+%_final`
as `conditional numerical effect not modeled (UnsupportedTag("StatThreshold"))`.
Unsupported entry counts include metadata and split text lines; they are not a
count of unique missing mechanics. The Ice Shot DPS differences above have not
been causally apportioned to individual effects.

The benchmark's six error-only paired rows do not show six PoBR import failures:
[`bench-pob-web.mjs`](../web/scripts/bench-pob-web.mjs) rejects the entire pair when
Lua fails and does not retain the already-completed PoBR result in that row;
`--pobr-only` measures those builds separately.

### Lua: infinity passed to integer formatting

Replaying all eight latest-season XML builds against a fresh, hash-verified
deployment snapshot reproduced six failures and two successes. Temporary
instrumentation called the original formatter and rethrew its error; it did
not suppress errors, repair output or change the committed adapter.

- Five failures reach `CalcDefence.lua:2789`: `s_format("= %d", poolProtected)`.
  The observed argument is `inf`. The deployed formula explicitly sets
  `poolProtected = math.huge` when `sharedMindOverMatter >= 100`.
- Deadeye reaches `CalcDefence.lua:3844`:
  `s_format("Total Effective Dot Pool: %d", output[damageType.."DotEHP"])`.
  The observed argument is also `inf`; the preceding formula divides
  `TotalPool` by `TakenDotMult`.
- A minimal probe on the deployed interpreter reports `Lua 5.2`: formatting
  `1.5` with `%d` succeeds as `1`, while `math.huge`, NaN and `2147483648`
  fail with `not a number in proper range`. Native LuaJIT accepts these calls.
  The actual six failures are infinity, not ordinary fractional values.

This is a defence-breakdown formatting compatibility failure after import,
not evidence that the build codes are malformed. See the
[upstream defence code](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/99bc6e107fd36ae541b349fc36a44281cb371313/src/Modules/CalcDefence.lua#L2773-L2789)
and [Lua 5.2 formatting implementation](https://www.lua.org/source/5.2/lstrlib.c.html#str_format).
Re-downloading identical artifacts does not fix this. A
[fresh PoB2 source replay](#fresh-pob2-source-replay) also reproduced the same
failures with v0.23.1. The benchmark must continue rejecting aborted calculations.

### Is the Lua WASM current?

Checked on **2026-10-12**; “latest” has three different meanings:

| Layer | Observed state | Verdict |
| --- | --- | --- |
| pob-web wrapper/shims | Pinned `ee33ce63920f23e634cc30e37d8ad339244dbf34` equals upstream HEAD | Current |
| Public deployment | Fresh `lua.wasm`, `lua.mjs` and `pob-lua.json` SHA-256 hashes all match the recorded baseline | Same as the currently served deployment |
| Embedded PoB2 | Runtime reports `v0.23.0`; upstream latest release is `v0.23.1` | Not the latest PoB2 release |

Sources: [pob-web commit](https://github.com/krauthaufen/pob-web/commit/ee33ce63920f23e634cc30e37d8ad339244dbf34),
[PoB2 latest release checked](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/releases/tag/v0.23.1),
and [recorded artifact hashes](../devs/ci/pob-web-wasm-size-baseline.json).
The wrapper pins PoB2 submodule `99bc6e1`; the checked PoB2 HEAD
[`bb52d6b`](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/commit/bb52d6b368307457eb9c54bb13f1829993d390b1)
is 115 commits ahead of that source pin. This does not identify the deployment's
exact build revision: `deployedRevision` remains unknown.
The wrapper's [interpreter build](https://github.com/krauthaufen/pob-web/blob/ee33ce63920f23e634cc30e37d8ad339244dbf34/packages/lua-wasm/scripts/build.mjs)
uses PUC Lua 5.2.4, not LuaJIT. Updating the language version alone is not a
demonstrated compatibility fix.

Recheck deployment freshness in a new directory, preserving the recorded snapshot:

```bash
node web/scripts/benchmark-pob-web.mjs --prepare \
  --directory .cache/pob-web-freshness-2026-10-12
# Compare provenance.json files[].sha256 against the recorded artifact hashes.
# A different deployment requires a new observation, not rewriting the old timing table.
```

### Fresh PoB2 source replay

On **2026-10-12**, downloaded PoB2's latest release
[`v0.23.1` / `7d6f530`](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/commit/7d6f530cbdab20389ff8bc6ba97a37ac27f74e41)
and current upstream
[`HEAD` / `bb52d6b`](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/commit/bb52d6b368307457eb9c54bb13f1829993d390b1)
into separate temporary source snapshots. Rebuilt each Lua/data bundle using
pob-web's pinned
[bundler](https://github.com/krauthaufen/pob-web/blob/ee33ce63920f23e634cc30e37d8ad339244dbf34/packages/pob-data/bundle-lua.mjs),
changing only its input/output directories. No PoB2 source fixes were applied.
The deployed WASM/JS, wrapper, shims and loading patches stayed unchanged;
a temporary adapter argument selected the experimental bundle before MEMFS loading.
These are local reconstructed bundles, **not new pob-web deployments**.

| PoB2 bundle | Difference from deployed contents | Result on the same 15-build cohort |
| --- | --- | --- |
| Deployed v0.23.0 | Original reference | 9 paired completions; 6 Lua errors |
| Fresh v0.23.1 | 4 files changed; defence module unchanged | Same 9 completions and same 6 `s_format` failures |
| Fresh HEAD `bb52d6b` | 191 files changed, 8 added | Engine boot fails; no builds reached |

The fresh release still fails at `CalcDefence.lua:2789` and `:3844`, now with a
`v0.23.1` runtime banner. All nine completed Lua cases retain the same recorded
selected statistics (`Life`, `TotalDPS`, `CombinedDPS`, `TotalEHP`) as the old
deployment. This does not assert that every breakdown or mechanic is unchanged.

HEAD fails earlier: `Modules/Main.lua:340: syntax error near '+'`.
That [source line](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/bb52d6b368307457eb9c54bb13f1829993d390b1/src/Modules/Main.lua#L340)
is `count += 1`, which the current Lua WASM loading path cannot parse.
It is not a BD-specific error, and this experiment does not establish that
the desktop PoB2 runtime fails.

The v0.23.1 bundle completed the same nine builds with the same recorded statistics
and failed the same six; no timing from that replay is reported here.
[Comparison evidence](../devs/ci/pob2-refresh-comparison-2026-10-12.json)
retains source commits, bundle hashes, changed-file lists, raw samples, diagnostics and
the exact failures. No ratio is reported for Lua failures or HEAD's boot failure.

**Conclusion:** the current pob-web Lua WASM/adapter combination has demonstrated
compatibility failures; replacing only its PoB2 bundle with the latest release
does not resolve them, and current HEAD needs additional compatibility work even
to boot. This does not erase PoBR's own unsupported effects.
