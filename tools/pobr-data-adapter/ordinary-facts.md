# Ordinary domains from pinned facts

`--ordinary-facts` converts a fixed RePoE physical fact package into the existing
`StatDef`, `ModDef`, `ModStat` and `SpawnWeight` JSON shapes by default. Add
`--ordinary-domain stats-mods` to select that same domain explicitly, or
`--ordinary-domain base-items` for the existing item DTOs described in
[ordinary-base-items.md](ordinary-base-items.md), or `--ordinary-domain skills`
for the five ordinary skill files described in [ordinary-skills.md](ordinary-skills.md),
or `--ordinary-domain gem-quality` with the explicitly pinned target policy in
[ordinary-quality.md](ordinary-quality.md).
Each call produces only its
selected domain for later assembly into an ordinary runtime package. It does
not produce a complete GameData/BuildData package or a restricted runtime profile.

```sh
mise exec -- cargo run --locked -p pobr-data-adapter -- \
  --ordinary-facts /absolute/path/to/F \
  --facts-manifest-sha256 56e71e00a72c8079d100b64861fc030206b12aaeb5ad655ccb4ab4e6726332d0 \
  --version 0.5.5.5 \
  --out /absolute/path/to/existing-parent/fresh-candidate
```

The output parent must exist; the leaf must not exist, even as an empty directory
or dangling symlink. Linux/macOS directory handles enforce the existing native
adapter's input and production `data/` protections. Use a path outside `data/`,
such as `.cache/ordinary-stats-mods`. Arguments are required, cannot repeat, and
cannot be combined with `--raw`, `--native-facts`, `--native-runtime` or vendor
adapter modes. The legacy `--raw … --patch …` interface and conversions remain
unchanged.

An invalid, missing-value or repeated `--ordinary-domain` is an error. Omitting
it preserves the Stats/Mods JSON, marker and receipt content except for the
compiled implementation revision. The sections below describe Stats/Mods;
base-items, skills and gem-quality use the same verification/output boundary
with their own table subsets. Quality-only options are rejected by other domains.

## Input and checks

The first supported source is the existing `repoe-native-facts/v4` envelope
(`items-actors-bc` acquisition), international patch `0.5.5.5`. That is an input
format identity, not a new output profile. The existing verifier still checks
the externally supplied manifest SHA **before parsing**, producer/schema/decoder
pins, embedded `source-lock.json`, selected lock entry and acquisition evidence.
`--version` must equal the declared `source_identity.game_patch`; it cannot
relabel another patch. The selected manifest binds the lock, so another lock
argument or an implicit search for a different package is unnecessary.

Only these payloads are read, retained as authenticated bytes, and parsed:

- `source-lock.json`
- `tables/Stats.json`
- `tables/Mods.json`
- `tables/ModType.json`
- `tables/ModFamily.json`
- `tables/Tags.json`

Other files remain manifest inventory; their bytes and relationships are not
certified by this conversion. No projected `items.json`, skill/tree/actor
payloads, legacy package, vendor extraction, download or CURRENT lookup is used.

Physical documents use the existing table reader's `columns`, `physical_fields`,
validation and `rows[{_row,values}]` checks. `_row` must match its zero-based
physical position in these exact bytes. Duplicate JSON keys, duplicate physical
rows, empty/duplicate stable IDs, missing fields, changed column types or foreign
key destinations, unresolved required references and malformed ranges fail
before output is reserved. Groups also require nonempty, unique `ModType.Name`.
The ordinary converter declares its own consumed columns, without requiring
unrelated native projection fields.

## Mapping to the existing DTOs

| Output | Source and handling |
| --- | --- |
| `StatDef.id` | `Stats.Id`, never `_row` |
| `is_local` | Required `IsLocal` boolean; null is an error |
| `semantic`, `category` | `Semantic`, `Category`: optional raw unsigned metadata, including zero; no classification lookup or semantic inference |
| `ModDef.id`, `name` | `Mods.Id`, `Name`; empty/null name becomes absent, as in the legacy converter |
| `mod_type`, `group` | Optional raw `ModType` row number retained for schema compatibility; that physical row's `ModType.Name` becomes `group` |
| `domain`, `generation_type`, `level` | `Domain`, optional `GenerationType`, `Level`; checked unsigned conversions, no clamp or invented zero |
| `stats` | All `Stat1` through `Stat8` join **Stats.Id**; signed `StatNMin` and `StatNMax` become the range in source slot order |
| `families` | `Families` joins **ModFamily.Id**, independent of `group`; null stays unknown, empty stays explicitly empty |
| `tags` | `Tags` joins `Tags.Id`, preserving source order |
| `spawn_weights` | Equal-length `SpawnWeight_Tags`/`SpawnWeight_Values` join `Tags.Id`; order and zero weights are preserved |

