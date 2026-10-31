# Native WASM saved-request API

The explicit native exports evaluate the same strict saved request and bounded
conditional hit as the [native CLI](native-cli.md). They reuse
`pobr_build::native_document::NativeBuildRequest`, native admission and
`NativeSkillResult` directly. There is no UI, separate calculator or native
full-build route.

| Rust host API | JavaScript export | Contract |
|---|---|---|
| `init_native_staged_data()` | `initNativeStagedData()` | Consume explicitly staged native files and validate them before installing immutable catalogs. |
| `init_native_data_from_dir(path)` | Host only | Load the explicit native version directory; no CURRENT or environment discovery. |
| `is_native_data_ready()` | `isNativeDataReady()` | Report native catalog availability, independently of request admission. |
| `calculate_native_build_json(request_json)` | `calculateNativeBuildJson(requestJson)` | Strictly reopen, admit and calculate the saved request afresh, returning the existing native result JSON. |

Use the existing `stageDataBytes(relativePath, Uint8Array)` or
`stageDataFile(relativePath, jsonText)` exports to stage `manifest.json` and all
files in its inventory under their exact relative paths. After staging a complete
pack and initializing the compiled module, the call sequence is:

```js
initNativeStagedData();
if (!isNativeDataReady() || isDataReady()) {
  throw new Error("expected native mode");
}
const result = JSON.parse(calculateNativeBuildJson(savedRequestJson));
```

The staged native initializer always uses `GameData::from_memory` and
`NativeData::load`, with actual integrity checks. It has no prechecked or
hash-skipping variant and does not use the releasing backend. `GameData` drops
after the immutable native catalogs load; no ordinary `BuildData`,
`RuntimeConstants` or default ruleset is constructed. Contract v1 can be loaded
and queried through host catalogs but cannot be calculated; admission requires v2.

Only one mode is active per instance (per thread for host tests). Successful
native initialization clears ordinary BuildData/GameData, calculation snapshots,
both translators, the tier index and all ordinary response entries. Successful
ordinary initialization, including `initStagedDataVerified`, clears native
catalogs and those caches. `isDataReady()` continues to report only ordinary
BuildData availability. Ordinary data-dependent routes fail in native mode;
native calculation fails in ordinary or uninitialized mode. Static helpers keep
their existing behavior.

Initialization is transactional for active data and derived caches. A failed load
preserves the previous mode and its catalogs/caches. Staged initializers
consume the staging area even on failure; stage a complete pack again to retry.
The native host directory initializer follows the same native loader and does
not consult legacy debug-backend selection.

Persist the complete existing [native document JSON](native-build-document.md).
Every calculation decodes it again with `NativeBuildRequest::from_json`, checks
the exact dataset, actor, gem/effect/stable branch, level/quality and actor-policy
pins, and reruns native admission. No response cache can bypass those checks.
Missing or duplicate fields, escaped duplicate aliases, unsupported edits,
different datasets and legacy XML/build-code input fail. No format autodetection
or fallback to ordinary calculation occurs.

Calculation errors use the existing JSON error envelope on the Rust `Err` side
and as the JavaScript exception message: `not_initialized` for unavailable native
mode, `bad_request` for document/admission rejection, and `internal` for
evaluation/serialization failures. Initializer errors remain plain messages,
matching the ordinary initializer convention. The additions preserve existing
request/response shapes and the ordinary schema handshake.

The result retains the [conditional model's scope](native-skill-hit.md), exact
provenance, unreviewed acquisition status, assumptions and unavailable outputs.
It does not certify DPS, pools, accuracy, affordability, requirements, sustain,
mitigation or CN numeric support. These exports do not activate or publish a
native pack or change ordinary native BuildData/ruleset guards.

Focused coordinator checks (commands are not evidence of a pass):

```sh
mise exec -- cargo test --locked -p pobr-wasm --lib state::tests::
mise exec -- cargo test --locked -p pobr-wasm --test native_api
mise exec -- cargo test --locked -p pobr-wasm --test staged_data
POBR_NATIVE_WASM_PACK=/absolute/path/to/exact-native-pack \
POBR_NATIVE_WASM_REQUEST=/absolute/path/to/saved-native-build.json \
  mise exec -- cargo test --locked -p pobr-wasm --test native_api \
  supplied_actual_pack_saved_request_reopens_and_rechecks_every_call -- --ignored
```

Default host tests reuse the small checked-in native v1 fixture for initialization,
transactionality, strict decoding and ordinary-route guards. The ignored test
requires both explicit actual v2 inputs and fails if either is missing. It checks
the saved-file reopen, complete shared-result serialization, pinned numerical
expectations and rejection after edits or catalog switches. Host tests do not
prove wasm-bindgen export wiring: the parent/coordinator must build with the
pinned mise toolchain and exercise the real JavaScript exports and staged actual
pack in compiled WASM before claiming that boundary passes.
