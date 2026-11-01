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

This mode can be selected alongside the other available ordinary domain modes;
omission still means `stats-mods`. Modes cannot be combined.
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

Passing this domain does not establish complete-package or BD/Web adoption.

## Optional compatibility overlay

When assembling a package containing the historical base override overlay, supply
all four `--codec-source`, `--codec-source-sha256`, `--codec-policy` and
`--codec-policy-sha256` options. The versioned ownership policy is
[`pipeline/base-items/0.5.5.5-overlay-policy.json`](../../pipeline/base-items/0.5.5.5-overlay-policy.json).
Pins are checked before parsing; the policy also fixes the legacy metadata and
exact delegated field set. Without these options the original base-only output
is unchanged.

The converter emits the same `base_items.json` plus
`overlay/base_item_overrides.json`. For every legacy entry whose name matches
target bases, it removes `req_str`, `req_dex`, `req_int`, `block_chance`, `spirit`
and `reload_time_ms`. The existing GameData loader then retains those values and
their absence directly from each target base ID. It does not pick one value for
multiple bases sharing a display name. Otherwise a late name-based legacy merge
would overwrite the new facts, even after a source value or component disappears.

Under the original v1 policy, other fields, unknown JSON values and entry order
remain unchanged, including inherited tags, socket limits, subtype and charm effects. Unmatched entries
remain complete declared compatibility; they do not gain a guessed target ID.
The receipt records removed old fields, all matched target IDs/physical rows and
the converted numeric values/presence. Exact legacy/policy sidecars are preserved,
and their directories are protected from output. These retained fields and the
original metadata are compatibility provenance, not complete native ownership.

Focused regressions exercise the actual `GameData::base_items` loader with
multiple same-name bases, changed target values, absent components, changed names,
unknown legacy fields, unmatched entries, bad pins/policy/DTOs, output protection
and the unchanged no-overlay invocation.

## Item inheritance tags in the same DTO

The optional paired `--supplement` and `--supplement-manifest-sha256` arguments
select a separate `repoe-item-inheritance/v1` package. This is not the supplemental
DAT-table format. The package contains exactly `manifest.json`,
`inheritance.json` and `source-lock.json`; all bytes are externally pinned before
parsing. Its producer sources, payload/lock fingerprints, selected source identity,
target patch/realm, raw resource fingerprints, root inventory and literal closure
are checked. Reconstructing the original BOM-marked UTF-16LE from ordered lines
checks the byte spans and raw identities; fields, chains and repeated tag
occurrences must agree. Extra closure members, missing parents, cycles and
inconsistent interpretations fail before output is reserved.

Every `BaseItemTypes.InheritsFrom` joins its exact extensionless stem plus `.it`.
No display name, leaf item ID or actor `.ot` is substituted. The frozen F77 has
5,496 nonempty stems, 82 distinct roots and 89 files. `nothing` is only a terminal
`extends` literal; it is not a default root. These counts describe that source,
not admission limits for a later patch.

Raw facts remain separate from the explicit
`pypoe-66d1d744-base-tag-child-first-ordered-union/v1` interpretation. It visits
child to parent and `Base.tag` fields in source order, retaining each exact string's
first occurrence. The adapter recomputes that interpretation and checks it against
the source payload, then unions the result with direct `Tags.Id` values and sorts
and deduplicates the existing `BaseItemDef.tags` set. The full raw archive preserves
every ordered field, unknown block/key, span and duplicate tag occurrence. Multiple
`Base` blocks or nonliteral/empty tags leave a root rejected with diagnostics;
they do not become empty inherited tags. Other fields have no inferred merge rule.
This is a fixed community parser interpretation, **not official client merge-law
certification**, raw unique coverage or an actor inheritance implementation.

When a legacy override is also supplied, use the explicit
[v2 ownership policy](../../pipeline/base-items/0.5.5.5-inherited-tags-policy.json).
It binds the inheritance manifest and interpretation ID in addition to the six
existing numeric fields. Old `tags` are removed only when every target with that
display name has resolved inheritance, allowing each ID's own tags to reach the
existing ordinary loader. A same-name group mixing resolved and unresolved roots
fails rather than borrowing another base's tags. Entirely unresolved groups and
unmatched old entries retain their declared compatibility. A v2 policy without
inheritance, v1 with inheritance/overlay, wrong pins or partial arguments fail.
The original no-supplement/v1 invocation retains its original behavior.

Three exact source sidecars are added: `item-inheritance-manifest.json`,
`item-inheritance.json` and `item-source-lock.json`. The `item_inheritance` receipt
records each base's direct tags, root, full chain/occurrences, interpretation,
ownership and final DTO tags. Source acquisition remains draft. Regression tests
use the real `GameData::base_items` loader to exercise parent tag/source-member
changes, distinct same-name IDs, old-tag removal, preserved unknowns and rejection
before output. Worker-authored Rust tests must be run by the integration owner;
source inspection and formatting alone are not runtime-test evidence.

## Remaining source and consumer contracts

External fact packages and the archive sidecars described above belong in RePoE
storage and temporary CI/test directories. PoBR retains adapter code, contracts,
precise input pins and small synthetic fixtures. Neither PoBR Git nor a PoBR
artifact ref is a destination for complete fact tables or version snapshots.

