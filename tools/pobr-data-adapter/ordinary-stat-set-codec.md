# Legacy stat-set codec with target membership verification

`--ordinary-domain stat-set-codec` copies an explicitly pinned compatibility
snapshot into an isolated ordinary domain candidate. The source file retains its
original bytes and provenance. Target F is used only to verify owner and raw
primary/additional set identities. It supplies no replacement export indices,
labels or numerical values.

```sh
mise exec -- cargo run --locked -p pobr-data-adapter -- \
  --ordinary-facts /absolute/path/to/F \
  --ordinary-domain stat-set-codec \
  --facts-manifest-sha256 56e71e00a72c8079d100b64861fc030206b12aaeb5ad655ccb4ab4e6726332d0 \
  --codec-source data/0.5.5.4.2/overlay/stat_set_labels.json \
  --codec-source-sha256 825e594f43f07a4115ee29e57622670688e5d1c099f7e11a30ba66b3b80cca06 \
  --codec-policy pipeline/skill-codec/0.5.5.5-legacy-stat-set-policy.json \
  --codec-policy-sha256 eab2e71feeed825f0b745970cfaa41c61e22ac260dbb35f1bc55e2b83eb8fb2c \
  --version 0.5.5.5 \
  --out /absolute/path/to/existing-parent/fresh-codec-candidate
```

All four codec options are required. Other ordinary domains reject them;
stat-set-codec rejects quality-only options. Duplicate flags, missing values,
unsupported domains and mixed raw/native modes remain errors. The supplied
source file may be a copy elsewhere: its bytes, not its actual filesystem
location, establish its identity.

## Explicit compatibility policy

The target policy schema is `pobr-ordinary-stat-set-codec-policy/v1`:

- `target`: exact target patch and F manifest SHA256.
- `source`: original snapshot version, reference path, exact codec SHA256 and
  complete original `_meta`. Its archived version/path must agree with the
  original extraction command's output path. This command is never executed.
- `evidence`: source-check, old runtime manifest/base and target base/receipt
  references, with the historical binding summary. These paths/hashes are
  evidence labels and are never opened by the producer.
- `raw_ownership_exceptions`: exact `(skill,set_id,set_index)` keys and explicit
  reasons for retaining unresolved raw ownership.
- `statement`: the scope and limits of that compatibility evidence.

The supplied policy SHA is checked before parsing; the codec's external SHA
must match both the actual bytes and policy's source hash. The existing strict
JSON reader rejects duplicate keys recursively. Policy structures reject unknown
fields and malformed fingerprints. Source `_meta` must match the pinned metadata
in full, including schema `stat_set_labels/v1`, extraction generator, vendor
`PathOfBuilding-PoE2`, vendor commit, file list and original command. The original
metadata remains unchanged in the output; no target-patch tag is inserted into it.

The fixed snapshot has 1,364 rows and 1,185 owners, from version `0.5.5.4.2`,
vendor `bb52d6b368307457eb9c54bb13f1829993d390b1`. Parent source-check
`70b7b4b28ca201154581d52611728168bb77559cf6c50a00d688812ec51e6060`
found 1,358 records bound in both old and target bases, with six unbound in both,
before prepared skill_overrides/minion merges. No binding state changed. Those
are source-comparison results, not results of this producer or complete runtime
execution. The six records stay in the codec. The policy names these three raw
ownership exceptions explicitly:

| Owner | Legacy set ID | Export index |
| --- | --- | ---: |
| `ExsanguinatePlayer` | `ExsanguinateDotPlayer` | 2 |
| `SupportMetaCastOnElementalAilmentPlayer` | `SupportMetaCastOnElementalAilmentPlayer` | 1 |
| `TriggeredHaemocrystalsPlayer` | `SupportHaemocrystalsPlayer` | 1 |

Every exception must be used exactly once by an unresolved source record.
Duplicates, unlisted exceptions and unused/stale decisions fail; a record which
now matches raw ownership cannot continue using an exception. Exception set IDs
need not exist in the raw set table. Exceptions preserve records; they never
instruct the loader to apply a branch or substitute a current primary set.
These IDs and corpus counts are policy data, not Rust rules.

## Consumed fields and output

`reader::open` and `Tables::selected` select only `GrantedEffects` and
`GrantedEffectStatSets`, with the existing manifest/lock/decoder checks. Required
column metadata, physical keys and stable identities are validated. All consumed
physical references are checked, including effects absent from the codec.

| Input field | Handling |
| --- | --- |
| `GrantedEffects.Id` | Stable owner identity; required and unique |
| `GrantedEffects.StatSet` | Required physical FK to `GrantedEffectStatSets.Id` |
| `GrantedEffects.AdditionalStatSets` | Required array of physical FKs; order and repeated occurrences retained |
| `GrantedEffectStatSets.Id` | Stable set identity; required and unique |
| Codec `skill`, `set_id` | Nonempty strings; owner must exist in F; raw membership or exact exception required |
| Codec `set_index` | Positive u32; unique `(skill,index)` and `(skill,set_id)` identities; no raw-order inference |
| Codec `label` | Required string; empty labels are valid and preserved |

All codec rows, ordering, whitespace, metadata, labels and indices are copied
verbatim to `overlay/stat_set_labels.json`. The producer uses the existing
`StatSetLabelsDef` DTO for structural validation. It does not serialize that DTO
back over the original bytes, sort records, deduplicate ordered references,
translate labels, parse Lua or read old numeric bases/prepared skill overlays.

```text
fresh-codec-candidate/
  _domain_candidate.json
  overlay/stat_set_labels.json
  source-manifest.json
  source-lock.json
  codec-policy.json
  producer-receipt.json
```

Output must be fresh, outside production `data/`, F and both codec-source/policy
parent directories. The output parent must exist. The existing directory-handle
writer writes the marker first and receipt last. All source/policy/model checks
and serialization complete before output reservation. An interrupted candidate
is not reused. There is no complete runtime manifest or CURRENT update.

The receipt records actual source/policy/output hashes, compiled implementation
revision and target F identity separately. `target_membership.records` preserves
source-row keys/classification; `owners` includes effect row plus primary and
ordered additional raw set identities. Each exception includes codec source row,
current owner structure and reason. Unmapped source columns retain table, payload,
row range and identity lookup. `historical_binding_evidence` preserves the six
pre-merge unbound keys as historical evidence, without claiming that runtime
binding was performed. Source acquisition remains draft and output is
`complete_runtime: false`, `adoption: "not-adopted"`.

## Unchanged consumer and verification

GameData already binds codec entries by `(effect_id,set_id)` and gives native
labels priority. Conservative primary fallback, prepared overrides and all
calculation logic remain unchanged. The existing consumer regression is
`domains::stat_set_labels::tests::native_labels_take_precedence_while_compatibility_indices_and_legacy_labels_survive`;
this slice does not require a new core/WASM build or corpus replay.

New adapter regressions cover original byte preservation, empty labels, source
indices versus raw reference order, repeated additional refs, exact exceptions,
all required type/FK/key checks, input pins/metadata/CLI errors, output isolation,
replay and all four prior ordinary domains. The implementation worker leaves
Cargo and every producer execution **UNRUN**. The parent integrates on its warm
target, freezes the binary, verifies actual dual output and compares old domains
(all non-receipt bytes unchanged; only true compiled revision differs in receipts).

Full target package assembly, prepared runtime selection, imported BD/Web behavior,
compatibility with every later exporter and actual adoption remain separate work.
