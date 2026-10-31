# Ordinary base items from pinned facts

The explicit `base-items` domain of `--ordinary-facts` reads the same fixed F
envelope through `reader::open` and `Tables::selected`. It emits the existing
`BaseItemDef`, `WeaponBaseStats`, `ArmourBaseStats` and `UtilityBaseStats` shapes.
It does not change the core, GameData/BuildData, DTOs or legacy converters.

```sh
mise exec -- cargo run --locked -p pobr-data-adapter -- \
  --ordinary-facts /absolute/path/to/F \
  --facts-manifest-sha256 56e71e00a72c8079d100b64861fc030206b12aaeb5ad655ccb4ab4e6726332d0 \
  --version 0.5.5.5 --ordinary-domain base-items \
  --out /absolute/path/to/existing-parent/fresh-base-items
```

`--ordinary-domain` accepts `stats-mods`, `base-items` and
[`skills`](ordinary-skills.md); omission means `stats-mods`. Modes cannot be combined.
The existing manifest/lock/decoder pins,
target patch equality, fresh-directory requirement, directory-handle protection
and validation-before-write rules still apply. No implicit source search,
vendor extraction, download, production manifest, translations or CURRENT write
occurs. The selected input remains the international 0.5.5.5 acquisition draft.

The eleven selected table payloads are BaseItemTypes, ItemClasses, Tags, Mods,
WeaponTypes, ArmourTypes, ShieldTypes, ItemSpirit, AttributeRequirements, Flasks
and ComponentCharges. The embedded source-lock is also verified. Stats and other
unrelated table/projection payloads are not read. Mods and Tags are used only for
stable IDs; their other semantics are not certified in this domain.

## Fields and joins

| Existing DTO field | Source and handling |
| --- | --- |
| `id`, `name` | BaseItemTypes.Id and Name; neither physical row nor display name is an identity substitute |
| `item_class` | ItemClass physical reference → ItemClasses.Id |
| `drop_level`, `width`, `height`, `mod_domain` | DropLevel, Width, Height, ModDomain; checked u32/u8 conversions, including actual zero |
| `tags`, `implicits` | Ordered Tags → Tags.Id and Implicit_Mods → Mods.Id; unresolved elements fail |
| `weapon` | WeaponTypes.BaseItemType → physical base row; DamageMin/Max, Speed in ms, raw CritChance, RangeMax copied without extra unit conversion; ReloadTime > 0 is milliseconds, zero is absent |
| `armour` | ArmourTypes.BaseItemType → physical base row; Armour/Evasion/EnergyShield/Ward copied; signed IncreasedMovementSpeed maps to `-raw/10000.0`, zero becomes absent |
| `armour.block_chance` | ShieldTypes.BaseItemType → existing armour; Block is already a percentage, including Some(0) |
| `spirit` | ItemSpirit.BaseItemType → base; SpiritGranted is Some(value), including zero; no row means None |
| `req_str`, `req_dex`, `req_int` | AttributeRequirements.BaseItemType → base; ReqStr/ReqDex/ReqInt copied; no requirement row means zero per the existing base-requirement convention |
| `utility` | Flasks.BaseItemType → physical base row; ComponentCharges.BaseItemTypesKey → **BaseItemTypes.Id string**, independently of component row order |
| Utility duration/life/mana/charges | RecoveryTime / 10 seconds, LifePerUse, ManaPerUse, MaxCharges, PerCharge; unsigned values become exact f64 amounts |
| `charm_buff` | Empty pending compatible inherent-effect descriptions; each `UtilityFlask` base is recorded as missing this field's semantics |

Required fields must be present and of the declared source type. Null is not an
invented zero; the nonoptional numeric fields reject it. Negative unsigned
values, overflow, inverted weapon ranges, duplicate component joins, duplicate
IDs/physical keys, unknown referenced rows and changed consumed-column contracts
are errors. No additional positive-speed or positive-size requirement is imposed.
The physical reader validates `_row` against the source position before any
join; component array position is never used to select a base. Equal display
names are allowed.

All 203 ShieldTypes rows in this F have corresponding ArmourTypes. If a future
selected input lacks that join, conversion fails; missing defences are not
fabricated as a zero armour shell. A present but malformed requirement/component
row also fails, including when its base would later be filtered.

Name is a required string under the existing raw contract. This F has no null
names; null fails instead of becoming an empty string. The existing placeholder
rule filters empty names or names containing `[DNT`, `[UNUSED` or `[OLD` **after**
all consumed fields and references have been validated. Each excluded stable ID,
source row and original name is recorded in `filtered_placeholders`.

## Retained omissions and output

```text
fresh-base-items/
  _domain_candidate.json
  base/base_items.json
  source-manifest.json
  source-lock.json
  producer-receipt.json
```

No Stats/Mods or language sidecars are emitted in this mode. Base items are
sorted by stable ID. The existing domain marker is written first, and the final
receipt is written last. Existing sealing/publication refuses the marker; this
directory is not a complete runtime package. Acquisition remains unchanged and
the receipt states `complete_runtime: false`, `adoption: not-adopted`.

The receipt binds the source/selected payload hashes, output hashes, target and
compiled implementation revision, now including the item converter and original
`catalog/items.rs` definition. Previous artifacts/receipts are not relabelled.
Its structured `conversion` section contains:

- `filtered_placeholders` and `bases_without_requirement_row` (counts all source
  bases, including placeholders), with the exact filtering/default policy.
- `missing_charm_buff`: stable IDs, source rows, class and inclusion status for
  UtilityFlask bases. Empty buffs do **not** mean effectless charms or complete
  base-item semantics. This producer does not resolve raw buff references into
  a new mechanism or borrow another patch's vendor/overlay values.
- `unmapped_fields`: per-table payload, column inventory and source-row interval
  for facts outside this DTO. They remain at their pinned source reference.
- `utility.unmapped_rows`: per-row copies of unconsumed Flasks/ComponentCharges
  values, including RecoveryTime2/MaxCharges2/PerCharge2 and raw buff references.
  Null and zero remain distinct. No meaning is inferred from those references.
- `utility.secondary_mode_differences`: explicit primary/secondary field/value
  pairs when they differ. The pinned F currently has equal primary/secondary
  duration and charge values; fixtures exercise differences and nulls to prevent
  future accidental substitution.
- `utility.charges_without_flask_row`: any charge rows not projected into Utility
  DTOs, with primary values and source identity. Flasks missing required charges
  are errors; independent charge facts are not silently attached to another base.

## Regressions and parent validation

The small fixture has permuted component rows, reversed stable-string charge
joins, repeated display names, zero/signed values, a placeholder and raw secondary
mode differences. Tests deserialize the existing DTOs and check every field,
unit, join, omission, relevant failure and CLI default. Existing Stats/Mods and
raw/native tests remain in the same binary target.

```sh
mise exec -- cargo test --locked -p pobr-data-adapter --bin pobr-data-adapter
```

The implementation worker did not execute Cargo or the producer. Parent performs
warm-target compilation/tests, fixed-binary conversion and field comparison after
integration. Passing this domain does not establish complete-package or BD/Web
adoption.