The following observations use the international 0.5.5.5 fact manifest
`050fcf93ce2cdc763d32fc9336516e25ee8791b0feb360bc811c835213712603`
(44,765 bytes) and item-inheritance manifest
`f8642243fc1d04cecb26c1bd770fb9bbfd70574b93c82969f737013be23c8f31`
(14,352 bytes). They identify inspected inputs, not a new source approval or
runtime adoption. Counts below are observations, never admission limits.

| Retained field | Existing consumer and required evidence |
| --- | --- |
| `socket_limit` | The [WASM augment editor](../../apps/pobr-wasm/src/build_api/augments.rs) reads the override directly to set `max_sockets` and reject an unknown limit. The 89-file item archive contains 16 ordered `Sockets.socket_info` literals and two `lock_sockets_when_full` occurrences. These do not yet establish the ordinary augment capacity: the same archive includes skill-gem and accessory socket tuples. Obtain the socket system, tuple meaning, inheritance/override rule and normal versus exceptional capacity before projecting a stable base ID to a limit. Unknown remains unknown; it must not become zero or the largest integer in a tuple. |
| `sub_type` | The same editor uses `Shield` + `Evasion` for the `buckler` slot and `Warstaff` for `quarterstaff`. The archive's 25 `Mods.inventory_type` occurrences are separate literals, not this vocabulary. Bind the relevant base/class/subtype relationship and augment qualification before removing each retained override. Armour values, attribute requirements or display names alone do not prove that relationship. |
| `charm_buff` | The ordinary loader merges the retained text into `BaseItemDef`; [utility injection](../../crates/pobr-build/src/calc_orchestrator/inject.rs) adds it to the active charm's attributed payload. A concrete target relationship already exists, detailed below. Complete packet interpretation and text conversion remain to be bound. |
| Rarity/mod eligibility | The archive preserves 27 `Mods.enable_rarity` and five `disable_rarity` occurrences. Raw `BaseItemTypes.Unmodifiable`, `ItemClasses.Unmodfiable` and `ItemClassFlags` are distinct inputs. Their inheritance, precedence, enum/bit meanings and applicable operation need evidence before changing editor admission. Existing [tier lookup](../../crates/pobr-item/src/tier.rs) already reads target `mod_domain`, inherited/direct tags and ordered mod spawn weights; migrating tags does not certify every crafting/rarity rule. |
| Unmatched overrides | Energy Blade One Handed, Energy Blade Two Handed, and the three Shrine Sceptre variants (Purity of Cold, Fire and the historical spelling `Lighting`) remain complete compatibility. Establish an exact base/skill/variant relationship and intended consumer before replacing one; name resemblance is insufficient. |

Keep socket/subtype overrides even though `BaseItemDef` has no corresponding
fields: their direct WASM reader is outside the ordinary base merge. A change
must exercise that reader's capacity and buckler/quarterstaff filtering, as well
as the existing loader. Successful base deserialization alone cannot verify it.

### Charm relationship already present in target facts

All 13 target charms resolve through ordered
`Flasks.UtilityBuff[] → UtilityFlaskBuffs.BuffDefinition → BuffDefinitions`.
Their direct `Flasks.BuffDefinition` is null; reading only that column would miss
all 13. The relevant complete table identities in the pinned manifest are:

| Payload | Bytes | SHA256 |
| --- | ---: | --- |
| `tables/Flasks.json` | 9,042 | `8fa1770564a98b6a65d361ce404f79ea2eee7e0acc7c9cc28d9aa0ebe64859ad` |
| `tables/UtilityFlaskBuffs.json` | 2,487 | `493a2c4fe87d554e1ea4efe0b8c5fa126c8ea631df63e83dd3139962196cc2e9` |
| `tables/BuffDefinitions.json` | 3,035,364 | `af156221e8a25b3b1f1f8f9bd54f06f8ea945aefcea49e524fb36bd720740094` |
| `tables/Stats.json` | 11,634,008 | `bef280b241a844972de0020e57c64cc403ac7e44fddf346b1014d17e1372376a` |

Five selected utility rows have one `StatValues` member: fire/cold/lightning
resistance 25, chaos resistance 18, and item rarity 15. Their buff definitions
contain the same stat reference in both physical `Stats` and `GrantedStats`
columns. These are separate columns, not two authorizations to apply the value
twice. The other eight definitions use `GrantedFlags` with empty `StatValues`;
empty numeric values do not establish an empty/effectless buff. In particular,
`FourCharm13` (Cleansing Charm) has a curse-immunity flag but no legacy
`charm_buff`, so the 12 retained text entries do not bound target coverage.

The next adapter slice should bind value-to-stat ordering, the roles of
`Stats`/`GrantedStats`/`Stats2`, flag truth semantics, conditions and applicable
effect scope, then convert complete supported packets through an explicit text
interpretation. Verify every consumed FK and list length. Retain ordering,
duplicates, nulls, zero values, unsupported members and their source pointers.
Do not synthesize numeric zero for flags, concatenate duplicate stat lists, or
use `BuffDefinitions.Description` as a numeric template. Known scalar values
alone do not authorize dropping an unknown member from the packet.

This is a PoBR consumption/interpretation gap in already exported facts, not a
request for a second raw charm exporter or another complete snapshot. RePoE owns
any newly required raw decoding; language assets remain in the translation
repository. Once a complete base-ID packet is owned, remove its late name-based
legacy charm override with the same protection against mixed same-name ownership
used for inherited tags. Test changed values, removal, explicit zero, unresolved
packets and active/inactive charm output before declaring that field migrated.
