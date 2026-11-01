# Source-derived special-currency catalog

`crafting_inputs.py` owns PoBR's UI selection and catalog projection. RePoE owns
the neutral seven-table facts, original exported bytes and their publication.
The current `pobr-build-data-lock/v2` requires both the external game/language
parts and this separate crafting package. A missing or corrupt crafting input
fails preparation; the game part must omit `trade_crafting_sources.json`, even
if a processed copy would have identical bytes. The previous-version v1 lock
remains an explicitly selected retained compatibility input.

The crafting ZIP has RePoE format `repoe-native-crafting-facts/v1`. Its immutable
Git address, archive bytes/SHA-256, embedded manifest, expanded size, original
input-manifest pin and producer fingerprints are fixed in the build lock. The
composer verifies every archive member, including the seven original tables,
before reading `crafting.json`. Complete tables never enter PoBR Git or player
payloads. Source acquisition status and target declarations remain unchanged;
the consumer does not newly observe client identity or accept a source. Language
missing from the F77 identity remains unknown; an optional declaration is inherited
without verification. Physical-ID/numeric joins do not infer English from names
or paths. The build receipt preserves this nullable declaration and its unobserved
status.

The neutral producer retains all physical rows and distinguishes `Mod`,
`DisplayMod`, ordered `OutcomeMods` and raw weights. It preserves all eight stat
slots, regular/hardmode endpoints, nulls, zeros and unknown fields. Only selected
internal references between the seven tables are validated. Other foreign
references and complete crafting semantics remain outside that closure.

PoBR applies these existing catalog policies:

- Select `CurrencyVerisiumAlloy*`, `CurrencyPerfectEssence*` and
  `CurrencyCorruptedEssence*` currency IDs. The prefixes and `alloy`/`essence`
  labels belong to PoBR.
- List `OutcomeMods` when present; otherwise list a nonnull `Mod`. This is a
  consumer policy, not a fallback in RePoE or proof of eligibility/probability.
  Never substitute `DisplayMod`.
- Aggregate by source label and exact Mods ID, union the identified item classes,
  and sort the catalog deterministically. Empty classes remain empty, never a
  wildcard. Raw duplicate/order information stays in the input receipt.
- Project every identified regular-stat slot, 1 through 8, without merging
  duplicate stat IDs. Preserve zero and negative endpoints. A selected identified
  stat with missing/inverted endpoints fails; hardmode values never replace the
  regular channel.

The result is a category/identified-regular-stat catalog, **not a complete mod
effect packet**. A null stat reference with nonzero raw endpoints remains a
diagnostic with its physical row, slot and exact values. No stat ID, zero effect
or missing probability is invented. Source diagnostics, selected relationship
provenance, raw weights and projection diagnostics are saved outside the runtime
bundle in:

```
<cache>/build-input-receipts/<build-lock-sha256>-<policy-sha256>.json
```

The receipt binds the exact package, policy source fingerprint, source scope,
inherited identity and output pin. It does not override the snapshot manifest or
source registry. The strict runtime bundle contains only its declared files.

For a separate regeneration candidate:

```sh
mise exec -- python3 pipeline/crafting_inputs.py --patch 0.5.5.5 --check-lock
mise exec -- python3 pipeline/crafting_inputs.py --patch 0.5.5.5 \
  --out "$POBR_CANDIDATE_OVERLAY/trade_crafting_sources.json"
mise exec -- python3 pipeline/test-trade-crafting.py
mise exec -- python3 pipeline/test-build-data.py
```

Output must be outside the canonical `data/` tree. A failed fetch/projection
leaves the prior candidate file unchanged. `regen-all.sh` checks the target before
costly generators and treats crafting generation failure as fatal. Published
builds fetch exact Git blobs; an unpublished local handoff requires an explicit
`--crafting <zip>` (and `--game <zip>` for its paired game part), with the same
byte checks. `--offline` alone never authorizes a local-only package or a fallback.

The bounded 0.5.5.5 comparison selects 89 source rows and produces 92 catalog
records, whose business fields equal the retained baseline. Seven unresolved
outcome/weight alignments are in unselected ordinary currencies and remain in
source diagnostics. `AlloyBallistaLimit1`, Mods row 15531, slot 2, has a null stat
and regular endpoints `-20/-20`; it remains explicitly unrepresented in this
identified-stat catalog. Its primary identified stat is not evidence that the
whole effect packet is owned.

The retained 31,192-byte file, SHA-256
`6153ccd3eee148312fac673c9669896c66c8ab1cf712167a5e836544821a4202`,
still has unknown original execution inputs/patch/realm. Equal business values
do not recover that history. The new output replaces the historical generator
header with accurate source/projection metadata, so its snapshot/runtime
identities change even though the 92 business records do not. The rollback lock
and original source-acceptance entries remain immutable.
