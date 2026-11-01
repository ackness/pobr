# Ordinary minion and spectre catalogs

`--ordinary-domain minions` and `--ordinary-domain spectres` use one shared actor
producer and the original `MinionsDef` / `MinionEntryDef` schemas. The v1 policies replace 22
owned fields with target facts while retaining authored membership, aliases and
all other raw JSON fields as fixed community compatibility. This is a catalog
migration, not a new creature-admission rule, minion model or gameplay-law claim.

## Explicit inputs

Each mode requires the existing four codec source/policy flags. No new flag is
introduced; branch-codec and quality flags are rejected. Duplicate, missing-value
and cross-domain options fail. The two initial policies are copied verbatim from
the frozen applicability templates:

| Mode | Legacy source SHA256 | Policy SHA256 |
| --- | --- | --- |
| minions | `338aa6da92aa85143b3e4f0478c9c498ad52a154826ed1c15754573dee20799a` | `9fda2d0b8293e1d04c65684cdc486b5625ad8f22c8d3db40e7356465245c2efe` |
| spectres | `50b98756c340cadbaba1ff5d503e79cc06e744d5a100719941c6e1fc765fdd94` | `24118a1eebc7d46e03887f2332c983c6143c9f27ce265129ec70a3803336ead0` |

Policies: [minions](../../pipeline/actors/0.5.5.5-minions-policy.json) and
[spectres](../../pipeline/actors/0.5.5.5-spectres-policy.json). They bind the mode,
file/schema, variety identity field, 22-field projection version, original source
hash/metadata/version/path and exact target patch/manifest. Archived projection
reference hashes describe the fixed recipe; neither those files nor evidence
reports are read by the producer. No vendor/Lua, default numeric table, parent
oracle, other candidate or implicit source path is consulted.

```sh
# Coordinator's frozen binary; UNRUN by the implementation worker.
/absolute/path/to/frozen/pobr-data-adapter \
  --ordinary-facts /absolute/path/to/F \
  --facts-manifest-sha256 56e71e00a72c8079d100b64861fc030206b12aaeb5ad655ccb4ab4e6726332d0 \
  --version 0.5.5.5 --ordinary-domain minions \
  --codec-source /absolute/path/to/data/0.5.5.4.2/overlay/minions.json \
  --codec-source-sha256 338aa6da92aa85143b3e4f0478c9c498ad52a154826ed1c15754573dee20799a \
  --codec-policy /absolute/path/to/pipeline/actors/0.5.5.5-minions-policy.json \
  --codec-policy-sha256 9fda2d0b8293e1d04c65684cdc486b5625ad8f22c8d3db40e7356465245c2efe \
  --out /absolute/path/to/existing-parent/fresh-minions
```

For spectres use `--ordinary-domain spectres`, its source/policy paths and the
pins in the table, with another fresh output leaf. Outputs cannot be within F,
either auxiliary parent or production data, or reuse an existing file, directory
or symlink. The existing Output directory-handle protections apply.

`reader::open` authenticates exactly MonsterVarieties, MonsterTypes,
MonsterResistances, Tags and GrantedEffects, plus source-lock: **six selected
payloads**. `Tables::selected` validates physical `_row`/stable IDs. Auxiliary pins
precede strict JSON parsing. Required consumed column metadata, exact signed i32s,
finite f32-range AttackCrit and every used FK are validated before output reserve.
Physical rows are source connection keys, never catalog IDs or tag/skill identity.
Source IDs use nonempty uniqueness and exact string matching, including whitespace;
they are not trimmed or treated as canonical policy tokens. An unreferenced source
ID remains valid, and an exact catalog/skill reference preserves its spelling.

## Owned projection and retained interpretation

| Output | Target source / fixed recipe |
| --- | --- |
| Catalog binding | Minions require `monster_variety_id`; spectres use `id`, with any supplied `monster_variety_id` required to agree. Unique catalog IDs; aliases sharing a variety are allowed. |
| name | MonsterVarieties.Name, including a real empty string |
| monster_tags | Every ordered Tags reference → Tags.Id, including duplicates |
| life / damage | LifeMultiplier / 100; DamageMultiplier / 100 |
| damage_spread | MonsterTypes.DamageSpread / 100 |
| attack_time | AttackSpeed / 1000, from declared milliseconds |
| attack_range / base_movement_speed | MaximumAttackDistance / MovementSpeed unchanged |
| crit_chance | `percentage = AttackCrit * 100`; `(percentage * 100 + 0.5).floor() / 100`. Negative halfway ties go toward positive infinity. No f32 recast, probability cap or Rust `round`. |
| armour / evasion / energy_shield | Type Armour / Evasion / EnergyShieldFromLife divided by 100 when nonzero; omit when raw zero, preserving existing optional semantics |
| Four normal resistances | First MonsterTypes.MonsterResistances reference, first element of Fire1/Cold1/Lightning1/Chaos1 |
| Four companion resistances | Same first reference, first element of Fire3/Cold3/Lightning3/Chaos3 |
| spectre_reservation | With `x = ExperienceMultiplier / 100.0`, `(x.powf(0.75) * 50.0).floor()` |
| companion_reservation | `((x.sqrt() * 100.0).floor() / 100.0) * 30.0`, in exactly this order |

