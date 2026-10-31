# Ordinary minion and spectre catalogs

`--ordinary-domain minions` and `--ordinary-domain spectres` use one shared actor
producer and the original `MinionsDef` / `MinionEntryDef` schemas. They replace 22
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

Original accuracy, base_damage_ignores_attack_speed, skill_list, mod_list,
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
