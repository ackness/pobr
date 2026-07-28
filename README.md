# PoBR — Path of Building in Rust

**English** | [简体中文](README.zh-CN.md)

**Live web app: <https://pobr-web.pages.dev>** — deployed automatically from
`v0.x` release tags once CI passes.

> **⚠️ Beta.** PoBR is under active development. Calculation results, game
> data, the wasm/JSON API and the CLI are all still evolving and may change
> or break without notice — don't build anything load-bearing on the API yet.

PoBR is a ground-up rewrite of the
[Path of Building (PoE2)](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2)
core calculation engine, from Lua to Rust. PoB2 compatibility stays the hard
regression baseline; the rewrite exists to fix what a port can't:

- **Performance** — removes the bottlenecks in large-scale modifier
  aggregation and multi-skill calculation; the core is pure-functional and
  deterministic, so heavy paths parallelize over read-only snapshots and the
  hot mod-parsing path is precompiled offline to zero-parse at runtime.
- **Source-level attribution** — beyond PoB2 parity, every output can be
  traced back to the item / mod line / passive / gem / config that
  contributed it (`TraceGraph` + `AttributionReport`).
- **Native i18n** — the calculation uses only stable IDs; all display text
  goes through language packs (`en-US` canonical + `zh-TW`, with zh-CN
  sidecars on the web). The web frontend even accepts item text pasted in
  Simplified Chinese. Adding a language means adding data, not code.
- **Runs anywhere via WASM** — the engine compiles to WebAssembly behind a
  JSON contract, so the web app runs fully in-browser with no server; the
  same core also powers the CLI and a desktop entry point.
- **Built to extend** — a layered workspace (data → core → build → apps)
  with a data-driven pipeline: game data ships as versioned JSON generated
  from GGG `.dat` exports, and most modifier/stat behaviour is data, not
  hard-coded rules.

## Getting started

