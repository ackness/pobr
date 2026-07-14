---
name: run-pobr
description: Bootstrap, build, test, and drive the pobr (Path of Building in Rust) Cargo workspace from a clean machine. Use when asked to run, build, test, set up, bootstrap, or smoke-test pobr; clone the PoB2 vendor reference; run the parity gate / version-bump-drill; or look up a PoB2 (Path of Building) formula in vendor Lua.
---

pobr is a headless **Rust workspace** (calc engine + CLI + data pipeline) — no GUI. You drive it via the committed harness **`.claude/skills/run-pobr/driver.sh`**: it installs luajit, clones the PoB2 vendor reference at the pinned commit (for reading PoB2's Lua formulas — never committed), builds the workspace, runs the targeted tests + parity gate, and greps vendor Lua for formula adjudication.

All paths below are relative to the repo root (`<unit>/`). The driver figures out the repo root itself, so you can call it from anywhere.

## Prerequisites

Ubuntu, root with `sudo`. The only system dep beyond the Rust toolchain + Node is **luajit** (for the vendor extraction/oracle tooling). `cargo`, `gcc`, `make`, `node`, `python3` are already present in this container.

```bash
sudo apt-get install -y luajit
```

`nextest` is **not** installed — use plain `cargo test` (the driver does).

## Run (agent path) — the driver

One-time bootstrap on a fresh machine (deps + vendor clone + build):

```bash
bash .claude/skills/run-pobr/driver.sh bootstrap
```

Check what's set up:

```bash
bash .claude/skills/run-pobr/driver.sh status
```

The green "is it working" signal (build + targeted tests incl. `parity_no_regression`, ~1540 tests, exit 0):

```bash
bash .claude/skills/run-pobr/driver.sh smoke
```

Look up a PoB2 formula / parse rule in vendor Lua (fixed-string match across ModParser/CalcOffence/CalcDefence/CalcPerform) — this is how you adjudicate engine-vs-PoB2 divergences:

```bash
bash .claude/skills/run-pobr/driver.sh lua "per (%d+) intelligence"
```

Other subcommands: `deps` (luajit only), `vendor` (clone/align PoB2 to the pinned commit), `build`, `test`, `data` (data status + regen-pipeline notes), `diff <verA> <verB>` (semantic cross-version data diff — what nodes/skills/mods changed), `drill` (version-bump reproducibility — see Gotchas).

## Vendor reference (PoB2 clone)

`vendor/PathOfBuilding-PoE2/` is a **local clone of Path of Building (PoE2) for reference only** — reading its Lua calc/parse implementation to verify pobr's formulas. It is **gitignored (`/vendor/`) and never committed** (~739 MB). The pinned commit is read from `data/<CURRENT>/overlay/mod_parser_rules.json::_meta.vendor_commit` so it always matches the in-tree data; `driver.sh vendor` clones it shallow (`fetch --depth 1` by SHA). It is **session-local** — it vanishes when the container recycles; re-run `driver.sh vendor` next session.

## Data & versions (data/calc are decoupled)

The game data is **committed** under `data/<version>/` (`base`/`overlay`/`generated`/`i18n`) — **testing needs no download**. Multiple versions live side by side (currently `4.5.0.3.4`, `4.5.2.1.3`, `4.5.4.3`).

The calc is **version-agnostic**: `pobr_gamedata::data_version()` resolves `POBR_DATA_VERSION` env → `data/CURRENT` → `pobr_data::DATA_VERSION` const. Switching the active version is **zero-code** — `export POBR_DATA_VERSION=4.5.0.3.4` or write `data/CURRENT`.

Prove it runs on every committed version (the `multi_version` smoke — `BuildData::load` + full calc per version):

```bash
bash .claude/skills/run-pobr/driver.sh versions
```

The active default is the latest committed version (`pobr_data::DATA_VERSION` = `4.5.4.3`, matching `data/CURRENT`). PoB2 golden/parity values are version-specific, so golden tests pin `pobr_data::GOLDEN_PARITY_DATA_VERSION` (= `4.5.0.3.4`, decoupled from the active default) — advancing the default doesn't false-red parity; re-pinning golden to a newer version requires **re-recording** it. `driver.sh data` prints the regen pipeline; the committed data is the source of truth.

To see *what actually changed* between two committed versions (the iteration input PoB2 gets from export+CHANGELOG — added/removed/renumbered nodes, skill stat deltas, mod-pool removals, overlay drift):

```bash
bash .claude/skills/run-pobr/driver.sh diff 4.5.0.3.4 4.5.2.1.3            # all domains
bash .claude/skills/run-pobr/driver.sh diff 4.5.0.3.4 4.5.2.1.3 --domain tree --limit 40
```

The full data-versioning + iteration model (PoB2's two regimes vs pobr's snapshot model, the test philosophy, and the open gaps) is documented in `devs/docs/architecture/16-data-versioning-and-iteration.md`.

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
| `drill` reports overlay `DIFF`/`FAIL` | expected with vendor present; run `smoke` for the pass/fail signal, or run `drill` with `--version 4.5.0.3.4` (still fails on the headless `config_options`/`mod_parser_rules` extracts). |
| `no such command: nextest` | not installed; use `cargo test` (the driver does). |
