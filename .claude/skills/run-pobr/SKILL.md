---
name: run-pobr
description: Bootstrap, build, test, and drive the pobr (Path of Building in Rust) Cargo workspace from a clean machine. Use when asked to run, build, test, set up, bootstrap, or smoke-test pobr; clone the PoB2 vendor reference; run the parity gate / version-bump-drill; or look up a PoB2 (Path of Building) formula in vendor Lua.
---

pobr is a Rust workspace with a Web frontend. Read the validation policy in
[`CLAUDE.md`](../../../CLAUDE.md) before choosing checks. The canonical driver is
`.claude/skills/run-pobr/driver.sh`; the optional, gitignored `.agents` entry forwards to it. Run commands
from the repository root so the configured Rust/Node toolchains stay consistent.

## Prerequisites

Inspect the existing environment first:

```bash
bash .claude/skills/run-pobr/driver.sh status
```

Use the configured Rust toolchain. Web checks use Node and the pnpm version in
`web/package.json`; CI uses Node 22. Lua extraction/oracle checks need `luajit`.
Do not bootstrap an already working checkout. On a fresh Ubuntu machine the
`bootstrap` command installs LuaJIT through apt, clones the pinned vendor and
builds the workspace. On other platforms install missing prerequisites using
the appropriate package manager before using the driver.

`cargo-nextest` is optional locally. The full gate detects it and otherwise uses
`cargo test`; CI installs nextest. A failing nextest run is a failure, never a
reason to retry the whole suite under another runner.

## Daily agent workflow

Select the affected test target before running Cargo. A test-name filter alone
can still compile unrelated integration targets; specify `--test` or `--lib`.
For example, a change to support gating needs:

```bash
bash .claude/skills/run-pobr/driver.sh test -p pobr-build --test skills support_gating::
bash .claude/skills/run-pobr/driver.sh lint -p pobr-build --lib --test skills
```

`test` forwards ordinary `cargo test` arguments unchanged. `lint` runs fmt and
Clippy for exactly the supplied Cargo targets. Both require explicit arguments.
They preserve compiler diagnostics and build-lock messages.

- Use the change-to-check table in `CLAUDE.md` for parity, cross-crate contracts,
  Web, Worker and script changes. Do not run Rust checks for documentation alone.
- Tests already compile their targets; do not prepend a workspace check/build.
- A normal local commit requires relevant tests and lint, not `full`. Reuse
  passing checks for unchanged code/dependencies/toolchain/features/data. Stop after relevant checks
  pass, and report the scope actually verified.
- Run only one Cargo command at a time per target directory. Keep each worktree's
  default `target`; do not change profiles or clean caches as routine preparation.
- For an environment smoke check with no specific changed behavior, use `smoke`.
  It runs representative aggregation, parser and codec tests without a workspace
  build. It is unnecessary after relevant tests already passed.

## Full gate

Before merge/release, or for toolchain/features/workspace dependency changes and
core changes whose impact cannot be bounded:

```bash
bash .claude/skills/run-pobr/driver.sh full
```

This runs fmt, workspace Clippy, nextest plus separate doctests (or `cargo test --workspace` when nextest is absent), and i18n lint. It stops at the first failure.
CI retains its full Rust and Web gates on tags/manual dispatch; ordinary pushes
and PRs do not trigger CI automatically. Do not repeat a successful full gate for
an unchanged local commit, or dispatch an identical CI run in addition to a tag.
Publishing, PR updates and version bumps follow the user's authorized scope.

Timing diagnostics and data-version drills are opt-in, not everyday gates:

```bash
cargo test -p pobr-wasm --test perf_timing --test perf_phases -- --ignored --nocapture
bash .claude/skills/run-pobr/driver.sh drill
```

For PoB2 formula adjudication, read the pinned local Lua source:

```bash
bash .claude/skills/run-pobr/driver.sh lua "per (%d+) intelligence"
```

Other commands: `deps`, `vendor`, `bootstrap`, `build`, `data`, `versions`,
`diff <verA> <verB>`. Use build/bootstrap only when their artifacts or setup are
needed, not as preparation for a test command.

## Vendor reference (PoB2 clone)

