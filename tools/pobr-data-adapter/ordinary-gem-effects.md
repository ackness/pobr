# Ordinary gem/effect links from target facts

`--ordinary-domain gem-effects` derives every normal gem's relationship tuple
from the pinned target physical facts. An explicitly pinned legacy codec supplies
display order and existing primary-owner precedence. Every legacy relationship
must match target facts exactly; this mode has no mismatch exceptions or remaps.

```sh
mise exec -- cargo run --locked -p pobr-data-adapter -- \
  --ordinary-facts /absolute/path/to/F \
  --ordinary-domain gem-effects \
  --facts-manifest-sha256 56e71e00a72c8079d100b64861fc030206b12aaeb5ad655ccb4ab4e6726332d0 \
  --codec-source data/0.5.5.4.2/overlay/gem_effects.json \
  --codec-source-sha256 6bee113b2068e80caece3d15db1c521d98f507c56c4726f5b927d9f259bd4f13 \
  --codec-policy pipeline/skill-codec/0.5.5.5-gem-effects-policy.json \
  --codec-policy-sha256 483cdb9d48c194e5a0277fa571bcda699e114babbf69a20600ee0ecc884d57ef \
  --version 0.5.5.5 \
  --out /absolute/path/to/existing-parent/fresh-gem-links
```

All four codec options are required. They are accepted only by this domain and
stat-set-codec; the two domains have distinct policy schemas. Quality-only flags,
duplicate options, missing values, unknown domains and mixed raw/native modes
are errors. Default Stats/Mods and the other ordinary domains retain their
existing outputs and receipt shapes, apart from the true compiled revision.

## Source relationships and validation

The existing `reader::open` and `Tables::selected` authenticate exactly
`BaseItemTypes`, `SkillGems`, `GemEffects`, `GrantedEffects` and
`GrantedEffectStatSets`, plus the source lock. No old numeric base, vendor Lua,
prepared overrides, language file or unselected payload is opened. Physical
`_row` is a validated foreign key, distinct from stable ID and any `_index` value.

| DTO field | Target source |
| --- | --- |
| `gem_id` | `SkillGems.BaseItemType` → `BaseItemTypes.Id` |
| `variant_id` | Single `SkillGems.GemEffects` → `GemEffects.Id` |
| `granted_effect_id` | `GemEffects.GrantedEffect` → `GrantedEffects.Id` |
| `additional_granted_effect_ids` | `GemEffects.AdditionalGrantedEffects` → ordered `GrantedEffects.Id` occurrences |
| `additional_stat_set_ids` | Primary effect's `AdditionalStatSets` → ordered `GrantedEffectStatSets.Id` occurrences |
| `granted_effect_display_order` | Explicit legacy codec only, after all four relationship fields match |

The original `is_placeholder(Name)` rule excludes empty, `[DNT`, `[UNUSED` and
`[OLD` names. Null `SkillGems.BaseItemType` remains the original no-base dev
placeholder. Names must otherwise be strings. Every consumed column's metadata,
value and non-null physical reference is checked, even on excluded records or
unreferenced variants/effects. A normal gem requires exactly one variant and a
non-null primary; multiple variants fail the existing single-variant DTO limit.
An unused/placeholder variant may have a null primary, which is never converted
to physical row zero. Duplicate physical/stable table identities or repeated
base gem identities fail. Ordered additional references may repeat; no deduplication
or positivity rule is introduced. Unknown names, support and dummy-looking
effects have no extra filter.

## Explicit compatibility policy and output order

`pobr-ordinary-gem-effects-policy/v1` binds target patch/F manifest, legacy
snapshot version/path/SHA/full `_meta`, parent source-check evidence and the
`base-first-legacy-owner-overlay-last/v1` consumer precedence decision. Its
reference paths and baseline hashes are evidence labels only, never implicit
producer inputs. Policy SHA and source SHA are verified before parsing; the
source pin must also equal the policy pin. The shared strict JSON decoder rejects
duplicate keys. Unknown policy fields/schema, malformed fingerprints, target or
metadata mismatches and duplicate codec gem/primary identities fail.

Each codec row's variant, primary, ordered additional effects and ordered
additional stat sets must exactly match the newly derived target row. Display
indices must be distinct and in range over primary plus additional effects.
Partial orders are valid: do not fill, sort or infer omitted positions. Empty
orders retain the current consumer fallback. The legacy file is preserved
verbatim as a separate sidecar, including whitespace, all rows and original
metadata; the target overlay has its own F provenance.

BuildData first visits base gems sorted by stable ID and keeps the first owner
for each primary; legacy overlay rows then override that owner. To retain this
behavior while emitting all target gems, the producer chooses the first sorted
gem ID for each primary, then applies the unique legacy owner when present.
It emits all non-preferred gems first, then all preferred gems, sorting each
partition by gem ID. The unchanged overlay last-wins lookup therefore keeps the
same primary owner, while GameData's by-gem lookup retains every row. Output is
deliberately not globally sorted by gem ID. Core/DTO/loader/selection code is
unchanged; consumers must retain file order for primary-owner insertion.

Parent source-check `e9cc49b9f7d6799f3794f9d745683a54b6314896ebd6b2103f3f0f61e581bc4d`
found all 966 legacy tuples matching target F, 24 display orders (five partial),
and 1,120 normal target gems including 154 absent from the legacy codec. Four
primaries are shared, including the 45 retained `DoLiterallyNothingPlayer` gems.
These are source-comparison facts, not worker producer/runtime results. Rust
contains no corpus counts or skill-ID lists. Legacy provenance remains version
`0.5.5.4.2`, vendor `bb52d6b368307457eb9c54bb13f1829993d390b1`; F remains
draft international `0.5.5.5` acquisition.

## Candidate files and receipt

```text
fresh-gem-links/
  _domain_candidate.json
  overlay/gem_effects.json
  source-manifest.json
  source-lock.json
  legacy-gem-effects.json
  codec-policy.json
  producer-receipt.json
```

All validation and serialization precede output reservation. Output must be fresh,
outside production `data/`, F and both supplemental input parents, with an
existing parent. Existing directory-handle protection writes the marker first
and receipt last. No runtime manifest, CURRENT update, source promotion or
implicit vendor regeneration occurs. `complete_runtime` is false and adoption
is `not-adopted`.

The receipt identifies actual payload/policy/codec/output hashes and compiled
revision. `source_records` pairs each output row with its SkillGems row, base,
variant, primary and every ordered additional reference position and target row/ID.
`legacy_tuple_matches` locates source codec rows and retained display arrays.
`shared_primary_owners` lists each collision's sorted members, preferred owner
and reason. `filtered_placeholders` records original exclusions, and
`unconsumed_source` preserves column metadata, payload and row range for fields
outside this link domain. Historical comparison, legacy compatibility and target
raw facts remain distinct. Prepared mechanics are not declared effectless.

Synthetic regressions cover the original DTO, new gems, shared owners, reference
occurrences, partial orders, all tuple mismatch fields, bad metadata/FKs/keys,
pins/CLI/output isolation, replay and the five prior domains. Worker Cargo and all
producer execution are **UNRUN**. Parent validation uses the existing warm target,
then a fixed binary for independent full field/owner comparison, dual replay and
old-domain byte comparison. Complete target package, ordinary BD/Web behavior,
prepared assets, later-exporter compatibility and adoption remain separate work.
