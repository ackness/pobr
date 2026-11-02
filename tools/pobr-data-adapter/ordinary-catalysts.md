# Ordinary catalysts and supplemental physical tables

The maintained RePoE `tables:candidate` command exports explicit selected DAT
resources using the existing pinned decoder and schema. Its externally pinned
`repoe-native-tables/v1` manifest contains raw-resource identities, exact physical
tables, the proposed source lock and unresolved foreign-table dependencies.
It does not download data or promote acquisition status.

The ordinary adapter's `--supplement` and `--supplement-manifest-sha256` accept
this envelope only alongside the fixed parent facts. Producer, schema, decoder,
game, patch, realm, source declaration and every payload fingerprint must agree.
Joins resolve actual physical rows against these pinned packages, never row
numbers from a different patch. No existing fact package is relabelled or extended
in place. The supplement is supported by catalysts and v2 ordinary tree refresh.

```sh
mise exec -- cargo run --locked -p pobr-data-adapter -- \
  --ordinary-facts /absolute/F --facts-manifest-sha256 "$FACTS_SHA" \
  --version 0.5.5.5 --ordinary-domain catalysts \
  --supplement /absolute/selected-tables --supplement-manifest-sha256 "$SUPPLEMENT_SHA" \
  --catalog-policy pipeline/policies/catalysts/0.5.5.5-policy.json --catalog-policy-sha256 "$POLICY_SHA" \
  --out /absolute/existing-parent/fresh-catalysts
```

`AlternateQualityTypes` and `ModEffectStats` join parent `BaseItemTypes`,
`ItemClasses`, `Stats` and `Tags`. Target facts own names, quality descriptors,
tags and currency relationships. The existing `CatalystsDef` needs one definition
per quality type: the policy supplies only historical XML ordinal identities.
All ordinary/refined currencies, class restrictions, raw flags and multiplier
values remain in the receipt. A new effect without a codec identity, changed
unrepresentable scope or invalid reference rejects the candidate.

This changes catalog production, not catalyst calculation. Raw `Multiplier`
units do not establish a new scaling law; existing imported item values and core
behavior are unchanged. The output uses the existing `overlay/catalysts.json`,
plus policy/source/implementation receipts and the normal domain guard. It does
not seal a full runtime package or change CURRENT.

Focused checks:

```sh
mise exec -- cargo test --locked -p pobr-data-adapter --bin pobr-data-adapter ordinary_facts::catalysts::
mise exec -- cargo test --locked -p pobr-data-adapter --bin pobr-data-adapter ordinary_facts::passive_tree::ascendancy::
```
