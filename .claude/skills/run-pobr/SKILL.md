---
name: run-pobr
description: Bootstrap, build, test, and drive the pobr (Path of Building in Rust) Cargo workspace from a clean machine. Use when asked to run, build, test, set up, bootstrap, or smoke-test pobr; clone the PoB2 vendor reference; run the parity gate / version-bump-drill; or look up a PoB2 (Path of Building) formula in vendor Lua.
---

pobr is a Rust workspace with a Web frontend. Read the validation policy in
[the maintainer guide](../../../docs/maintainer-guide.md) before choosing checks. Development commands
are mise tasks in the root `mise.toml` (`mise tasks`, `mise run <task> [args]`), backed by
`devs/scripts/driver.sh`; the game CLI binary `pobr` carries none of them; the optional, gitignored `.agents` entry forwards to it. Run commands
from the repository root so the configured Rust/Node toolchains stay consistent.

## Prerequisites

Inspect the existing environment first:

```bash
bash devs/scripts/driver.sh status
```

`rust-toolchain.toml` pins the Rust version with rustfmt/Clippy; rustup installs
it on first use. `mise.toml` and CI both use Node 26.
Use the pnpm version in `web/package.json`. Lua extraction/oracle checks need `luajit`.
Do not bootstrap an already working checkout. On a fresh Ubuntu machine the
`bootstrap` command installs LuaJIT through apt, clones the pinned vendor and
builds the application CLI. Use `mise run build --workspace` only when all binaries
are needed. On other platforms install missing prerequisites using
the appropriate package manager before using the driver.

`cargo-nextest` is optional locally. The full gate detects it and otherwise uses
`cargo test`; CI installs nextest. A failing nextest run is a failure, never a
reason to retry the whole suite under another runner.

## Daily agent workflow

Select the affected test target before running Cargo. A test-name filter alone
can still compile unrelated integration targets; specify `--test` or `--lib`.
For example, a change to support gating needs:

```bash
mise run verify build skills support_gating::
```

Use `mise run targets` to discover suites. For slow compilation diagnostics only,
use `mise run timings build parity`; it is not an extra validation step.

`test` accepts `<crate> <suite|lib> [filter]` or forwards raw `cargo test`
arguments unchanged. `verify` runs the selected tests followed by lint; a failure
stops the command. `timings` compiles selected tests without executing them.
`lint` runs fmt and
Clippy for exactly the supplied Cargo targets. Both require explicit arguments.
They preserve compiler diagnostics and build-lock messages.

- Use the change-to-check table in `docs/maintainer-guide.md` for parity, cross-crate contracts,
  Web, Worker and script changes. Do not run Rust checks for documentation alone.
- Tests already compile their targets; do not prepend a workspace check/build.
- A normal local commit requires relevant tests and lint, not `full`. Reuse
  passing checks for unchanged code/dependencies/toolchain/features/data. Stop after relevant checks
  pass, and report the scope actually verified.
- Reuse one default `target` per worktree, including its parallel sessions; run
  Cargo commands sequentially. Do not create per-session/gate targets or share
  writable targets across worktrees. Dev/test disable incremental object caches
  but retain Cargo artifact reuse; do not toggle profiles or clean as routine preparation.
