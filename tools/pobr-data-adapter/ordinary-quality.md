# Ordinary quality from target facts

`--ordinary-domain gem-quality` extends [ordinary facts](ordinary-facts.md) with
one overlay file in the original `GemQualityStatsDef` shape. The existing
quality converter supplies the effect-wide model, ordered main/alt vectors and
permille-to-rate conversion. The runtime consumer, alt/Gemling gating and
`trunc(rate * quality)` remain unchanged.

```sh
mise exec -- cargo run --locked -p pobr-data-adapter -- \
  --ordinary-facts /absolute/path/to/F \
  --ordinary-domain gem-quality \
  --facts-manifest-sha256 56e71e00a72c8079d100b64861fc030206b12aaeb5ad655ccb4ab4e6726332d0 \
  --quality-policy pipeline/gem-quality/0.5.5.5-facts-policy.json \
  --quality-policy-sha256 334907c02b15e2303f9a6f550dfedaf80d889046048fe97d309ceba2182848da \
  --version 0.5.5.5 \
  --out /absolute/path/to/existing-parent/fresh-quality-candidate
```

Both policy arguments are required for this domain and rejected for every other
domain. Unknown/repeated options, missing values, a wrong external manifest or
policy pin, and target patch/manifest disagreement fail before output reservation.
The policy hash is checked before JSON parsing; the shared strict JSON decoder
also rejects duplicate object keys, including repeated exclusion IDs. The policy
and nested target/reference/applicability/compatibility records reject unknown
fields; opaque archival `reference.provenance` retains the previous receipt's
source description. Policy fingerprints are lowercase hexadecimal, and the
existing enabled/excluded classification must be nonempty, disjoint and complete
for the target's quality owners. New or disappeared owners require a corresponding
explicit policy input. No effect list is embedded in Rust.

## Numeric source and model reference

Only `tables/GrantedEffects.json`, `tables/Stats.json` and
`tables/GrantedEffectQualityStats.json` are selected through `reader::open` and
`Tables::selected`, alongside the existing envelope/lock validation. No gem table,
vendor, old numeric output, language pack or download is consulted.

The new policy schema is `pobr-ordinary-quality-policy/v1`. Its `target` binds the
exact F manifest and `0.5.5.5`; `compatibility` carries the previous effect-wide
scope decision, enabled IDs and explicit exclusion reasons. `reference` retains
the original `0.5.5.4.2` source receipt identity, provenance, raw input fingerprints
and baseline hash. Those paths/hashes are evidence labels and are never opened
by this producer. The vendor reference remains
`ce566eac45ea8a86477f513c7ee65a1ebe60014e` /
`583415d9d5a49699c2581f3092dcb45ebb81b9fa0c9ad695b207a7f58f5fa85d`.

`target_applicability` records the coordinator's independent source comparison
(`cbf23c7ae7161f46435884c701e398f460b98d2bc377f173ee22b372adac5039`):
429 target owners match 410 enabled and 19 excluded IDs, enabled ordered
stat/rate vectors and all main/alt scope arrays match the previous baseline.
That evidence supports reuse of this existing model policy for this F; it is
not producer validation, independent game-rule proof, applicability of other
assets or acquisition approval. All newly produced numbers come from target F.

## Consumed fields and pending facts

| Source | Existing output / handling |
| --- | --- |
| `GrantedEffects.Id`, `Stats.Id` | Stable IDs joined through verified physical `_row`; neither an imported `_index`, a name nor ID sorting supplies the reference key |
| `GrantedEffectQualityStats.GrantedEffect` | Required owner reference, unique per quality table; every owner must have a policy decision |
| `Stats` / `StatsValuesPermille` | Equal-length arrays; stat FK resolves to `Stats.Id`; signed i32 value divided by 1000.0 |
| `AltStats` / `AltStatValuesPermille` | Same conversion, appended after main entries with `alt: true` |
| `ApplyToStatSets` / `AltApplyToStatSets` | Original ordered arrays retained under `_meta.unapplied_stat_set_scopes` and in the receipt; no set application or FK lookup |

Repeated stat references, repeated scope positions, zero and negative rates are
preserved. `IsSupport` and gem membership do not filter owners, so enabled
non-gem supports remain. Required metadata, physical/stable keys, consumed value
types and FKs are checked even for excluded rows; filtering cannot hide errors.
The fixed F has integer scopes (including zero), with no null array/element.
The source scopes are declared i32 and the existing quality metadata uses u32
positions: invalid null, negative or out-of-i32-range positions are rejected,
never changed to zero. Empty scope arrays retain the existing representation:
an owner is omitted from the scope map only when both arrays are empty; its
original arrays remain in F, identified by `conversion.owners`.

Excluded owners retain reason, quality row, effect row and the seven consumed
raw fields in `conversion.excluded_effects`; this is a compatibility gap, not a
claim of no effect. `conversion.owners` maps every quality row to its stable
effect ID and effect source row. `unmapped_source_columns` lists unconsumed
column metadata, pinned payload, row range and identity lookup. In particular,
`AddTypes` and `AddMinionTypes` remain unapplied and traceable in F; their foreign
keys are not certified by this three-table conversion. No second mechanism or
new consumer model is introduced.

## Candidate and implementation boundary

```text
fresh-quality-candidate/
  _domain_candidate.json
  overlay/gem_quality_stats.json
  source-manifest.json
  source-lock.json
  quality-policy.json
  producer-receipt.json
```

`quality-policy.json` is the supplied byte sequence unchanged. Runtime `_meta`
separates target source/payload identities from old model references. The receipt
hashes the actual selected payloads, source sidecars, policy and overlay, and
binds the compiled implementation revision. The marker is written first and
receipt last. The existing directory-handle writer protects both base and
overlay layers and detects replacement; output must be fresh and outside F,
production `data/` and the policy's canonical parent directory. The output
parent must already exist. No complete `manifest.json`, adoption, CURRENT change
or source promotion occurs: acquisition remains draft, `complete_runtime: false`,
`adoption: "not-adopted"`.

The raw `--gem-quality` wrapper keeps its original source receipt schema/version/
hash guards, output metadata and file behavior. Both entries call the extracted
`skills::quality::{validate_compatibility,index,convert_rows}` helpers. Only new
physical input performs the additional column/source representation checks.
Other ordinary domains emit their original files and receipt fields; their
compiled revision changes because it fingerprints the shared implementation.

Coordinator verification (worker authored the regressions but did not execute
Cargo or any producer):

```sh
mise exec -- cargo test --locked -p pobr-data-adapter --bin pobr-data-adapter ordinary_facts::quality::tests
mise exec -- cargo test --locked -p pobr-data-adapter --bin pobr-data-adapter skills::quality::tests
# Coordinator may combine these and prior-domain checks in the adapter's one warm-target suite.
```

The synthetic fixture asserts original DTO values and raw-wrapper equivalence,
ordered duplicates, exclusions, metadata, source/policy/CLI failure boundaries,
directory replacement, deterministic replay and isolation of the three previous
domains. Actual F output vectors, excluded records, scope arrays and prior-domain
byte compatibility remain for the coordinator to verify with a fixed binary.
