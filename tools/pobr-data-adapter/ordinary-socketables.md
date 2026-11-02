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

The shared [strict CSD renderer](ordinary-passive-tree.md#complete-text-and-compatibility)
selects only global item descriptions, with the same checked unit formatting and
English-equivalent descriptor handling used by ordinary passive text. Every
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
  --codec-policy pipeline/policies/socketables/0.5.5.5-policy.json --codec-policy-sha256 "$POLICY_SHA" \
  --item-descriptions /absolute/native-descriptions/descriptions.json \
  --out /absolute/existing-parent/fresh-socketables
```

Supply the actual external fingerprints; the policy pins the CSD document and
its manifest. [Ordinary fact verification](ordinary-facts.md) applies unchanged.

## Translation-library boundary

Selected effect translations for socketables come from the translation
repository's own publication; PoBR does not render or pair them. Existing pairs
remain intact; ambiguous, missing or unproven multiline mappings stay
untranslated, and the `Bonded:` item syntax marker is preserved.

The producer requires a fresh output. Assembly remains explicit and records the
new snapshot/shared-layer identity before ordinary consumption. Regression scope:

```sh
mise exec -- cargo test --locked -p pobr-data-adapter --bin pobr-data-adapter ordinary_facts::socketables::
```

New selection receipts include Rust-formatted numeric literals. Translation
selection reuses these exact literals in both library templates after checking
source identity, stat IDs and handlers; it does not duplicate the unit conversion
in JavaScript. Historical integer-only receipts remain readable.