- `mise run cache` reports storage and file counts without building or deleting.
  `pnpm --dir web build-wasm` reuses verified unchanged bindings; `--force`
  bypasses receipt reuse without cleaning Cargo. See [cache policy](../../../docs/development-workflow.md#缓存与磁盘占用).
- For an environment smoke check with no specific changed behavior, use `smoke`.
  It runs representative aggregation, parser and codec tests without a workspace
  build. It is unnecessary after relevant tests already passed.

## Full gate

Full merge/release and broad-change gates should run in cloud CI. Release tags
trigger the gate automatically; for another pushed ref use `mise run ci <ref>`
and inspect `mise run ci-status`. This tests the remote ref, never local edits.
A dispatch request is not a passing result. Do not require a duplicate local
full gate before cloud CI. When local full Rust validation is explicitly needed:

```bash
mise run full
```

This runs fmt, workspace Clippy, nextest plus separate doctests (or `cargo test --workspace` when nextest is absent), and i18n lint. It stops at the first failure.
CI retains its full Rust and Web gates on tags/manual dispatch; ordinary pushes
do not trigger CI, and pull requests only run the lightweight checks in `pr.yml`. Do not repeat a successful full gate for
an unchanged local commit, or dispatch an identical CI run in addition to a tag.
Publishing, PR updates and version bumps follow the user's authorized scope.
See [workflow commands](../../../docs/development-workflow.md). Application version
bumps leave the seven `crates/` library versions unchanged.

Timing diagnostics, parity dashboards and data-version drills are opt-in, not everyday gates:

```bash
mise run test build parity parity_baseline_report -- --ignored --nocapture
cargo test -p pobr-wasm --test perf_timing --test perf_phases -- --ignored --nocapture
bash devs/scripts/driver.sh drill
```

For PoB2 formula adjudication, read the pinned local Lua source:

```bash
bash devs/scripts/driver.sh lua "per (%d+) intelligence"
```

Other commands: `deps`, `vendor`, `bootstrap`, `build`, `data`, `versions`,
`diff <verA> <verB>`. Use build/bootstrap only when their artifacts or setup are
needed, not as preparation for a test command.

## Vendor reference (PoB2 clone)

`vendor/PathOfBuilding-PoE2/` is a **local clone of Path of Building (PoE2) for reference only** — reading its Lua calc/parse implementation to verify pobr's formulas. It is **gitignored (`/vendor/`) and never committed** (~739 MB). The pinned commit is read from `data/<CURRENT>/overlay/mod_parser_rules.json::_meta.vendor_commit` so it always matches the in-tree data; `driver.sh vendor` clones it shallow (`fetch --depth 1` by SHA). In ephemeral environments it disappears when the checkout is recycled; run `driver.sh vendor` only when the reference is missing or needs aligning.

## Data & versions (data/calc are decoupled)

The game data is **committed** under `data/<version>/` (`base`/`overlay`/`generated`/`i18n`) — **testing needs no download**. Multiple versions live side by side; inspect `data/CURRENT` for the active default.

The calc is **version-agnostic**: `pobr_gamedata::data_version()` resolves
`POBR_DATA_VERSION` → the loader data root's `CURRENT`, and fails when
neither is set; no version is compiled in. `POBR_DATA_ROOT`
overrides the loader data root; otherwise it is the repository's `data/`.
Switching the active version needs no code change: set `POBR_DATA_VERSION`
or update the selected data root's `CURRENT`.

Prove it runs on every committed version (the `multi_version` smoke — `BuildData::load` + full calc per version):

```bash
bash devs/scripts/driver.sh versions
```

The active default is discovered from `data/CURRENT` at runtime. Tests and
PoB2 reference values always target the active version: a data update
re-records the references with `examples/demo-bd-test/tools/recapture_golden.py`
and `recapture_panel_golden.py`. The repository keeps the active version plus
the two most recently added (`mise run data:prune`). `driver.sh data` prints the regen
pipeline; committed data is the source of truth.

To see *what actually changed* between two committed versions (the iteration input PoB2 gets from export+CHANGELOG — added/removed/renumbered nodes, skill stat deltas, mod-pool removals, overlay drift):

```bash
bash devs/scripts/driver.sh diff <old> <new>            # all domains
bash devs/scripts/driver.sh diff <old> <new> --domain tree --limit 40
```

For the committed data workflow, read [pipeline/README.md](../../../pipeline/README.md)
and [docs/version-bump-architecture.md](../../../docs/version-bump-architecture.md).
`devs/docs/architecture/16-data-versioning-and-iteration.md` is optional local
background; its historical gap list is not a current implementation checklist.

## Gotchas

- **vendor commit is per-data-version.** Each `data/<version>/` pins its own PoB2 vendor commit in `overlay/mod_parser_rules.json::_meta.vendor_commit` (e.g. `4.5.4.3` → `29ab8262`, `4.5.0.3.4` → `2df5a74`). The driver reads the commit for `data/CURRENT` and clones that; pass `--version <ver>` to `version-bump-drill.sh` to drill a non-current version.
- Use `smoke` for the green signal; `drill` is the niche version-bump-reproducibility check (older pinned versions fail its extract step per above).
- **extract-lua needs an *absolute* `--vendor-root`.** A relative path + the tool's `cd vendor/src` doubles the `LUA_PATH` and the bundled `runtime/lua/xml.lua` isn't found (`no file '.../xml.lua'`).
- **GGG patch CDN 404s for old pinned versions**, so the `.dat` download step isn't runnable; the in-tree data is authoritative.

## Troubleshooting

| Symptom | Cause / fix |
|---|---|
| `no file '/usr/.../xml.lua'` from extract-lua | relative `--vendor-root`; use absolute (the driver/oracle scripts already do). |
| `drill` reports overlay `DIFF`/`FAIL` | Inspect the selected data version and its pinned vendor; old-version extraction limitations do not justify ignoring failures for the current version. |
| `no such command: nextest` | Use `cargo test`; `full` selects this fallback automatically when nextest is unavailable. |
