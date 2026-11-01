# Native display catalog consumption

`native-display-texts.mjs` projects the independent translation library's pinned
`poe2-native-catalog/v1`, `/v2` or `/v3` into `i18n/<locale>/display_texts.json`. It calls the
library's `createCatalogLookup` for every exact table/key/field/current-English
query. The request pins the catalog manifest and the closed library
implementation; both payloads, source modules and inputs are checked before
publication into a fresh repository `.cache/` directory.

Transfer data and evidence through Git; thread messages need only commit IDs,
paths and hashes. Input JSON has no fixed byte cap. Declared byte counts, actual
file sizes and SHA-256 values must still match, including the final input recheck.

```sh
mise exec -- node pipeline/native-display-texts.mjs \
  --request /absolute/path/request.json --out .cache/fresh-display-candidate
```

The request has `schema: "pobr-native-display-texts-request/v1"`, exact
`game_patch`, `realm`, `locale`, `catalog: {manifest, sha256}`, and
`library: {root, files}`. `files` contains SHA-256 values for
`src/native-catalog.mjs`, `src/native-names.mjs`, and `src/strict-json.mjs`.
The binding-aware library also requires `src/native-catalog-binding.mjs` and
`src/native-localization.mjs`. A v2 catalog requires this complete five-module
closure; v1 supports its original three-module library. Both also support newer
closed libraries. The v3 library requires six modules, adding
`src/native-catalog-fk-binding.mjs`; its module hashes must equal the manifest's
producer closure. V1/V2 retain their existing independently pinned compatible
lookup libraries. Manifest and catalog versions must agree. V2/V3 target identity remains
separate from its unknown observed-client patch and realm.
V3 additionally requires the original `verification.json` bytes, pinned by both
the catalog and its manifest. Those bytes go to `createCatalogLookup` as
`expected.verification`; the library recomputes strict and FK bindings before
any runtime projection. Missing or altered verification evidence rejects the
entire import. It is a build input, never a player asset.
The library retains ownership of pairing, target applicability and diagnostics.
This adapter does not read client data, infer patch/realm, convert Chinese
scripts or apply numeric transformations.

## Optional reviewed-only supplement

A request may carry `supplement: {manifest, bytes, sha256, module: {file, bytes,
sha256, revision}}`. It is absent unless the declarative importer declares
`translation.supplement.mode = "reviewed-only"`, and it requires a v3 catalog.
`manifest` pins the compact `poe2-reviewed-native-supplements-manifest/v1`
package shipped by the translation library's `workbench-export`; `module` pins
the library's own `src/translation-workbench.mjs` at its own Git revision. The
official six-module `library.files` closure and the catalog manifest producer
stay exactly as before: the workbench module is never mixed into them. Instead
the supplement manifest's seven-module producer must equal those six official
hashes plus the pinned workbench hash.

The external pins are the supplement manifest (`bytes`, `sha256`), the module,
and the `supplements.json` identity declared by that manifest. All three are
regular, symlink-free files that are read once and re-verified at the end like
every other input. With a verified package the producer builds
`createReviewedSupplementLookup([{bytes, sha256, verification}], {bytes,
sha256})` from the original catalog Buffer, its manifest SHA-256 and the original
`verification.json` Buffer; the library keeps the official catalog lookup as its
internal fallback, so the 335 MB sidecar is validated once, not twice. Without a
`supplement` field the request keeps the existing official-only
`createCatalogLookup` path unchanged.

Every exact row query carries its complete English `context` object so the
library can match `table/field/id/current-English/context` exactly; nothing here
guesses a context, converts a script, approves a draft or invents a record. The
library rejects unreviewed, cross-scope, stale or foreign records as a whole, and
the producer publishes nothing in that case. An empty register is a valid input
and leaves `entries`/`scopes` identical to the official projection; supplement
provenance (source inventory, record count, module and runtime pins) is recorded
only in `receipt.json`. The compact package, the workbench module and the large
catalog sidecar remain build inputs: they are never copied to the runtime output,
a player download or the WASM binary.

The display-only sidecar has two views:

- `scopes["Table.Field"][stableKey] = [currentEnglish, translatedOrNull]`
  contains the indexes declared by `translation.scopes` in
  [`data-requirements.json`](data-requirements.json), currently the three used by `nativeDisplayNames.ts`:
  `BaseItemTypes.Name`, `Characters.Name`, and `Ascendancy.Name`. Unresolved
  records in those indexes remain null. The other fields have no current
  exact-ID browser caller and stay in the translation library rather than
  inflating every player's download. Add a scope when its caller is implemented.
  These keys do not create new calculation or DTO identities.
- `entries[currentLiteral] = translatedOrNull` supplies the existing Web
  `translateLines` path. Every matching record across tables must agree.
  Reference-markup display aliases participate in that same collision check.
  Unresolved or conflicting records yield null; they never pick the first row.
  A rejected v2 group can have a null English field: each of its candidate
  English values and markup aliases still inserts a null blocker. Another ID
  with that text cannot hide the rejected group or enable old dictionary fallback.
  Numeric placeholders, hash slots and unknown syntax remain in the library and
  decision receipt; they require an explicit renderer instead of this lookup.

An explicit request can supply `projection: {scopes, literals}`. The only
supported literal policy is `all-catalog-conflict-participants`: selecting scopes
never removes unresolved/colliding records from the literal ambiguity check.
Without this field, the producer reads and pins the repository requirements file.
The [declarative Git importer](data-imports.md) creates this request from the
requirements and a separate immutable source lock.

The ordinary backend reads only sidecars listed by its snapshot manifest and
checks their version and locale. A native literal takes precedence over the old
display dictionaries. Null means return the original English; absent literals
retain the existing compatibility behavior. A catalog for one locale is never
converted or relabelled as another locale. Original build/item text, stable IDs,
WASM responses and calculation inputs remain unchanged.

The compact JSON sidecar contains display data only. Source tables, physical
columns, provenance, binding witnesses, authoring queues and agent review history
are offline inputs; they are not browser downloads or embedded in the WASM binary.
The receipt preserves every library decision and all unscoped collisions. Its
scope differs from the library's per-table collision count. Generating this
sidecar or exercising `translateLines` does not establish that every field has a
visible UI surface: scoped skill/class/flavour metadata needs an explicit caller.
The existing three ID-keyed TW name sidecars and CN class dictionary remain
separate contracts; this adapter does not overwrite them.

Include a candidate in a new sealed snapshot and bind its manifest/consumer
identity when validating it. Do not copy it into an active snapshot or change
`CURRENT` as a side effect of generation. Source approval, source-client identity
and final adoption remain separate decisions.