All DTO fields are covered. Negative stat values and ranges crossing zero are
valid; inverted ranges, null endpoints on any slot, unsigned-field negatives or
overflow fail. A null stat reference denotes no stat, even when unused endpoints
are nonzero. Such endpoints are retained in `conversion.unassigned_stat_ranges`
in the receipt, with the source mod ID, physical row and slot; they are not
applied to Stats row zero. The legacy shell rule (no name and no bound stats) is
applied only after field/reference validation, and every omitted mod is listed
in `conversion.filtered_shells`.

The eight-slot physical contract is explicit. An unhandled `StatN…` field,
including a ninth slot, fails instead of silently truncating it. Known hardmode
ranges, implicit tags, generation weights and other fields outside the ordinary
DTO remain in the pinned source payload; this mode does not add their semantics.
The receipt and source manifest retain the exact source/payload identities for
following those facts. `source_row` references `tables/Mods.json#/rows/N/values`.

## Output and adoption boundary

```text
fresh-candidate/
  _domain_candidate.json
  base/stats.json
  base/mods.json
  source-manifest.json
  source-lock.json
  producer-receipt.json
```

The ordinary JSON files are sorted by stable ID. The receipt records actual
source/selected payload fingerprints, target, output sizes/hashes, field mapping,
omissions and the adapter's compiled source-content revision (`sha256:…`). It
copies acquisition status/evidence without upgrading it. A checkout's later Git
HEAD is not substituted for the implementation that produced the bytes.

The existing `_domain_candidate.json` marker is written first; no runtime
`manifest.json` is emitted. Existing package sealing/publication refuses this
marker. `producer-receipt.json` is written last; an interrupted directory is
incomplete and is never reused. `complete_runtime: false` and `adoption:
"not-adopted"` separate conversion from complete-package integration and actual
use. Target-compatible remaining domains/assets are still needed for a complete
ordinary runtime package; the schema-4 guard is unchanged.

## Shared reader for subsequent producers

Within this tool, reuse:

```rust,ignore
use crate::native_facts::reader::{self, Tables};

let facts = reader::open(input, expected_manifest_sha256, version, selected_tables)?;
let tables = Tables::selected(&facts, selected_tables)?;
let values = tables.row("Stats", physical_row)?;
```

`reader::open` returns `VerifiedFacts`, containing the verified manifest/lock,
selected immutable bytes and parsed documents. `Tables::selected` accepts
`&[&'static str]`; `row(table, physical_row)` returns that row's `values`, and
`len(table)` is for already selected table names. Column/schema metadata remains
available in `facts.documents["tables/Table.json"]`. Consumers declare their
field contracts and perform domain joins after this shared verification; they
must not reinterpret `_row` as a stable ID or reopen unverified payload bytes.
This is the same physical reader as the native adapter, not another parser in
the engine. Full-tree `native_stats` and later item/skill producers should reuse
it, adding the appropriate selected tables and ordinary domain conversions.

## Focused validation

The synthetic fixture reuses the existing v4 trust envelope and only provides
the selected files. Assertions deserialize output into the **original DTOs**,
check eight slots, signed/zero/null values, family/group separation, sorted IDs
versus physical references, receipt hashes, determinism, source failures and
output protection. Existing `mods::tests` retain legacy converter coverage.

Coordinator commands (not run by the implementation worker):

```sh
mise exec -- cargo test --locked -p pobr-data-adapter --bin pobr-data-adapter ordinary_facts::tests
mise exec -- cargo test --locked -p pobr-data-adapter --bin pobr-data-adapter mods::tests
mise exec -- cargo test --locked -p pobr-data-adapter --bin pobr-data-adapter native_facts::
```
