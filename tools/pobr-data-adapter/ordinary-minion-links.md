# Ordinary granted-effect/minion links

`--ordinary-domain granted-effect-minions` produces the existing
`GrantedEffectMinionsDef` sidecar. Only `ignore_minion_types` is regenerated from
target facts. Membership, summon relationships, equipment fields and unknown
values remain explicit fixed community compatibility. The ordinary skills
producer already supplies the same boolean for all base effects.

## Inputs and invocation

The existing four codec source/policy flags are required. Branch-codec and
quality flags are rejected, as are duplicate, missing and mixed-mode arguments.

```sh
# Parent's frozen binary; UNRUN by the implementation worker.
/absolute/path/to/frozen/pobr-data-adapter \
  --ordinary-facts /absolute/path/to/F \
  --facts-manifest-sha256 56e71e00a72c8079d100b64861fc030206b12aaeb5ad655ccb4ab4e6726332d0 \
  --version 0.5.5.5 --ordinary-domain granted-effect-minions \
  --codec-source /absolute/path/to/data/0.5.5.4.2/overlay/granted_effect_minions.json \
  --codec-source-sha256 356efcf25802787c52de25e5e34700ebb2cada2d6cafc71895e7fcf41a95e16c \
  --codec-policy /absolute/path/to/pipeline/actors/0.5.5.5-minion-links-policy.json \
  --codec-policy-sha256 ad8f713e7686e9a0a91d510468689ee64313bcb9fba0175b3cf4859df3ef7ce3 \
  --out /absolute/existing/external-parent/fresh-minion-links
```

The initial [policy](../../pipeline/actors/0.5.5.5-minion-links-policy.json) is the
verbatim frozen applicability template. It binds target patch/manifest,
domain/file/schema, one owned field, original source hash/version/metadata,
supported projection and an explicit alias inventory. Unknown policy schema,
duplicate/empty alias IDs, invalid declarations or target mismatch fail.

Only `GrantedEffects` and `MonsterVarieties` plus source-lock are authenticated
payloads: **three**, using `reader::open` and `Tables::selected`. Both table IDs
and `GrantedEffects.IgnoreMinionTypes` are consumed. Physical rows identify
source locations, never alias or business IDs. Column metadata, every table
identity and every effect boolean are checked before reserve.

The policy's `alias_catalog.source_declaration` cites old minions.json as authoring
evidence. That file is **not provided, read or hash-verified by this producer**.
Its path, metadata, hash and role remain declarations; only the supplied policy
bytes and target alias bindings are verified. No vendor/Lua, numeric catalog,
oracle, other candidate or inferred source path is read.

## Mapping and preservation

Every original entry keeps its exact `effect_id` and order. Nonempty unique
effect IDs bind to target `GrantedEffects.Id`; aliases resolve through the
policy to exact `MonsterVarieties.Id`. Whitespace, including trailing whitespace,
is preserved. All `minion_list` and `add_minion_list` occurrences retain order
and multiplicity; aliases may share a variety. Unused aliases are valid inventory
and still bind to the target. `source_catalog_row` is authoring evidence, not a
target row or semantic key.

For each curated entry, target `IgnoreMinionTypes=true` emits
`ignore_minion_types: true`; target false removes the field, following the existing
DTO's default false convention. Both directions of target change are accepted.
No legacy value or default supplies a missing/malformed target boolean.

Every other field is preserved as its original JSON Value, including unknown
nested fields, minion_uses, minion_has_item_set and absent versus empty/default
shape. The original DTO validates shape without reserializing retained content.
`MinionType` remains unconsumed; it does not establish summoning relationships,
equipment semantics, creature eligibility or a new minion model.

Effects outside the curated sidecar get no generated entries. Their base boolean
remains authoritative under the existing loader's matched-entry merge. Receipt
records all outside true IDs/physical rows and compressed outside row ranges.
Parent source inspection observed 181 entries, 34 references and 32 aliases;
150 curated true values and 21 further true values among the 171 target true
effects. These are source observations, not producer results or hardcoded limits.

## Output and receipt

Seven files are written in a fresh external directory:

```text
_domain_candidate.json
overlay/granted_effect_minions.json
legacy-minion-links.json
codec-policy.json
source-manifest.json
source-lock.json
producer-receipt.json
```

Source/policy/F identity bytes are archived verbatim. Original overlay metadata
fields stay intact; added `target_fields`, `fixed_compatibility` and
`ordinary_producer` declarations separate target boolean provenance from original
generator/relationship provenance. Conflicting reserved metadata keys fail.

Receipt records every source/output entry hash, effect physical identity, raw
boolean and projected presence/value, ordered reference/alias/variety bindings,
retained pointers/hashes, all aliases and usage counts, outside true identities,
unconsumed column metadata/full row ranges and actual input/output hashes. The
unprovided authoring catalog is explicitly distinguished from verified inputs.
New producer/policy code and the existing actor DTO participate in the compiled
implementation revision.

Validation and serialization finish before reserve; marker first and receipt
last. Existing data/source/auxiliary-parent/symlink/overwrite protections apply.
The result is draft, `complete_runtime: false`, `not-adopted`, with no runtime
manifest or CURRENT change. Full target package and ordinary BD/Web acceptance
remain separate.

## Parent validation

Focused synthetic regressions are **UNRUN** by the worker: both boolean changes,
outside base values, ordered duplicates/aliases, exact whitespace, retained
unknown/default absence, all binding/schema/pin/policy/path/CLI failures,
seven files/three payloads and prior thirteen-domain stability. Fixtures use
small separate objects, without a giant serde macro.

```sh
mise exec -- cargo test --locked -p pobr-data-adapter --bin pobr-data-adapter ordinary_facts::minion_links::tests
mise exec -- cargo test --locked -p pobr-data-adapter --bin pobr-data-adapter
mise exec -- cargo clippy --locked -p pobr-data-adapter --bin pobr-data-adapter --tests -- -D warnings
```

These commands and real conversions are **UNRUN** by the worker. Parent uses its
single warm target, retains its three historical test corrections, freezes one
binary, performs two actual conversions and an independent all-entry/retained/
receipt comparison, and checks base-plus-sidecar flags across all effects.
Prior thirteen domains require old/new byte comparison, allowing only the true
compiled receipt revision to differ. Source-limited read controls, candidate
sealing and path guards remain parent validation. No unchanged core/WASM/corpus
rebuild is required by this adapter slice.
