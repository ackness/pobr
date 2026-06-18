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

Other subcommands: `deps` (luajit only), `vendor` (clone/align PoB2 to the pinned commit), `build`, `test`, `data` (data status + regen-pipeline notes), `drill` (version-bump reproducibility — see Gotchas).

## Vendor reference (PoB2 clone)

`vendor/PathOfBuilding-PoE2/` is a **local clone of Path of Building (PoE2) for reference only** — reading its Lua calc/parse implementation to verify pobr's formulas. It is **gitignored (`/vendor/`) and never committed** (~739 MB). The pinned commit is read from `data/<CURRENT>/overlay/mod_parser_rules.json::_meta.vendor_commit` so it always matches the in-tree data; `driver.sh vendor` clones it shallow (`fetch --depth 1` by SHA). It is **session-local** — it vanishes when the container recycles; re-run `driver.sh vendor` next session.

## Data

The game data is **committed** under `data/<version>/` (`base`/`overlay`/`generated`/`i18n`) — **testing needs no download**. `driver.sh data` prints the regen pipeline for version bumps. **In this cloud env the data regen is NOT reproducible** (see Gotchas); the committed data is the source of truth.

## Gotchas

- **vendor commit is per-data-version, and the drill defaults to the wrong one.** `data/CURRENT` = `4.5.0.3.4` → vendor `2df5a74`; but `pipeline/config.json` `patch` = `4.5.2.1.3` → a *different* vendor commit (`a82a33b4`). `version-bump-drill.sh` defaults `VERSION` to the config patch, so it byte-diffs the wrong version's overlays. The driver reads the commit from `data/<CURRENT>` and clones that; pass `--version 4.5.0.3.4` if you run the drill by hand.
- **`extract-lua` and `pob2-oracle` do NOT work with vendor `2df5a74`.** The extractor asserts a global `modLib.parseMod`, but this commit loads the parser into *local* scope (`modLib` is `nil`; re-`LoadModule("Modules/ModParser")` then dies on local `SkillType`). `pob2-oracle/run.sh` hangs >120 s in headless bootstrap. So **data regen is not reproducible here** — only the `precompile` step byte-reproduces (drill step 4 = `byte-diff=0`). **Reading vendor Lua works fine** (the `lua` subcommand) — that's the main use.
- **`driver.sh drill` FAILS when vendor is present** (step-3 extract-lua drifts/fails per above), and **PASSES when vendor is absent** (step 3 SKIPs). Use `smoke` for the green signal; `drill` is the niche version-bump-reproducibility check.
- **extract-lua needs an *absolute* `--vendor-root`.** A relative path + the tool's `cd vendor/src` doubles the `LUA_PATH` and the bundled `runtime/lua/xml.lua` isn't found (`no file '.../xml.lua'`).
- **GGG patch CDN 404s for old pinned versions** (`4.5.0.3.4`), so the `.dat` download step isn't runnable; the in-tree data is authoritative.

## Troubleshooting

| Symptom | Cause / fix |
|---|---|
| `no file '/usr/.../xml.lua'` from extract-lua | relative `--vendor-root`; use absolute (the driver/oracle scripts already do). |
| `modLib.parseMod missing after bootstrap` | extract-lua/oracle vendor-incompat (commit `2df5a74`); known — regen not reproducible here, read vendor Lua instead. |
| `drill` reports overlay `DIFF`/`FAIL` | expected with vendor present; run `smoke` for the pass/fail signal, or run `drill` with `--version 4.5.0.3.4` (still fails on the headless `config_options`/`mod_parser_rules` extracts). |
| `no such command: nextest` | not installed; use `cargo test` (the driver does). |