`vendor/PathOfBuilding-PoE2/` is a **local clone of Path of Building (PoE2) for reference only** — reading its Lua calc/parse implementation to verify pobr's formulas. It is **gitignored (`/vendor/`) and never committed** (~739 MB). The pinned commit is read from `data/<CURRENT>/overlay/mod_parser_rules.json::_meta.vendor_commit` so it always matches the in-tree data; `driver.sh vendor` clones it shallow (`fetch --depth 1` by SHA). In ephemeral environments it disappears when the checkout is recycled; run `driver.sh vendor` only when the reference is missing or needs aligning.

## Data & versions (data/calc are decoupled)

The game data is **committed** under `data/<version>/` (`base`/`overlay`/`generated`/`i18n`) — **testing needs no download**. Multiple versions live side by side; inspect `data/CURRENT` for the active default.

The calc is **version-agnostic**: `pobr_gamedata::data_version()` resolves `POBR_DATA_VERSION` env → `data/CURRENT` → `pobr_data::DATA_VERSION` const. Switching the active version is **zero-code** — `export POBR_DATA_VERSION=4.5.0.3.4` or write `data/CURRENT`.

Prove it runs on every committed version (the `multi_version` smoke — `BuildData::load` + full calc per version):

```bash
bash .claude/skills/run-pobr/driver.sh versions
```

The active default is recorded in `data/CURRENT` and `pobr_data::DATA_VERSION`. PoB2 golden/parity values are version-specific, so golden tests pin `pobr_data::GOLDEN_PARITY_DATA_VERSION` (= `4.5.0.3.4`, decoupled from the active default) — advancing the default doesn't false-red parity; re-pinning golden to a newer version requires **re-recording** it. `driver.sh data` prints the regen pipeline; the committed data is the source of truth.

To see *what actually changed* between two committed versions (the iteration input PoB2 gets from export+CHANGELOG — added/removed/renumbered nodes, skill stat deltas, mod-pool removals, overlay drift):

```bash
bash .claude/skills/run-pobr/driver.sh diff 4.5.0.3.4 4.5.2.1.3            # all domains
bash .claude/skills/run-pobr/driver.sh diff 4.5.0.3.4 4.5.2.1.3 --domain tree --limit 40
```

For the committed data workflow, read [pipeline/README.md](../../../pipeline/README.md)
and [docs/version-bump-architecture.md](../../../docs/version-bump-architecture.md).
`devs/docs/architecture/16-data-versioning-and-iteration.md` is optional local
background; its historical gap list is not a current implementation checklist.

## Gotchas

- **vendor commit is per-data-version.** Each `data/<version>/` pins its own PoB2 vendor commit in `overlay/mod_parser_rules.json::_meta.vendor_commit` (e.g. `4.5.4.3` → `29ab8262`, `4.5.0.3.4` → `2df5a74`). The driver reads the commit for `data/CURRENT` and clones that; pass `--version <ver>` to `version-bump-drill.sh` to drill a non-current version.
- **`extract-lua` and `pob2-oracle` do NOT work with the old `4.5.0.3.4` vendor commit (`2df5a74`).** That commit loads the parser into *local* scope (`modLib` is `nil`), and `pob2-oracle/run.sh` hangs >120 s in headless bootstrap — so regen for that pinned version is not reproducible; only the `precompile` step byte-reproduces. Current-version extraction works. **Reading vendor Lua always works** (the `lua` subcommand) — that's the main use.
- Use `smoke` for the green signal; `drill` is the niche version-bump-reproducibility check (older pinned versions fail its extract step per above).
- **extract-lua needs an *absolute* `--vendor-root`.** A relative path + the tool's `cd vendor/src` doubles the `LUA_PATH` and the bundled `runtime/lua/xml.lua` isn't found (`no file '.../xml.lua'`).
- **GGG patch CDN 404s for old pinned versions** (`4.5.0.3.4`), so the `.dat` download step isn't runnable; the in-tree data is authoritative.

## Troubleshooting

| Symptom | Cause / fix |
|---|---|
| `no file '/usr/.../xml.lua'` from extract-lua | relative `--vendor-root`; use absolute (the driver/oracle scripts already do). |
| `modLib.parseMod missing after bootstrap` | extract-lua/oracle vendor-incompat (commit `2df5a74`); known — regen not reproducible here, read vendor Lua instead. |
| `drill` reports overlay `DIFF`/`FAIL` | Inspect the selected data version and its pinned vendor; old-version extraction limitations do not justify ignoring failures for the current version. |
| `no such command: nextest` | Use `cargo test`; `full` selects this fallback automatically when nextest is unavailable. |
