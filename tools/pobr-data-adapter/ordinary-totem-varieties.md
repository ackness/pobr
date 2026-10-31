# Target totem objects from pinned facts

`--ordinary-domain totem-varieties` produces the original
`TotemVarietiesDef` / `TotemVarietyDef` version overlay from six authenticated
physical tables. The existing GameData loader prefers this version overlay to
the shared fallback. BuildData's totem definitions and numerical formulas remain
unchanged; the producer supplies raw source percentages.

```sh
mise exec -- cargo run --locked -p pobr-data-adapter -- \
  --ordinary-facts /absolute/path/to/F \
  --ordinary-domain totem-varieties \
  --facts-manifest-sha256 56e71e00a72c8079d100b64861fc030206b12aaeb5ad655ccb4ab4e6726332d0 \
  --version 0.5.5.5 \
  --out /absolute/path/to/existing-parent/fresh-totem-varieties
```

No policy, codec, old numerical catalog, vendor or Lua input is used. The mode
rejects quality/codec-only flags, duplicate or missing arguments and mixed
raw/native modes. Seven previous ordinary domains and the old entrypoints retain
their behavior and file/receipt shapes, except for true compiled revision.

## Source join

`reader::open` and `Tables::selected` authenticate and validate only
`GrantedEffects`, `ActiveSkills`, `ActiveSkillType`, `SkillTotemVariations`,
`MonsterVarieties` and `MonsterTypes`, with the existing manifest/lock checks.

| Input | Handling |
| --- | --- |
| `GrantedEffects.Id` | Unique output effect key; every matched effect is retained |
| `GrantedEffects.ActiveSkill` | Nullable physical FK to ActiveSkills; null is not row zero |
| `ActiveSkills.Id` | Stable provenance ID |
| `ActiveSkills.SkillTotemId` | Opaque selector matched by value to SkillTotemsKey |
| `SkillTotemVariations.SkillTotemsKey` | Same opaque `enumrow → SkillTotems` namespace |
| `SkillTotemVariations.TotemSkinId` | Signed i32; exactly zero selects the original DTO's skin |
| `SkillTotemVariations.MonsterVarietiesKey` | Required physical FK to the object's variety |
| `MonsterVarieties.MonsterType` | Required physical FK to the object's type |
| `MonsterVarieties.LifeMultiplier` | Existing life_multiplier percentage, unchanged |
| `MonsterTypes.Armour/Evasion/EnergyShieldFromLife` | Existing raw percentage fields, unchanged |
| `ActiveSkills.ActiveSkillTypes` | Ordered refs to ActiveSkillType.Id; diagnostics and provenance only |

`ActiveSkills.GrantedEffect` is a separate string column which may be empty; it
is not read as a reverse FK or an index. Selectors are not variation row numbers,
and neither nonzero nor any particular selector value determines whether an
effect summons a totem. The declared `SkillTotems` namespace is recorded in the
receipt. Its external table content is not available to this six-table domain
and is neither read nor invented. Equality of the two selector values is the
only join performed.

Each selector must have at most one skin-zero variation, even if no GE currently
uses it; ambiguity fails instead of choosing the first. All matched GE aliases
are emitted, including non-player effects and effects without a SummonsTotem tag.
Output is sorted by granted_effect. Other skins stay unapplied source records;
their numbers are not replaced with skin-zero values.

Consumed schemas, physical keys, stable IDs, required fields, integer types and
all non-null references validate before output, including unused rows and other
skins. Ordered tag references retain duplicate occurrences. All physical
LifeMultiplier/Armour/Evasion/EnergyShieldFromLife and skin values must fit their
declared signed i32 schema. Only selected DTO numbers additionally require u32
conversion; unrelated signed source facts may remain negative. Null, fractional,
overflowing or missing values are errors, never clamped or defaulted. No unit,
growth, rarity, uptime or level formula is added.

## Unresolved source and provenance

A tuple tagged SummonsTotem without a skin-zero match remains explicitly
unresolved, with effect/active physical rows and stable IDs, selector and all tag
occurrences. It receives no inferred variety or numerical fallback and is not
declared effectless. Other unmatched effects are grouped by selector with counts
and compressed physical row ranges; null-active effects have their own count and
ranges. Their identities remain accessible in the pinned source without copying
the full GrantedEffects table into the receipt.

Parent source-check `30ec3186efb74b07e0116612be167b933a2e6191fbee2927ee4f0fae2f32c0b7`
found 11 matched target rows, including all eight prior shared rows with the same
fields. It identified one unresolved tagged tuple: `VoodooDollTaunt`, GE row6898,
AS row467 `totem_taunt_triggered`, selector27 without skin0. These are parent
source-probe facts, not worker producer/runtime results or hardcoded Rust rules.
The old shared values and parent expected.json are not producer inputs.

`source_records` records every emitted effect/active/selector/variation/variety/type
row and stable identity, four raw numeric fields and ordered tags.
`variation_records` preserves all variations and labels them matched-skin-zero,
unreferenced-skin-zero or other-skin-unapplied, including signed unused values.
`unconsumed_source` lists remaining columns, payloads and source row ranges.
The common receipt binds actual source/output hashes and compiled conversion/DTO
revision. The overlay `_meta` identifies target F, patch and six payload hashes.

```text
fresh-totem-varieties/
  _domain_candidate.json
  overlay/totem_varieties.json
  source-manifest.json
  source-lock.json
  producer-receipt.json
```

All validation/serialization precedes reservation. The existing directory-handle
writer refuses overwrite, F descendants and production `data/` (including the
shared overlay directory); output must be a fresh leaf under an existing parent.
Marker is first, receipt last. No base layer, complete runtime manifest, CURRENT
change or source promotion. Acquisition remains draft international0.5.5.5,
complete_runtime is false, adoption is not-adopted.

Normal resistances come from the separate monster-resistances domain. Remaining
object identities, skins, level scaling and other minion/totem catalogs are not
certified by this file. Complete target package, ordinary BD/Web, accuracy and
adoption still need separate acceptance.

Synthetic regressions cover original DTO fields/units, selectors versus physical
rows and backlinks, aliases/untagged matches, partial unresolved/null summaries,
skin selection/ambiguity, signed versus selected u32 values, malformed refs/schema/
keys, deterministic replay, guards and all seven previous domains. Worker Cargo,
Rust regressions and actual producer execution are **UNRUN**. Parent uses the
existing warm target and frozen binary for independent full-field/diagnostic
comparison, dual replay, sealing and prior-domain byte checks. No unchanged
core/WASM or full-corpus rebuild is requested by this adapter-only slice.