Standard cargo workflow ([`cargo-nextest`](https://nexte.st/) recommended for
running tests):

```bash
cargo nextest run --workspace          # all tests
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check

# CLI (binary name: pobr)
cargo run -p pobr-cli -- calculate --base-life 1000 --mod "+50 to maximum Life"
cargo run -p pobr-cli -- decode-code <pob_code>        # PoB build code → XML
cargo run -p pobr-cli -- parse-mod "20% increased Fire Damage"
```

For the web frontend see [`web/README.md`](web/README.md) (Vite + React + TS,
decoupled from the engine through a wasm JSON contract; not part of the cargo
workspace).

Rust **edition 2024**; all crates share one workspace version, kept in sync
with the `v0.x` release tags.

## Architecture at a glance

Data flow:

```
GGG .dat export
  └─(pobr-data-adapter, offline)→ data/<version>/*.json
       └─(pobr-gamedata, runtime loader)→ calculation layers
```

Calculation pipeline (`pobr-core`):

```
modifier text → parse → ModDb → aggregation queries → calc
  → OutputTable + Breakdown + TraceGraph + AttributionReport
```

Standard stat aggregation: `(base + Σbase) * (1 + Σinc/100) * Π(1 + more/100)`.

All file I/O is confined to `pobr-gamedata`; `pobr-data` / `pobr-core` stay
zero-I/O. Dependencies only point downward, with `pobr-data` at the bottom.

## Workspace layout

14 members — `crates/` are libraries, `apps/` are executables, `tools/` are
data/maintenance tooling:

| Crate | Responsibility |
|-------|----------------|
| `crates/pobr-data` | Pure data definitions (catalog schema: BaseItem/Stat/Mod/SkillGem/PassiveNode…); zero logic, zero I/O; bottom dependency of every crate |
| `crates/pobr-core` | Modifier parsing / storage / aggregation + calculation engine + source-level attribution + source ingestion (item/passive/gem/flask). Zero I/O |
| `crates/pobr-gamedata` | Runtime data loader — the only layer in the data system that touches files; lazy per-domain loading + i18n sidecars |
| `crates/pobr-i18n` | Language pack loading / fallback / display-text mapping (`en-US` canonical + `zh-TW`) |
| `crates/pobr-tree` | Passive tree topology, allocated-node mod collection, radius jewels |
| `crates/pobr-build` | Build state, PoB build code encode/decode, import recognition, `CalcOrchestrator` (cached), build comparison. **Home of the parity tests** |
| `crates/pobr-item` | Full-fidelity edit-mode parsing of raw item text + reverse serialization (BuildRaw round-trip) |
| `apps/pobr-cli` | CLI: `calculate` / `parse-mod` / `decode-code` / `encode-code` |
| `apps/pobr-wasm` | Web/WASM API: pure-Rust JSON in/out; wasm-bindgen bindings behind the `wasm` feature |
| `apps/pobr-desktop` | Minimal desktop entry-point skeleton |
| `tools/pobr-data-adapter` | Data pipeline adapter: GGG `.dat` export → denormalized committed JSON |
| `tools/sync-pob-catalog` | Extracts the stat catalog from PoB core Lua; parity check / diff |
| `tools/lint-i18n` | Language pack completeness check |
| `tools/precompile-mods` | Offline pre-compilation of mod-parser rules / coverage report |

(`tools/pob2-oracle` is a non-workspace pure-Lua wrapper that dumps PoB2-side
calculation breakdowns for per-component comparison.)

## Parity system (regression baseline)

PoB2 compatibility is a hard regression gate, guarded by three complementary
layers:

1. **`crates/pobr-build/tests/ninja_parity.rs`** — walks real PoB2 builds with
   golden stat values, comparing all classes / skills with zero hard-coding;
   `parity_no_regression` asserts the aggregate hit rate never drops below the
   recorded baseline.
2. **golden / dual-run suites** — pin intermediate values and config semantics.
3. **`tools/pob2-oracle`** — when a divergence needs per-component diagnosis,
   dumps the Lua-side calculation breakdown straight from the vendored PoB2.

```bash
cargo test -p pobr-build --test parity -- --nocapture   # parity dashboard
```

`vendor/PathOfBuilding-PoE2/` is a full checkout; verify formulas by reading
the local Lua directly instead of searching online.

## Documentation

- [`CLAUDE.md`](CLAUDE.md) — verification tiers, command cheat-sheet, key
  conventions (read before contributing).
- [`agent-docs/`](agent-docs/) — PoE2 (0.5.0) game-mechanics reference in
  Chinese (damage types / resistances / armour, evasion, ES / crit / ailments /
  damage-defence order, …).
- [`web/README.md`](web/README.md) — web frontend.

## Conventions

- **Only stable IDs inside the calculation** (`StatId` / `ModName` /
  `SourceId`); display text goes through `pobr-i18n`.
- **Immutable / deterministic**: mutable writes to `Env` are concentrated in
  `perform`; parallelism only fans out over read-only snapshots.
- Changes touching calculation / modifiers / the parser need matching
  integration tests or golden fixtures; changes to crate boundaries,
  aggregation semantics, catalog or parity rules must update the architecture
  docs in the same change.

## References & acknowledgements

- [PathOfBuildingCommunity/PathOfBuilding-PoE2](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2)
  (MIT) — the reference implementation and parity baseline of this project:
  calculation formulas, modifier semantics and specialModList parsing rules are
  verified one-by-one against its Lua implementation (checked out locally under
  `vendor/`, not committed; the pinned commit is recorded in
  `data/<version>/overlay/mod_parser_rules.json::_meta`).
- [poe2db.tw](https://poe2db.tw/) and the PoE2 Wiki — sources for game
  mechanics and text translations.

## License

The code is released under the [MIT](LICENSE) license.

This project is not affiliated with or endorsed by Grinding Gear Games. The
game data under `data/` is derived from Path of Exile 2 client assets and is
copyrighted by Grinding Gear Games; it is included solely for build-calculation
interoperability, in line with Path of Building community practice.
