# Ordinary monster resistances from target facts

`--ordinary-domain monster-resistances` generates the existing
`MonsterResistancesDef` / `MonsterResistanceDef` file from three authenticated
physical tables. The original raw adapter and this mode call the same
`monsters::adapt_rows` join; all valid source sums retain their existing meaning.

```sh
mise exec -- cargo run --locked -p pobr-data-adapter -- \
  --ordinary-facts /absolute/path/to/F \
  --ordinary-domain monster-resistances \
  --facts-manifest-sha256 56e71e00a72c8079d100b64861fc030206b12aaeb5ad655ccb4ab4e6726332d0 \
  --version 0.5.5.5 \
  --out /absolute/path/to/existing-parent/fresh-monster-resistances
```

No compatibility policy or legacy numeric input is needed. Quality/codec-only
options, duplicate flags, missing values, unknown domains and mixed raw/native
modes are errors. The other six ordinary domains retain their file/receipt shapes
and conversion behavior, except for the actual compiled implementation revision.
The old raw `adapt`/`write` path retains its receipt, patch and hash metadata; its
valid sums are unchanged. Checked i32 addition now reports invalid overflow
instead of a debug panic or wrapping sum.

## Mapping and validation

The existing `reader::open` authenticates manifest, lock and only these payloads;
`Tables::selected` validates the physical records. Typed rows deserialize directly
from verified `values` in memory. There is no temporary raw directory or new
source parser. All three tables' `Id` columns must contain unique nonempty strings,
and their physical `_row` keys must be valid. Stable IDs never substitute for
physical row references; a source `_index` field is not used.

| Input | Existing conversion |
| --- | --- |
| `MonsterVarieties.Id` | Output map key, with no monster filtering |
| `MonsterVarieties.MonsterType` | Required physical FK to `MonsterTypes` |
| `MonsterTypes.MonsterResistances` | Required ordered array of physical FKs; every occurrence contributes |
| `MonsterResistances.Fire1` | Singleton signed i32 array → fire percentage points |
| `MonsterResistances.Cold1` | Singleton signed i32 array → cold percentage points |
| `MonsterResistances.Lightning1` | Singleton signed i32 array → lightning percentage points |
| `MonsterResistances.Chaos1` | Singleton signed i32 array → chaos percentage points |

Negative vulnerabilities and zero components are preserved. Ordered duplicate
references contribute repeatedly; the conversion neither deduplicates nor chooses
the first/max record. An explicit empty list produces known base zeros. Missing,
null, negative/fractional/unresolved references and invalid values never default
to zeros. All consumed columns must have the expected type/array/FK metadata.
Unused type references are also checked, and every resistance row, including
unused rows, must have exactly one signed i32 value in each normal column. Empty
or multi-valued normal arrays fail rather than guessing level-dependent semantics.
Every accumulation uses checked i32 addition in the shared original join.

Other rarity arrays and columns remain pinned source facts, with their metadata
and row ranges disclosed in the receipt. They are not applied or declared
effectless. This domain introduces no rarity, race, level or gameplay calculation.

## Candidate and provenance

```text
fresh-monster-resistances/
  _domain_candidate.json
  base/monster_resistances.json
  source-manifest.json
  source-lock.json
  producer-receipt.json
```

All validation, summation and serialization precede output reservation. The
existing directory-handle writer requires a fresh leaf under an existing parent,
outside F and production `data/`; even an empty directory or dangling symlink is
refused. Marker is written first and receipt last. No runtime manifest, CURRENT
change or source promotion occurs. Candidate is `complete_runtime: false`,
`adoption: "not-adopted"`.

The target file retains schema `monster-resistances/v1` and the original
`monsters` map. Its `_meta` identifies the target patch, F source identity,
manifest SHA and three actual input hashes; it does not reuse an old raw receipt.
The producer receipt binds source/output bytes and compiled conversion/DTO
revision. Each `source_records` entry contains variety ID/physical row, type
ID/physical row, and every resistance occurrence's position, row, stable ID and
four original singleton arrays. Empty lists remain explicit. `unconsumed_source`
lists unused column metadata, table/payload and row range.

Parent source-check `ee04d9dba6afac296da06d839837ef245d2304ed808bd0a8510599d509e6bdc6`
reports 2,794 varieties, 1,571 types and 22 resistance rows, with 11,176 expected
components, 592 negative components and 1,631 all-zero varieties. Four varieties
have explicit empty lists; 521 use two records and 42 use three. These are source
probe counts, not worker producer/runtime results or Rust constants. Source
remains draft international `0.5.5.5`; the root's expected.json is not a producer
input.

## Existing consumer and verification scope

`GameData::monster_resistances` already loads this DTO. `BuildData::load` replaces
minion/spectre base resistances by `monster_variety_id` (falling back to the entry
ID), and `totem_defs` reads the same table. None of those consumers changes.
Full minion/spectre catalogs, minionList/loadout mappings, totem catalogs and their
other numeric fields remain separate assets; this file does not certify old
catalog numbers for the new patch. Full target package, ordinary BD/Web behavior
and adoption still require separate acceptance.

Synthetic regressions cover signed multi-row/duplicate sums, empty and zero
facts, all required keys/metadata/types/FKs including unused rows, singleton
arrays, every component's overflow/underflow, deterministic replay, pins/CLI and
protected output, original raw bytes/metadata/guards, and six-domain compatibility.
Worker Rust tests, all Cargo and actual producer execution are **UNRUN**. Parent
uses its existing warm target and frozen binary for independent full field and
receipt occurrence comparison, dual replay, sealing/guard and old-domain byte
checks. No unchanged core/WASM or full-corpus rebuild is requested by this slice.
