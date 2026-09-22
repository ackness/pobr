---
name: use-pobr-wasm
description: Integrate the standalone PoBR WebAssembly calculation engine in Node.js, browsers or Web Workers; initialize release game data, call the JSON API and run a calculation demo. Use for consuming PoBR WASM packages, not for changing PoE2 formulas or operating the PoBR UI.
---

# Use PoBR WASM

Use a matching PoBR release engine/data pair. The package is a `wasm-pack
--target web` ES module; Node.js can load the same binary through `initSync`.
It does not need Lua, the PoBR website or the Pages Worker.

## Obtain and verify the package

- Use an extracted `pobr-wasm-v<version>-web.tar.gz` provided by the user, or
  download a chosen version from `https://github.com/ackness/pobr/releases`.
  Pin the version when integrating. Releases predating WASM packaging may have
  no binary asset. Do not silently substitute source archives or a different version.
- Check the sibling `SHA256SUMS` against the downloaded assets. After extraction,
  inspect `manifest.json` for `packageVersion`, `schemaVersion`, `dataVersion`
  and `commit`. Keep `pobr_wasm.js`, its `.wasm` and `pobr-data.json.gz` together.
- If building from a checkout, run its configured `pnpm --dir web build-wasm`,
  `sync-data`, `package-wasm` and `smoke-wasm-package` scripts. Outputs are in
  `.cache/wasm-release/`. Source changes need a rebuild even without a version bump.

## Run the verified demo

Node.js 22 or newer is sufficient; no npm install is needed. Resolve this
skill's `scripts/demo.mjs` path, then run:

```bash
node <skill-dir>/scripts/demo.mjs --package <extracted-package-dir>
node <skill-dir>/scripts/demo.mjs --package <extracted-package-dir> --build-code <build-code.txt>
```

The package also includes the same script as `example.mjs`: from the extracted
directory run `node example.mjs`. It initializes all data, calculates a level-90
Warrior with the default 5% Life quest reward disabled, adds a synthetic
`+100 to maximum Life` modifier and asserts that Life increases by exactly 100.
With `--build-code`, it instead calculates that PoB2
code and checks positive Life. Output includes package/data/schema versions,
Life and unsupported-modifier/item-error counts. Report observed output and
failures; do not describe merely importing WASM as a successful calculation.

## Integrate

Read [references/api.md](references/api.md) for initialization, runnable browser
code, JSON shapes, errors and version checks. Inspect the chosen package's
`pobr_wasm.d.ts` for exported signatures and `api-types.ts` for JSON types.
Use `import type` for `api-types.ts`; most domain APIs accept and return strings.

Initialize once per WASM instance: load binary, stage every relative data path,
then call `initStagedData()`. Calculation calls are synchronous. Use a Web Worker
for expensive browser operations and avoid racing initializations. Each Worker
has its own data and memory. Data is separate from the WASM binary; omitting it
prevents full build calculations.

Preserve `unsupported_modifiers`, `item_errors` and other API diagnostics in the
integration. PoBR is an independent implementation with incomplete PoB2 coverage;
a parsed modifier, successful demo or unchanged schema does not prove numerical
parity for every build. Package size is not runtime memory usage. Do not publish,
upload builds or modify remote releases as part of this consumption workflow.
