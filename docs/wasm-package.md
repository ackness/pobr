# PoBR WASM package

The `pobr-wasm-v<version>-web.tar.gz` GitHub Release asset contains the standalone
calculation engine, ES module bindings, TypeScript declarations, JSON contract
types, matching game data, an executable demo, the `use-pobr-wasm` skill and the
PoBR license. It needs no PoBR web UI, Cloudflare Worker, Lua interpreter or Rust
installation to run.

## Download and run

Download the archive and `SHA256SUMS` from the chosen
[GitHub Release](https://github.com/ackness/pobr/releases). Keep the extracted
files together. With Node.js 22 or newer:

```bash
tar -xzf pobr-wasm-v<version>-web.tar.gz
cd pobr-wasm-v<version>-web
node example.mjs
# Or calculate a UTF-8 file containing a PoB2 build code:
node example.mjs --build-code build.txt
```

The default demo calculates a level-90 Warrior with the default 5% Life quest
reward explicitly disabled and verifies that a synthetic
`+100 to maximum Life` modifier increases Life by 100. It reports package, data
and schema versions, Life and diagnostic counts. The imported-build mode checks
positive Life and reports unsupported modifiers/item errors.

`manifest.json` records the package version, build-time source commit and dirty
state, compile-input fingerprint, tool versions, compiled JSON schema version,
data version, file sizes and SHA-256 hashes. `SHA256SUMS` next to the archive
covers the release assets. The API is beta: pin an engine/data pair and compare
`schemaVersion()` with your supported JSON schema. Unchanged schema numbers do
not promise unchanged game calculations or complete PoB2 mechanic coverage.

## Agent skill and Browser/Node.js API

The archive includes `skills/use-pobr-wasm/`. Copy that complete folder into
your agent's skill directory to install it, then invoke `$use-pobr-wasm`.
The skill remains usable after installation: pass `--package` to its demo to
select the extracted release directory.

Read `skills/use-pobr-wasm/references/api.md` inside the archive for Browser/Web
Worker code, initialization order, JSON examples, error handling and limitations.
In a repository checkout, the canonical skill is at
[`.claude/skills/use-pobr-wasm/SKILL.md`](https://github.com/ackness/pobr/blob/master/.claude/skills/use-pobr-wasm/SKILL.md).
Its API guide is
[`.claude/skills/use-pobr-wasm/references/api.md`](https://github.com/ackness/pobr/blob/master/.claude/skills/use-pobr-wasm/references/api.md).
Generated `pobr_wasm.d.ts` documents JS exports; `api-types.ts` supplies JSON
request/response types. Use `import type` for the latter.

## Build and release maintenance

From a repository checkout with the configured Rust, wasm-pack and Node tools:

```bash
pnpm --dir web build-wasm
pnpm --dir web sync-data
pnpm --dir web package-wasm
pnpm --dir web smoke-wasm-package
```

Outputs go under `.cache/wasm-release/`. Packaging reuses the built WASM and the
complete synchronized Web data manifest, including shared overlays and i18n.
It excludes the UI, images, example player builds and maintenance audit report.
`build-wasm` first checks the source fingerprint, tool versions, relevant build
environment/Cargo configuration, generated binding hashes and compiled schema.
Unchanged inputs reuse the existing package and its original receipt without
calling wasm-pack. `pnpm --dir web build-wasm --force` explicitly rebuilds through
Cargo without cleaning its cache. Missing or old receipts cause one normal build.
A new receipt is written only after a successful build with unchanged compile
inputs. Packaging verifies that receipt against the current Rust sources,
embedded locales, manifests, lockfile, build configuration and JSON types, checks
the generated binding hashes, and reads the schema from the compiled engine.
It also compares synchronized data with the current snapshot and patch/common
layers, including same-version changes. Stale inputs require a rebuild or sync;
packaging itself never rebuilds or downloads. Direct `wasm-pack` builds without
the receipt cannot be packaged. An unrelated later commit does not relabel the
binary: its original build commit and dirty flag remain in the release manifest.
See [development workflow](development-workflow.md) for cache storage reporting
and the dev/test profile policy. Old versioned archives in `.cache/wasm-release/`
are retained until explicitly removed; packaging does not delete release history.

The existing tag/manual CI runs packaging tests, builds WASM, packages it, and
extracts the archive in a temporary directory to calculate both a new character
and an existing build. Manual dispatch saves the `wasm-release` Actions artifact
and size summary. A pushed `v*` tag publishes the assets only after both Rust
and Web gates pass, creating a Release if needed or preserving existing notes
when attaching assets. Tags must match the Cargo workspace version. A tag with
a version suffix such as `-rc.1` creates a prerelease. No npm publication occurs.

`wasm-size.json` / `wasm-size.md` report raw byte counts, gzip level-9 estimates,
the actual archive size, and a **measured, hash-identified** pob-web reference.
`devs/ci/pob-web-wasm-size-baseline.json` records the acquisition/measurement dates,
raw and gzip bytes, artifact hashes and compression runtime. CI uses that
committed snapshot and does not download a moving upstream target. The deployed
binary/data source revision remains unknown; the pinned wrapper/shims commit
does not identify those artifacts.

To acquire and measure a reference, then package and exercise PoBR:

```bash
node web/scripts/benchmark-pob-web.mjs --prepare
node web/scripts/measure-pob-web-size.mjs --output devs/ci/pob-web-wasm-size-baseline.json
pnpm --dir web package-wasm
pnpm --dir web smoke-wasm-package
node --test web/scripts/measure-pob-web-size.test.mjs web/scripts/package-wasm.test.mjs
```

The reference data bundle is compressed once with gzip level 9; transfer estimates
also gzip WASM and JS, without compressing the stored data gzip again. The complete
PoBR release archive additionally contains types, docs, a demo and the agent skill.
pob-web does not provide an equivalent release archive in this snapshot, so its
runtime components—not an invented release archive—are the comparison scope.

[pob-web's Lua bundler](https://github.com/krauthaufen/pob-web/blob/master/packages/pob-data/bundle-lua.mjs)
packages PoB Lua source, game data and runtime libraries; its
[WASM build](https://github.com/krauthaufen/pob-web/blob/master/packages/lua-wasm/scripts/build.mjs)
compiles the Lua interpreter. PoBR compiles calculations directly into WASM.
The report compares WASM + JS + compressed data and separately estimates all-gzip
transfer. Dataset contents, supported mechanics and compression differ; neither
the comparison nor a passing package smoke test establishes numerical parity.
These are disk/transfer sizes, not runtime memory measurements.

## Runtime performance comparison

The [performance and build analysis](wasm-performance.md) document
([中文](wasm-performance.zh-CN.md)) records an opt-in
PoBR Rust WASM versus pob-web Lua WASM **code-import + calculation** benchmark.
The [baseline](../devs/ci/pob-web-performance-baseline.json) preserves the measured
samples, host, artifact hashes, selected outputs and failed fixtures; later
observations are kept beside it as dated records (see the analysis document). They are
separate from the measured size baseline above. `mise run setup` installs the toolchain
this benchmark needs.

`node web/scripts/benchmark-pob-web.mjs --prepare` acquires a hash-verified local
reference snapshot, preserving the upstream license and pinning its wrapper,
shims and loading patches. The downloaded deployment's binary/data revision
is unknown; the wrapper commit must not be used to label those artifacts.
Run `node web/scripts/bench-pob-web.mjs --iterations 9 --warmup 2` from the root
after building WASM and syncing data. Full output is written to
`.cache/pob-web-performance.json`; failed fixtures are retained without speed
ratios and cause exit status 1. Add `--pobr-only` to measure PoBR alone on every build
without a reference snapshot (for builds the Lua reference rejects); that report goes to
`.cache/pobr-performance.json` and carries no speed ratio. Subsequent preparation
verifies the existing snapshot offline; use a new directory to measure a changed deployment.
The runner uses an explicit seven-build historical cohort from the shared parity
fixtures and discovers all fixtures in `examples/wasm-benchmark/builds/`.
Eleven previously failing/unstable historical builds were removed from this
performance cohort at the user's request; their parity fixtures remain intact.
The historical cohort is post-selected and is not an unbiased sample. New poe.ninja
captures retain their actual league, snapshot, character URL, fetch date and code
hash; they do not silently become parity goldens. README tables distinguish
latest-season captures from historical inputs. To preserve a compact baseline
with raw samples/diagnostics but without full breakdown responses, run
`node web/scripts/snapshot-pob-web-benchmark.mjs` after the benchmark report exists.
Without `--output` it overwrites the baseline above, so pass a dated path to keep earlier
records; a `--pobr-only` report requires `--output`.

Regression checks:

```bash
node --test web/scripts/benchmark-comparison.test.mjs web/scripts/benchmark-builds.test.mjs web/scripts/benchmark-pob-web.test.mjs
```

Lua measurements include the Node Worker host, adapter checks and upstream
headless UI-state setup. Implementations, JSON responses and supported mechanics
differ. These observations do not establish a kernel-speed advantage,
incremental-edit latency, browser performance, memory usage or numerical parity.
