# Monster scaling arrays from target facts

`--ordinary-domain monster-scaling` reads only authenticated
`DefaultMonsterStats` and produces `base/monster_scaling.json` in the existing
`MonsterScalingTable` shape. GameData -> load_ruleset -> BuildData constants
already consumes this file. Existing enemy, minion, totem, defence, lookup and
clamp behavior is unchanged.

```sh
mise exec -- cargo run --locked -p pobr-data-adapter -- \
  --ordinary-facts /absolute/path/to/F \
  --ordinary-domain monster-scaling \
  --facts-manifest-sha256 56e71e00a72c8079d100b64861fc030206b12aaeb5ad655ccb4ab4e6726332d0 \
  --version 0.5.5.5 \
  --out /absolute/path/to/existing-parent/fresh-monster-scaling
```

No auxiliary flags or policy, codec, vendor, Lua, old package, default constant,
translation or download supplies values. Quality/codec-specific flags are
rejected. Missing/duplicate arguments and mixed raw/native modes fail. The eight
previous ordinary domains retain their output/receipt shapes and behavior,
except for their true compiled implementation revision.

## Level and numeric mapping

`DisplayLevel` must be a nonempty ASCII decimal string parsing to a unique level
in `1..=MONSTER_SCALING_TABLE_LEN` (currently 100). Every level must be present.
Leading zeros are decimal syntax; uniqueness is by the parsed level. Whitespace,
signs, decimal points, exponents, non-ASCII digits, overflow and out-of-range
levels fail. Physical `_row` is provenance only. Source rows may arrive in any
level order; array index is always `level - 1`. No repetition/truncation, default
table or zero filling repairs a missing level or field.

| Source field | Existing array | Conversion |
| --- | --- | --- |
| Accuracy | accuracy | exact i32 -> checked nonnegative u32 |
| Evasion | evasion | exact i32 -> checked nonnegative u32 |
| Armour | armour | exact i32 -> checked nonnegative u32 |
| MonsterLife | life | exact i32 -> checked nonnegative u32 |
| AllyLife | ally_life | exact i32 -> checked nonnegative u32 |
| AilmentThreshold | ailment_threshold | exact i32 -> checked nonnegative u32 |
| PoiseThreshold | poise_threshold | exact i32 -> checked nonnegative u32 |
| Damage | damage | finite f32-range number -> existing two decimal places |
| MinionDamage | ally_damage | finite f32-range number -> existing two decimal places |

All nine fields are required on every row. Integer JSON values must represent
i32 exactly; fractional/float-spelled integers are not truncated. Negative or
overflowing unsigned output values fail. The two floating fields accept numeric
JSON within `[-f32::MAX, f32::MAX]` without requiring f64 spelling to equal a
widened f32 bit pattern. Signed values and zero are valid. The existing DTO
convention is `(value * 100).round() / 100` in f64: nearest 0.01, ties away from
zero. No f32 cast before rounding, positivity rule, extra unit conversion,
growth curve or hiddenDamageFixup formula is added. Null/string/nonfinite or
out-of-range values fail.

Consumed column names/types/array/interval/reference metadata, required fields,
physical keys, level uniqueness/completeness and numeric representations validate
before output reservation. The same pinned reader/strict JSON/Tables and
directory-handle Output are reused, without another parser or raw directory.

## Evidence and boundaries

The target manifest is `56e71e00`, international 0.5.5.5, acquisition still draft;
selected payload SHA256 is
`b95c37f932f08ae52f0664090c781c94889dd23b8805de0f11fec99364998745`.
Parent source probe SHA256
`d155c747ff70419bd456c119b9d597eb8b091ad4a5f62feb2497a3eb13304f9c`
compared the 100 rows / 900 owned values to the old package and found equality.
That is source comparison only, not producer/runtime validation. The old package,
parent expected.json and Rust default arrays are not producer inputs.

Receipt `source_records` is sorted by parsed level and includes physical row,
original DisplayLevel, all nine raw fields, output index and nine output values.
Two normalization records identify raw and normalized damage separately.
`unconsumed_source` retains full metadata and source ranges for Experience,
Difficulty, EvasiveEvasion, AltLife2 and every other unused column. Unapplied does
not mean effectless. Target `_meta` and common receipt bind source identity,
manifest/payload/output hashes and compiled conversion/DTO revision.

```text
fresh-monster-scaling/
  _domain_candidate.json
  base/monster_scaling.json
  source-manifest.json
  source-lock.json
  producer-receipt.json
```

These five files are a domain candidate, not a complete runtime package. No
overlay, complete manifest, CURRENT change, source approval or adoption. Output
must be a fresh leaf under an existing parent; existing guards protect source
and production paths and reject overwrite/symlinks. Validation/serialization
precedes reservation, marker is first, receipt last. complete_runtime=false and
adoption=not-adopted remain explicit.

This file does not prove minionGemLevel-to-monsterLevel mapping, full minion
catalogs, remaining prepared assets, complete target package, ordinary BD/Web
coverage or accuracy. Synthetic regressions cover complete shuffled levels, all
fields, precise integers, noisy/short/signed/halfway floats, invalid data and
metadata, deterministic receipts, guards and eight prior domains. Worker Cargo,
Rust tests and actual producer are **UNRUN**. Parent uses its one warm target and
frozen binary for the independent 900-value/receipt oracle, dual replay, sealing
and prior-domain byte comparison; no unchanged core/WASM/full-corpus rebuild is
requested by this adapter-only slice.
