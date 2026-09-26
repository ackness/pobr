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
`build-wasm` writes a receipt only after a successful build with unchanged
compile inputs. Packaging verifies that receipt against the current Rust sources,
embedded locales, manifests, lockfile, build configuration and JSON types, checks
the generated binding hashes, and reads the schema from the compiled engine.
It also compares synchronized data with the current snapshot and patch/common
layers, including same-version changes. Stale inputs require a rebuild or sync;
packaging itself never rebuilds or downloads. Direct `wasm-pack` builds without
the receipt cannot be packaged. An unrelated later commit does not relabel the
binary: its original build commit and dirty flag remain in the release manifest.

The existing tag/manual CI runs packaging tests, builds WASM, packages it, and
extracts the archive in a temporary directory to calculate both a new character
and an existing build. Manual dispatch saves the `wasm-release` Actions artifact
and size summary. A pushed `v*` tag publishes the assets only after both Rust
and Web gates pass, creating a Release if needed or preserving existing notes
when attaching assets. Tags must match the Cargo workspace version. A tag with
a version suffix such as `-rc.1` creates a prerelease. No npm publication occurs.

`wasm-size.json` / `wasm-size.md` report raw byte counts, gzip level-9 estimates,
the actual archive size, and a comparison with the user-provided **approximate**
pob-web baseline: `lua.wasm` 256 KiB, `lua.mjs` 67.5 KiB and
`pob-lua.json.gz` 4.65 MiB (about 4.97 MiB combined). The baseline's revision and
measurement date are unknown; CI does not download a moving upstream target.
Update `devs/ci/pob-web-wasm-size-baseline.json` with measured bytes and provenance
when a pinned reference is available.

[pob-web's Lua bundler](https://github.com/krauthaufen/pob-web/blob/master/packages/pob-data/bundle-lua.mjs)
packages PoB Lua source, game data and runtime libraries; its
[WASM build](https://github.com/krauthaufen/pob-web/blob/master/packages/lua-wasm/scripts/build.mjs)
compiles the Lua interpreter. PoBR compiles calculations directly into WASM.
The report compares WASM + JS + compressed data and separately estimates all-gzip
transfer. Dataset contents, supported mechanics and compression differ; neither
the comparison nor a passing package smoke test establishes numerical parity.
These are disk/transfer sizes, not runtime memory measurements.
