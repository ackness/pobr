# Ordinary socketables from RePoE

`--ordinary-domain socketables` writes the existing `RunesDef` overlay for the
ordinary item editor and BuildData. It consumes eight pinned physical tables:
BaseItemTypes, SoulCores, SoulCoreTypes, SoulCoreStats, SoulCoreStatCategories,
SoulCoreLimits, Stats and ItemClasses. No runtime engine or calculation law is added.

## Field ownership

The target facts own membership, names, ordered normal/bonded stat packets,
required levels, socket flags and numeric limits. A versioned policy maps target
item categories to existing slot keys and distinguishes shared limit groups from
per-augment limits. Exact target item-class lists guard those mappings against drift.
Graphical descriptions, unknown fields and unconsumed socket rules remain in the
fact package; they are not inferred as new runtime behavior.

The existing strict CSD renderer selects only global item descriptions. Every
packet of a socketable must render. Otherwise its entire effect text comes from
the separately pinned legacy entry, only when the complete target packet still
matches an explicit policy binding and its slot set agrees. There is no partial
target/legacy effect mixture. Changed unsupported values, unknown categories,
invalid references and missing paired values reject the candidate. This is a
fixed compatibility dependency, not certification that old text applies to the
target patch. Eight unused entries are excluded by exact full-entry fingerprints;
changed or unused exclusions reject. Historical import-name aliases retain the
new target effects and are separately recorded, never counted as target members.

The candidate contains `overlay/runes.json`, the exact compatibility input,
policy and source/implementation receipts. It does not adopt data or change CURRENT.

```sh
mise exec -- cargo run --locked -p pobr-data-adapter -- \
  --ordinary-facts /absolute/F --facts-manifest-sha256 "$FACTS_SHA" \
  --version 0.5.5.5 --ordinary-domain socketables \
  --codec-source data/0.5.5.4.2/overlay/runes.json --codec-source-sha256 "$RUNES_SHA" \
  --codec-policy pipeline/socketables/0.5.5.5-policy.json --codec-policy-sha256 "$POLICY_SHA" \
  --item-descriptions /absolute/native-descriptions/descriptions.json \
  --out /absolute/existing-parent/fresh-socketables
```

Supply the actual external fingerprints; the policy pins the CSD document and
its manifest. [Ordinary fact verification](ordinary-facts.md) applies unchanged.

## Translation-library boundary

`pipeline/native-socketable-translations.mjs` consumes the own translation
repository's pinned `poe2-native-localization-candidate/v1` export. It follows
the exact file/record/branch selected by the socketable receipt and checks source
manifest, stat IDs and formatting handlers. It renders only actual consumed
literal values into the existing `stat_lines.json` pairs. It never flattens the
entire contextual archive into a dictionary or invents Chinese wording.

Existing pairs remain intact. Ambiguous context-free mappings, missing pairs and
unproven multiline alignment are reported and left untranslated. The `Bonded:`
item syntax marker is preserved. This adds selected effect translations, not a
complete target-language approval, name catalog or Chinese-server numeric proof.

```sh
mise exec -- node pipeline/native-socketable-translations.mjs \
  --selection /absolute/socketables/producer-receipt.json --selection-sha256 "$RECEIPT_SHA" \
  --native /absolute/translation-library/fixed-candidate.json --native-sha256 "$LANGUAGE_SHA" \
  --legacy data/0.5.5.4.2/i18n/zh-CN/stat_lines.json --legacy-sha256 "$LINES_SHA" \
  --out /absolute/existing-parent/fresh-translations
```

Both commands require fresh outputs. Assembly remains explicit and records the
new snapshot/shared-layer identity before ordinary consumption. Regression scope:

```sh
mise exec -- cargo test --locked -p pobr-data-adapter --bin pobr-data-adapter ordinary_facts::socketables::
mise exec -- node --test pipeline/test-native-socketable-translations.mjs
```
