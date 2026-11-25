---
name: audit-builds
description: >-
  Collect PoE2 builds from poe.ninja or other build sites into local cache,
  compare PoBR calculations against PoB2/reference values, separate genuine
  PoBR engine bugs from PoB2 bugs or intentional mechanics using game data,
  and implement, verify, and ship minimal fixes. Use when auditing build parity,
  fetching new builds to test PoBR, investigating large calculation
  discrepancies (~5%+ heuristic), or running iterative accuracy rounds.
---

# Audit PoBR Against External Builds

Run an evidence-first loop to find and fix meaningful PoBR calculation bugs
without chasing noise or copying PoB2 defects. Defer repository rules,
validation tiers, and data boundaries to the
[Maintainer Guide](../../../docs/maintainer-guide.md),
[Diagnostics Guide](../../../docs/diagnostics.md), and
[Workflow Skill](../workflow/SKILL.md).

## Guiding Principles

- **Strictly PoE2-only:** Only audit **Path of Exile 2 (PoE2)** builds,
  endpoints, guide sections, and mechanics (`poe2wiki.net`, `poe2db.tw`, PoE2
  ladder data). Ignore PoE1 builds, PoE1 Wiki/formulas, and PoE1 endpoints
  completely; reject any export code whose decompressed XML root is PoE1
  `<PathOfBuilding>` rather than `<PathOfBuilding2>`.
- **`~5%` is a triage heuristic, not a rigid quota.** Focus on discrepancies
  that indicate missed mechanics, unmapped `skill_data` or gem stats, formula
  errors, wrong weapon/trigger scopes, or silently dropped inputs. Ignore tiny
  rounding differences or exporter-only artifacts.
- **Check intentional differences before editing.** Never assume PoB2 or an
  exported build reference is right. Before changing engine code, check
  [docs/pob2-differences.md](../../../docs/pob2-differences.md),
  [known-differences.json](../../../devs/scripts/known-differences.json), and
  existing regression tests under `crates/pobr-build/tests/skills/`.
- **Judge truth in strict order:** (1) game data and item/gem text
  (`mise run lookup`, `data/<ver>/base/`), (2) PoE2 Wiki / PoE2DB, (3) vendored
  PoB2 Lua as cross-check only. Never port a PoB2 bug into PoBR.
- **Scope and ownership:** Route decoding/fact fixes to `ackness/repoe`,
  translations to `ackness/poe2-zh-cn-stats`, and calculation/modifier fixes to
  `ackness/pobr`. Only model mechanics that affect supported static-simulation
  outputs, and never let an input disappear without a diagnostic.
- **Workspace hygiene:** Store fetched builds only under `.cache/` (never
  commit `.cache/`, `web/src/fixtures/*.json`, or scratch reports). Stop
  scraping immediately on HTTP `429`, Cloudflare, or anti-bot gates and fall
  back to offline corpora. Never run `triage`, `diagnose`, or `compare` on
  `examples/reference-builds/holdout/`. Run strictly one Cargo command at a
  time.

## Workflow

### 1. Acquire or Stage Build Codes

See [Build Sources Reference](./references/build-sources.md) for PoE2-only raw
PoB endpoints, `poe2.ninja` / `poe.ninja/poe2` ladder APIs, PoE2 guide hubs,
and non-PoB formats.

- Fetch a fresh PoE2 `poe.ninja` cohort (one character per ascendancy):
  ```bash
  mise run builds:fetch .cache/<batch> --pick first   # or --pick last
  ```
- Normalize PoE2 build codes from PoE2-specific links or shares (`maxroll.gg/poe2/pob/...`,
  `poe2db.tw/pob/...`, PoE2 `pobb.in` / `pastebin.com` / `rentry.co` shares) into
  `.cache/<batch>/builds/<build-id>/code.txt` (with optional `meta.json`), after
  verifying the decoded XML uses `<PathOfBuilding2>`.