Signed/zero integer facts are valid, except negative ExperienceMultiplier cannot
enter real sqrt/pow. AttackCrit must be finite and within f32 range; its JSON
spelling need not equal widened f32 bits. The selected resistance reference and
selected arrays must be nonempty. All referenced resistance rows and every
consumed array element are validated, including later unapplied occurrences.

The actor recipe deliberately keeps the archived first-reference projection.
The separate [normal resistance producer](ordinary-monster-resistances.md) still
sums all its references, and its runtime injection is unchanged. Multiple refs,
extra array elements, other rarities and unconsumed columns remain explicit in
receipts; they are not declared effectless or merged under a new rule.

Under v1, original accuracy, base_damage_ignores_attack_speed, skill_list, mod_list,
weapon/category/location/limit/extra_flags and every other non-owned field remain
raw Values, including nested unknown fields and absent-vs-default shape. The
existing DTO validates source/output shape, but never reserializes these retained
entries. MonsterTypes.Flag0 is not interpreted to replace the retained flag.
Every ordered skill_list occurrence must resolve to target GrantedEffects.Id;
unknown skills fail instead of being dropped or replaced by a primary fallback.

Parent source inspection observed 32 minions / 617 spectres, all variety/skill IDs
present; 630 / 12,319 owned output fields and 84 / 2,694 retained skill occurrences.
Companion reservation has 1 / 137 f64-tail differences from old numeric text under
the exact formula. These are source observations, not producer results, law
certification or Rust count limits. The 154 spectres with multiple resistance refs
retain the archived projection plus complete reference evidence.

## Output and receipt

Each mode emits seven files:

```text
fresh-catalog/
  _domain_candidate.json
  overlay/minions.json       # spectres.json for that mode; still a minions array
  legacy-actors.json
  codec-policy.json
  source-manifest.json
  source-lock.json
  producer-receipt.json
```

Original source/policy and F manifest/lock bytes are archived verbatim. The
catalog's `_meta` separates target fields, archived recipe references and legacy
membership/field compatibility; original patch0.5.5.4.2/bb52 identity stays in the
archive/provenance. Authored catalog order and every non-owned field remain.

Per-entry receipt evidence includes source row/key/alias and hashes; raw variety
and type fields; ordered tag/resistance refs and arrays; first selected indices
and unapplied ranges; all 22 output values or omission states; retained JSON
pointers/hashes and each skill occurrence's physical row. Mapping/formulas,
unused column metadata/table ranges, actual source/output/policy hashes and the
compiled source-content revision are included. Parsed JSON/f64 equality is
separate from textual equality, mathematical agreement and full BD behavior;
legacy comparisons never gate target output or erase floating tails.

Validation/serialization finish before reserve; marker first, receipt last.
No base files or complete runtime manifest, source approval, CURRENT change or
adoption. Source stays draft; `complete_runtime: false`, `adoption: not-adopted`.
Remaining actor links/minion levels/tree/config assets, full target package,
ordinary BD/Web and accuracy need separate acceptance.

## Coordinator verification

Authored synthetic regressions are **UNRUN** by the worker. They cover both
catalogs, all owned fields, target changes, aliases/ordered duplicates, exact
rounding/formula order, empty/zero omission, unknown retained JSON, source and
policy failures, unused/referenced whitespace IDs, seven files/six payloads,
CLI/path guards and all eleven earlier
domains. A focused test distinguishes actor first-ref from unchanged normal sum.
Same-binary replay does not replace the parent's frozen old/new binary comparison.

```sh
mise exec -- cargo test --locked -p pobr-data-adapter --bin pobr-data-adapter ordinary_facts::actors::tests
mise exec -- cargo test --locked -p pobr-data-adapter --bin pobr-data-adapter
mise exec -- cargo clippy --locked -p pobr-data-adapter --bin pobr-data-adapter --tests -- -D warnings
```

