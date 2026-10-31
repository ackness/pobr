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
`translateLinesToZhCn` also returns `not_initialized` in native mode; its ordinary
missing-template and pre-initialization pass-through behavior is preserved.

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

The ordinary-damage-passive profile uses the same exports and result fields.
It requires an explicitly assembled new candidate and a document bound to that
candidate's actual six dataset fields and new policy hash. The existing
`build.tree.allocated_nodes` array selects graph instances; the original no-allocation profile
continues to reject allocations. Selection is all-or-nothing, with no text fallback,
free starting grants or class-name/tree-label selection. Selected nodes are
assumed active for conditional comparison; the returned `allocation-legality`
reason leaves point budget, reachability and progression unverified. Node-specific
origins and exact packet pointers reuse existing modifier evidence and selection
sources. `counted` records actual nonzero aggregation input, not marginal damage.

The primary-spell profile also uses these exact exports and JSON function
signatures. The global WASM handshake stays 16. It requires its own immutable
candidate, policy hash and `conditional-skill-hit-v2` evaluation; it keeps the v1
document envelope and reruns exact selection/level/passive admission on every call.
Edit all stable skill and level mirrors together, with empty prepared requirement
levels and neutral config. The initial inventory covers Spark, Fireball and
Frostbolt primary hits at exact authored levels with actor7 level1 unchanged.
Ordinary mode, default API dispatch and translation behavior are unchanged.

V2 serializes unavailable count/lifetime as null with explicit reasons. Clients
must branch on the evaluation and preserve nulls. Projectile classification cannot
supply a missing count, and Frostbolt's retained duration is not a certified
projectile lifetime. Historical v1 requests still require their complete numeric
intrinsic outputs; old/new tuple mixing rejects.

Calculation errors use the existing JSON error envelope on the Rust `Err` side
and as the JavaScript exception message: `not_initialized` for unavailable native
mode, `bad_request` for document/admission rejection, and `internal` for
evaluation/serialization failures. Initializer errors remain plain messages,
matching the ordinary initializer convention. The opt-in evaluation distinguishes the new nullable intrinsic semantics; historical
v1 results and the ordinary schema handshake remain unchanged.

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

The separate ignored new-profile host case keeps the old baseline case intact:

```sh
POBR_NATIVE_PASSIVE_PACK=<new-immutable-pack> \
POBR_NATIVE_PASSIVE_REQUEST=<new-bound-baseline-document> \
  mise exec -- cargo test --locked -p pobr-wasm --test native_api \
  supplied_actual_passive_request_edits_stage_reopen_and_reject_whole -- --ignored
```

It checks actual saved allocations, distinct equal-stat nodes, four applicable
scopes, remove/reopen, directory/staged equality, rejected mixed selections,
stale pins, a corrupt staged packet and native translation/catalog-switch guards.
This remains a host test; the rebuilt JavaScript boundary and complete parsed
CLI-to-WASM equality are coordinator acceptance work.

The primary-spell host-wrapper case is independently opt-in:

```sh
POBR_NATIVE_PRIMARY_SPELL_PACK=<new-immutable-pack> \
POBR_NATIVE_PRIMARY_SPELL_REQUEST=<new-bound-document> \
  mise exec -- cargo test --locked -p pobr-wasm --test native_api \
  supplied_actual_primary_spell_edits_stage_reopen_and_preserve_failed_load -- --ignored
```

It compares directory/staged host results with the shared API across selected
skills and exact levels, rejects old evaluation, and checks that corrupt staged
projection bytes preserve the previous active native catalog. It does not execute
wasm-bindgen or prove JavaScript export equality; rebuilt public WASM and real
CLI/WASM proof remain coordinator work after integration.