- If live scraping hits HTTP `429` or anti-bot checks, stop at once and audit
  existing `.cache/` batches plus `examples/wasm-benchmark/builds` and
  `examples/reference-builds/tuned/builds`.

### 2. Batch Triage and Cluster Discrepancies

Prepare local data/oracle if needed (`mise run data:prepare`, `mise run vendor`),
build the CLI, and triage the batch:

```bash
mise exec -- cargo build -p pobr-cli
python3 devs/scripts/triage.py --jobs 4 --json .cache/<batch>/triage-before.json .cache/<batch>/builds
```

Inspect `engine`, `both`, and `no oracle` rows with meaningful or large deltas
(`~5%+` relative diff or unexpected `0`/`inf` values). Cluster affected builds
by shared `stat`, `leads`, `unsupported` lines, `unread` modifiers, or rejected
`gem_stats` rather than patching individual builds.

### 3. Attribute PoBR vs. PoB2 / Exporter Root Causes

For each cluster, inspect a representative build before touching code:

1. Check [docs/pob2-differences.md](../../../docs/pob2-differences.md),
   [known-differences.json](../../../devs/scripts/known-differences.json), and
   `crates/pobr-build/tests/` to avoid reverting intentional PoE2 mechanics.
2. Inspect formulas, breakdowns, and line fate:
   ```bash
   mise run compare <build> [Stat ...]
   mise run diagnose <build>
   ./target/debug/pobr calculate-build --file <code.txt> --reference-basis --breakdown <Stat,...> --usage
   mise run gap "<template or line>"
   mise run lookup <stat-or-skill-id>
   ```
3. Decide:
   - **Genuine PoBR bug / missing rule** (contradicts game data/text on a
     supported output) -> Step 4.
   - **PoB2 bug / exporter artifact / intentional PoE2 model** -> leave engine
     unchanged; document evidence if helpful.
   - **Upstream data / locale bug** -> route via [Workflow Skill](../workflow/SKILL.md).

### 4. Implement Minimal Fixes and Regression Tests

- Fix the shared rule, formula, or stat-mapping root cause cleanly.
- If modifier/rule data changed, run `mise run regen` once for the batch.
- Add L1 unit tests (`crates/pobr-core`) and/or real-data integration tests
  (`crates/pobr-build/tests/`) reproducing the mechanic from game data.

### 5. Staged Verification (One Cargo Command at a Time)

1. Format and run focused checks:
   ```bash
   mise exec -- cargo fmt --all
   mise run verify <crate> <suite> <filter>
   ```
2. Verify L2 invariance and accuracy baselines:
   ```bash
   mise run snapshot:report   # Expect 0 unexpected L2 output changes
   mise run accuracy          # Confirm no regressions on benchmark/tuned/holdout
   ```
   An unexpected L2 diff is an immediate warning to re-check intentional designs.
3. Rebuild `pobr-cli` and re-run triage to confirm the target cluster dropped
   with zero newly introduced regressions:
   ```bash
   mise exec -- cargo build -p pobr-cli
   python3 devs/scripts/triage.py --jobs 4 --json .cache/<batch>/triage-after.json .cache/<batch>/builds
   ```
4. Before pushing, run workspace checks:
   ```bash
   mise exec -- cargo fmt --all -- --check
   mise exec -- cargo clippy --workspace --all-targets -- -D warnings
   mise exec -- cargo test --workspace
   mise run check:pr
   ```

### 6. Commit, Push, and Report

- Confirm `git status --short` and `git diff --stat` include only intended code,
  rules, or tests (never `.cache/` or `web/src/fixtures/*.json`).
- Commit (`fix(...): ...`), push within authorized scope, and confirm remote CI
  passes (`gh pr checks <pr>`).
- In the final report, state exact before/after numbers, list fixed PoBR bugs
  with evidence, and separate confirmed PoB2 bugs or intentional mechanics.