Parent retains its historical skills unused-unit and skill-overrides test payload
count fixes; this slice does not edit those two files. Parent uses its one warm
target and fixed binary for both modes twice, independent complete-field/retained
JSON/receipt/oracle checks, source-limited input guards and old eleven-domain
byte compatibility. No worker Cargo, producer, Lua, new target or core/WASM/corpus
rebuild is performed.

## Target-owned actor skill lists

The optional [minions v2](../../pipeline/actors/0.5.5.5-minions-skills-policy.json)
and [spectres v2](../../pipeline/actors/0.5.5.5-spectres-skills-policy.json)
policies additionally own `skill_list`. They use the same invocation, five F
tables, original DTO and ordinary loaders. Existing v1 policies preserve their
original behavior. The v2 schema and projection ID must agree with the exact
23-field ownership set and the explicit skill-list declaration.

Each curated catalog ID still binds its exact `MonsterVarieties.Id` or alias.
The target variety's `GrantedEffects` references now supply the complete ordered
`GrantedEffects.Id` list, including duplicates and an explicit empty list.
New membership, removal and ordering come from target facts; the previous list
never limits the new list, fills an empty list or supplies a primary fallback.
Unknown physical references, malformed arrays, changed consumed column contracts
or unsupported additions fail before output is reserved. Exact source ID strings
remain untrimmed. Old list bytes remain in the archived source and per-field
comparison evidence, even when an old ID no longer exists in the target.

These two fixed authored templates declare no extra skills. Their exact
`Export/Minions/Minions.txt` and `Spectres.txt` identities are recorded separately
from the target list, and the policy explicitly declares `authored_additions: []`.
The producer neither reads nor executes those reference paths; this declaration
was checked when authoring the policy. A future nonempty addition needs an
explicit adapted contract, not a guessed difference between old and target lists.
Target F currently supplies 84 ordered occurrences for 32 minions and 2,694 for
617 spectres, all equal to the pre-v2 lists. These are source observations, not
new consumer execution or full BuildData evidence.

Receipts distinguish `target_skill_occurrences` from retained community skill
occurrences, retain every physical owner/reference position, record the raw
ordered reference array and remove the consumed column from unconsumed evidence.
All other prepared fields and unknown JSON remain explicit compatibility.
In particular, `accuracy = 1` in the archived exporter is a compatibility rule,
not a missing client Accuracy multiplier; the v2 producer does not invent one.
Synthetic regressions use the existing ordinary actor loaders to prove changed
membership/order/duplicates/empty lists reach consumers while aliases, unknown
fields and non-owned rules remain intact. They are worker-authored and UNRUN
until the integration owner executes the focused actor tests.

## Remaining source-backed prepared actor fields

The [minions v3](../../pipeline/actors/0.5.5.5-minions-prepared-policy.json) and
[spectres v3](../../pipeline/actors/0.5.5.5-spectres-prepared-policy.json) policies
own 27 fields: the v2 fields plus `monster_category`, `weapon_type1`,
`weapon_type2` and `base_damage_ignores_attack_speed`. They require paired
`--supplement`/`--supplement-manifest-sha256` arguments for the exact selected
MonsterCategories table, and select ItemClasses from the existing F alongside
the original five actor tables. A supplement without the matching policy or a
v3 policy without its supplement fails. V1 and v2 still select their five tables.

Category joins follow `MonsterVarieties.MonsterCategory` to the supplemental
row, then its `Tag` reference to the original F's `Tags.Id`. Six explicit stable
tag-to-engine vocabulary entries preserve the existing category representation.
Unknown stable category tags survive as the existing consumer's `Other` string,
with an explicit receipt status. A null source reference removes the old category.
MonsterCategories has **no `Id` or `Type` column**: its localized `Name`, `Icon`
and `Unknown0` remain raw evidence. Neither the display name nor the observed
unknown integers 144–149 define the engine category.

Main/off-hand class references resolve to exact target `ItemClasses.Id` values.
An explicit engine vocabulary supplies the existing weapon strings, including
Sceptre → One Hand Mace and Unarmed → None. Null omits the field. Quiver is an
explicit non-weapon class with its full identity retained in the receipt; another
unmapped class fails rather than borrowing an old type or silently disappearing.
These are representation rules for the existing consumer, not new raw columns.

