# PoBR WASM integration

## Initialization and types

`pobr_wasm.js` exports the generated ES module bindings. `pobr_wasm.d.ts` describes
those functions. `api-types.ts` describes their JSON payloads, including
`CalculateBuildRequest`, `CalculateBuildResponse` and `BuildJson`; use `import type`
to consume these shapes without a runtime dependency.

`manifest.json` has `packageVersion`, `schemaVersion`, `dataVersion`, `commit`,
`dataFileCount` and hashed runtime files. `pobr-data.json.gz` expands to
`{ version, files: { "base/...json": "JSON text", "overlay/...json": "JSON text", ... } }`.
Stage each **relative key and string value** unchanged, including `overlay-common/`
and localization files, then call `initStagedData()`. Do not prepend the data version
or parse the values into JS objects before passing them to `stageDataFile`.

`schemaVersion()` is the JSON compatibility handshake. Compare it to the schema
your integration expects, and to the manifest. Package versions and data versions
are separate. Breaking JSON changes bump the schema; formula/data updates need
not. Pin the complete release for reproducible behavior.

## Node.js

Read [../scripts/demo.mjs](../scripts/demo.mjs) for the complete executable example.
The essential binary load is:

```js
import { readFileSync } from 'node:fs';
import { initSync } from './pobr_wasm.js';
initSync({ module: readFileSync(new URL('./pobr_wasm_bg.wasm', import.meta.url)) });
```

Decode data with `node:zlib`'s `gunzipSync`, stage it, and calculate as in the demo.
This is ESM, not a CommonJS `require()` build. Node's `fetch()` does not load local
`file:` URLs; use bytes and `initSync` as shown.

## Browser / Web Worker

Place this code in an ES module beside the extracted runtime files and serve
over HTTP(S). Serve `.wasm` as `application/wasm`. Serve `pobr-data.json.gz` as
`application/gzip` **without `Content-Encoding: gzip`**: this example explicitly
decompresses the file. Ordinary HTTP compression may be enabled for JS/WASM.
Cross-origin hosting needs CORS. Use a Worker to keep synchronous calculations
off the UI thread.

```js
import init, * as engine from './pobr_wasm.js';

async function get(name) {
  const response = await fetch(new URL(name, import.meta.url));
  if (!response.ok) throw new Error(`${name}: HTTP ${response.status}`);
  return response;
}

await init();
const manifest = await (await get('manifest.json')).json();
if (engine.schemaVersion() !== manifest.schemaVersion) throw new Error('Schema mismatch');
// Also compare schemaVersion() with the version supported by your application.
const response = await get('pobr-data.json.gz');
const data = await new Response(
  response.body.pipeThrough(new DecompressionStream('gzip')),
).json();
if (data.version !== manifest.dataVersion) throw new Error('Data version mismatch');
for (const [path, content] of Object.entries(data.files)) engine.stageDataFile(path, content);
engine.initStagedData();

const result = JSON.parse(engine.calculateBuildJson(JSON.stringify({
  character: { class_name: 'Warrior', level: 90 },
})));
console.log(result.stats, result.unsupported_modifiers, result.item_errors);
```

Initialize once per instance. Do not stage concurrently with another initialization;
the data initialization consumes the staging map. Browser decompression temporarily
allocates the decompressed JSON in addition to engine memory. Compressed file size
does not estimate peak memory.

## JSON API examples and boundaries

| Export | Input | Result |
| --- | --- | --- |
| `decodeBuildJson(code)` | PoB2 build code string | JSON string describing imported build/loadouts |
| `calculateBuildJson(json)` | `CalculateBuildRequest` serialized as JSON | `CalculateBuildResponse` JSON string |
| `fullDpsJson(json)` | Calculation request JSON | `FullDpsResponse` JSON string |
| `attributionJson(json)` | `AttributionRequest` JSON | Source contribution JSON string |
| `gemCatalogJson()` | No argument; data initialized | Gem catalog JSON string |
| `encodeBuildJson(json)` | Calculation request with `character.class_name` required | PoB2 code string, not JSON |

For an imported code, use `{ pob_code: code }` as the calculation request.
For a new build, use `{ character: { class_name: 'Warrior', level: 90 } }`.
`decodeBuildJson()` output is an editable/import model, not a calculation request;
do not pass it wholesale to `calculateBuildJson()`.
For export, send the materialized editable request with `character.class_name`.
Optional `base_code` preserves other imported loadouts while replacing the active
sets; omitting it generates a single-set build. Optional `notes` supplies exported
notes. These export-only fields extend the bundled calculation request type.

Optional fields such as `items`, `socket_groups`, `allocated_nodes` and `flasks`
replace those sections of the request; they are not patches. `extra_modifiers`
adds synthetic modifier text for a probe, for example `['+100 to maximum Life']`.
Quest Stat rewards default on, including 5% increased Life from Molten Shrine.
The demo disables `config_inputs['questInterlude 2Khari CrossingMolten Shrine']`
to make its flat-Life delta assertion independent of that reward.
Use the chosen release's types for slots, gem IDs, skill selection and configuration.
The low-level calculation consumes one active weapon context; UI-level weapon
switching/loadout orchestration is not automatically reproduced by this package.

Look up stats by stable `stats[].id` (for example `Life` or `TotalDPS`), not array
position or translated display labels. `value` can be `null` for unavailable
outputs. Preserve `unsupported_modifiers`, `item_errors`, and tree diagnostics.

Invalid calls throw JS `Error`s. Build endpoints commonly put a JSON
`{ code, message, slot? }` error envelope in the message (`not_initialized`,
`bad_request`, `decode_error`, `internal`); initialization and other exports can
throw plain text. Parse conditionally and retain the original error on failure.
Do not catch errors and substitute zero DPS or suppress unsupported effects.
