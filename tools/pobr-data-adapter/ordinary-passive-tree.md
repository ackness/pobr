# Complete ordinary passive tree with target native fields

`--ordinary-domain passive-tree` preserves the complete explicitly supplied
prepared tree and policy-owned class/ascendancy metadata. Only eligible nodes'
`native_stats` and the classes' `base_str`, `base_dex`, `base_int` come from target
facts. Output uses the original `PassiveNodeDef[]` and `PassiveTreeMeta` shapes;
it is a domain candidate, not a complete runtime or an adopted data version.

```sh
/path/to/fixed/pobr-data-adapter \
  --ordinary-facts /absolute/path/to/F \
  --facts-manifest-sha256 56e71e00a72c8079d100b64861fc030206b12aaeb5ad655ccb4ab4e6726332d0 \
  --version 0.5.5.5 --ordinary-domain passive-tree \
  --codec-source /absolute/path/to/data/0.5.5.4.2/base/passive_tree.json \
  --codec-source-sha256 7e021c17f9679b3ff4d72cacdba20ee3d17bf8d0acf7ea02311dd29886996851 \
  --codec-policy /absolute/path/to/pipeline/passive-tree/0.5.5.5-prepared-compat-policy.json \
  --codec-policy-sha256 93078d21dbe2e894061f5e1b9330535adfd7ddf2c0679fd8b7957595c872e260 \
  --out /absolute/path/to/existing-parent/fresh-tree-candidate
```

All four codec options are required. Duplicate, missing, branch-codec and quality
options fail. The external F manifest, selected bytes, source and policy pins are
checked before parsing; target version and policy manifest must match F. Only
`PassiveSkills`, `Stats`, `Characters`, `ClassPassiveSkillOverrides` and the lock
are selected through `reader::open` / `Tables::selected`. No derived graph, vendor,
Lua, official export, translation or other asset is implicitly opened.

The policy declares the old source's original version and snapshot manifest. Its
`legacy_metadata.value` supplies the complete metadata as an explicit input. The
old metadata file and historical evidence paths are **authoring declarations**:
the producer neither reads them nor claims to verify their file hashes. Policy
bytes and target identities are verified independently.

## Preservation and binding

Every prepared entry remains in source order with its exact JSON fields and
presence, including unknown values, all text, coordinates, variants, unlock
constraints, ascendancies, `apply_to_armour`, and ordered duplicate references.
Serialization through DTO defaults is avoided; the DTO validates known fields.
An old native packet is removed or replaced, never used as a numeric fallback.

Each prepared `id` binds an exact `PassiveSkills.Id`; `PassiveSkillGraphId` must
match `skill`. Physical rows are evidence/FK identities, never business IDs.
The fixed policy's numeric-slug alias verifies its full original entry hash,
name and target identity. It keeps the original output ID and receives no native
packet. Missing identities and stale/unused or duplicate declarations fail.

Connections to emitted nodes or variant aliases retain their existing shape.
An outside endpoint requires an exact `legacy_connection_exceptions` occurrence:
source row/skill/id, full original entry hash, pointer/position and target skill.
The declared stable target Id must resolve to the recorded F row/name/GraphId.
Each exception is consumed once; changed, extra, duplicate, unused or now-resolved
exceptions fail. Multiple positions in one entry remain distinct. These are
retained compatibility edges, not newly emitted nodes, rewiring, or certification
of reachability, allocation eligibility or effectlessness. The pinned policy
currently declares 15; that observation is not a compiled count or ID whitelist.

## Native fields and metadata

The existing whole-row reference recipe compares every physical key/value except
`Id`, `Name`, `Icon_DDSFile`, `PassiveSkillGraphId`, `Stats`, `Stat1Value` with the
pinned context of `maximum_resistances1`. The reference must contain exactly the
fire-resistance stat; its signed value is not pinned or reused for other nodes.
Opaque fields remain context comparisons, not inferred predicates or ordinals.

A native packet requires one complete raw stat/value, matching whole context,
no involvement as either endpoint of a class override, a normal prepared node
without variant/ascendancy/unlock/armour transformation, and one nonempty trimmed
original display line. The shared `pobr_core::rules::passive_stats::map_stat`
must accept that complete stat. No mapper list or rule is duplicated here.
All other nodes retain their complete original text with native absent; there
is no partial multi-stat projection or diagnostic suppression.

The packet uses target patch, policy tree tag, unchanged node identity/text and
signed target value. Both the passive value and stat identity have distinct
payload-hash/pointer provenance. Text remains fixed compatibility display, not
a certified target formatter. Consumer version/context/variant guards and
whole-node fallback are unchanged.

Classes match exact unique `Characters.Name`, requiring complete coverage. The
three base attributes use exact signed i32 facts, including zero/negative values.
Class and ascendancy order, IDs/names and all other metadata remain policy-owned.
`Requirement` is not interpreted as a node predicate. Missing Ascendancy tables
do not authorize guessing ordinal mappings.

## Files and receipts

The fresh directory contains eight files:

- `_domain_candidate.json` (first)
- `base/passive_tree.json`, `base/passive_tree_meta.json`
- `legacy-passive-tree.json`, `codec-policy.json` (verbatim inputs)
- `source-manifest.json`, `source-lock.json`
- `producer-receipt.json` (last)

All validation and serialization precede reservation. Existing directory-handle
protections reject overwrite/symlinks and output within inputs or production
`data/`. No overlay, complete manifest, profile, `CURRENT`, vendor regeneration
or adoption is generated.

Receipt records each source/output entry hash, all retained field pointers and
hashes, target row/Id/graph, alias, full raw-row hash, ordered raw stat evidence,
context differences, class-override involvement, native reasons/packet hash and
consumed connection exceptions. It also includes the complete context reference,
class relationships, attribute evidence, metadata value/hash and unmapped or
qualification-only columns. Compiled revision includes this converter, the tree
and native DTOs and existing mapper.

The current prepared source has 4913 nodes, full 12-class/23-ascendancy metadata
and 12 true armour flags. These are source observations, not admission limits.
Source acquisition remains draft; complete target text/legality, package
assembly, ordinary BD/Web acceptance and adoption remain separate work.