The target MonsterTypes column at zero-based position 8 is a boolean named
`Flag0` by the pinned dat-schema. The separately pinned community
`Export/spec.lua` calls that same position `BaseDamageIgnoresAttackSpeed`.
V3 explicitly opts into that interpretation and preserves both identities and
the source boolean in its receipt. True emits the existing boolean field; false
omits it under the existing default-false DTO, clearing an old true value.
This is fixed community schema interpretation, **not official confirmation of
the anonymous raw field's meaning**. Changed source position/type or changed
interpretation declarations fail.

The category manifest and all original supplemental row values are retained in
receipts alongside physical references, stable identities, vocabulary conversion
and omissions. The source stays an acquisition draft. Focused worker-authored
regressions cover both ordinary loaders, changed category/weapon/boolean sources,
localized names that differ from category IDs, raw unknowns, absent references,
unknown categories and rejection of unmapped weapons/bad references/policy drift.
They remain UNRUN until integration validation. No core/DTO, actor OT, AI,
summoning relationship, spawn history or mod-list interpretation is added.

Remaining accuracy, catalog membership/aliases, limits, spawn labels, extra flags
and mod lists retain their declared ownership. Accuracy is a compatibility rule;
limits and extra flags are authored catalog rules. Mod lists mix DAT modifiers,
actor metadata and authored additions and cannot be replaced with an item `.it`
closure. Spawn labels require their own area/encounter source and interpretation.
The historical export-index codec also remains separate from client facts.

## Contract for the remaining mod-list source

The international 0.5.5.5 fact manifest
`050fcf93ce2cdc763d32fc9336516e25ee8791b0feb360bc811c835213712603`
already contains the six ordered DAT groups `Mods`, `Mods2`, `Special_Mods`,
`Part1_Mods`, `Part2_Mods` and `Endgame_Mods`. RePoE's
`RePoE/native_actor_facts.py` retains them separately in `actors.json`.
`tables/MonsterVarieties.json` is 7,492,728 bytes, SHA256
`87f788a94347a4f30735f6d929b270a8075eda3b4a33ffc5127eacdcf7a2e4e4`.
These groups are available facts; they do not close the actor OT or authored
parts of `mod_list`.

For the existing 649 catalog entries, the physical `InheritsFrom` column is an
**ordered string array**, with 27 distinct arrays (including empty) and 26
distinct nonempty literals. It is not a scalar actor root or a complete OT
archive. An empty array does not certify an actor with no inherited modifiers.
The item archive's 89 `.it` files do not supply those actor resources.

The smallest source handoff that can replace a mod-list field requires:

1. In RePoE, resolve exact actor resources from the indexed client inventory for
   the selected variety IDs and ordered inheritance literals. Record the actual
   resource mapping, target/observed patch and realm, raw bytes/SHA, unresolved
   paths and the complete selected parent closure. Preserve source spellings;
   do not assume appending an extension to every variety/alias proves a match.
2. Export raw OT directives, ordered blocks/fields, duplicate occurrences,
   literals and byte spans, separately from any proposed merge interpretation.
   Include every dependency needed by the selected roots. An item-tag union
   policy cannot define actor modifier inheritance or precedence.
3. Bind the ordering, default/override/addition rules and applicability of OT
   values and each DAT group. Account for all mod stat slots, flags, conditions
   and any declared authored additions. Keep absent source, explicit zero,
   unimplemented interpretation and intentional curation distinct.
4. In PoBR, project only packets whose semantics are complete into the existing
   `MinionModDef`, preserve other packets and provenance, and verify the actual
   [definition-modifier reader](../../crates/pobr-core/src/calc/minion.rs).
   That reader currently handles unconditional numeric `CritMultiplier`,
   `PhysicalDamage`, `DamageTaken`, `EnemyCurseLimit` and attack `Speed` entries
   under its existing flag/type guards. A count of transcribed `mod_list`
   entries cannot establish a change to a supported static output.

Membership, aliases, limits, extra flags and spawn labels remain separately
owned curation or encounter interpretation. `accuracy = 1` remains a compatibility
rule, and Flag0 retains its explicit community interpretation. None of these is
automatically a missing raw client column. The next slice should name the
supported output it changes before adding another actor rule or configuration.
Changed numeric values, ordering, removal, conditions, incomplete closure and
unchanged authored rules need regression coverage before ownership changes.

Store and publish the raw/fact archive in RePoE. PoBR imports its exact revision
and byte identities temporarily in CI or tests requiring real data; it retains
code, pins, source metadata and small synthetic fixtures, with no complete
archive in Git or a PoBR artifact ref. The existing 27-field catalog projection
continues independently of this unimplemented mod-list handoff.
