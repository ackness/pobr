# Fixed skill interpretation map for the target migration

`--ordinary-domain skill-stat-map` copies an explicitly pinned community
interpretation map verbatim into an ordinary domain candidate. The existing
SkillStatMapDef / StatMapCatalog / stat_map_engine and GameData/BuildData consumer
remain unchanged. Target F certifies stat presence/absence and raw branch
ownership; it does not acquire or prove the map's gameplay rules or constructor
parameter values.

```sh
mise exec -- cargo run --locked -p pobr-data-adapter -- \
  --ordinary-facts /absolute/path/to/F \
  --ordinary-domain skill-stat-map \
  --facts-manifest-sha256 56e71e00a72c8079d100b64861fc030206b12aaeb5ad655ccb4ab4e6726332d0 \
  --version 0.5.5.5 \
  --codec-source data/0.5.5.4.2/overlay/skill_stat_map.json \
  --codec-source-sha256 944e86eb729899a547daedc8eab6741d773eb40ec1f64a380a9acd2c20054661 \
  --codec-policy pipeline/skill-stat-map/0.5.5.5-legacy-policy.json \
  --codec-policy-sha256 72b2c0503d5127eecd74743b6a81a180fe50b2fc51bead1245620f3665d8e6a7 \
  --branch-codec data/0.5.5.4.2/overlay/stat_set_labels.json \
  --branch-codec-sha256 825e594f43f07a4115ee29e57622670688e5d1c099f7e11a30ba66b3b80cca06 \
  --out /absolute/path/to/existing-parent/fresh-skill-stat-map
```

The existing codec-source flags carry the map, codec-policy flags carry its target
compatibility policy. Both new branch-codec flags are required here and rejected
in every other domain. Quality flags fail here; all pins, missing values and
duplicate options are checked. No implicit legacy path, prepared candidate,
vendor/Lua, numerical package, translation or regeneration is used.

## Authentication and preserved semantics

The existing auxiliary reader validates each external SHA256 before strict JSON
parsing, including duplicate-key rejection. Policy target patch/F manifest must
match. Map and branch codec pins, original metadata, schema, generator/vendor,
commit, snapshot version and archived output path must agree with policy. The
archived `data/<snapshot>/overlay/...` path is checked against original extraction
metadata; an explicitly supplied local copy may live elsewhere. All three
actual auxiliary input parents are protected from output overlap.

Policy schema is `pobr-ordinary-skill-stat-map-policy/v1`. The tracked target
policy is the frozen template verbatim. Unknown policy fields/schema, malformed
hashes, empty/noncanonical identities, duplicate exceptions or unsupported
exception actions fail. Evidence references retain their path/hash without
implicitly reading the referenced old runtime or source-check file.

Map JSON is parsed into original SkillStatMapDef for shape compatibility, but
runtime output uses original bytes. Constructors, group/scalar transforms,
tags, order, unknown fields and unsupported/unextractable records are never
filtered, reserialized, normalized, retargeted or interpreted. The branch codec
and policy are likewise copied exactly, including original metadata. The two
legacy assets remain0.5.5.4.2 / vendor bb52d6b3, not0.5.5.5 facts.

## Target identity and branch checks

Only F Stats, GrantedEffects and GrantedEffectStatSets are selected through
reader::open / Tables::selected. Consumed schemas, physical keys and unique
stable IDs validate. Every global and per-scope stat occurrence records the exact
Stats.Id physical row or explicit absence. Policy `absent_target_stat_ids` must
equal the actual absent ID set: both new absences and unused/stale exceptions fail.
Constructor parameters are not treated as target game numbers.

Per-set scopes have canonical nonempty effect IDs and positive canonical decimal
u32 index strings (no leading zero/sign/whitespace). Branch codec identities are
checked for every record: nonempty canonical skill/set IDs, positive u32 index,
unique `(skill,index)` and `(skill,set_id)`. Empty label text remains legal.

For each map scope, use the actual codec `(skill,index)` record's stable set_id,
then validate ownership through the effect's primary and ordered additional
physical references. Export index is never inferred from raw position, label,
set sorting or primary fallback. Repeated ordered additional refs remain repeated.
Only map-used owner references are certified here; unused codec archival records
need not bind to raw target owners. The existing stat-set-codec domain still
validates its whole archival inventory through the same extracted owner helper.

This policy's unbound exception action preserves a scope only when both target
effect and codec record are absent. Every declared exception must be consumed;
newly matching, partial, missing or unused exceptions fail. No ownership override
or fallback is introduced.

Parent source probe `e25640a3e17b6dfd7dc04f1e30303bf8065497bb6f092ac7ce3b9f1281f45d91`
observed975 global /618 scoped entries,400 scopes with399 raw-owned and exactly
BloodbarrierPlayer/1 absent from both effect and codec,132 absent stat IDs, and two
unextractable global records. These are identity observations, not hardcoded Rust
counts/skill lists or a claim of gameplay-law correctness. All records remain.

## Receipt, output and remaining boundaries

Receipt `global_records` and each `scope_records.stats` preserve every mapping
occurrence with source JSON pointer, canonical-entry hash, target stat row or
absence, and unextractable marker/raw entry. Scope records include exact codec
row/record and raw owner primary/ordered additional references, or explicit
unbound policy status. Unused table columns retain full metadata and row ranges.
Legacy versions/meta/pins, target F provenance, all output hashes and compiled
conversion/DTO revision remain separate. Unapplied or missing is not effectless.

```text
fresh-skill-stat-map/
  _domain_candidate.json
  overlay/skill_stat_map.json
  branch-codec.json
  codec-policy.json
  source-manifest.json
  source-lock.json
  producer-receipt.json
```

Validation/serialization precedes reservation; marker first, receipt last. Fresh
leaf, overwrite/symlink/source/production and three auxiliary-parent protections
reuse Output. No base numerical domain, complete manifest, CURRENT/source
promotion or adoption. Source acquisition remains draft international0.5.5.5;
complete_runtime=false and adoption=not-adopted.

Nine previous domains and old raw/native behavior remain, apart from true
compiled revision. Meaningful synthetic regressions are authored **UNRUN**:
exact bytes/unsupported retention, actual branch binding/order/aliases/partial
coverage/unbound facts, exceptions, bad keys/FKs/metadata, three independent pins,
CLI/output guards and old-domain compatibility. All worker Cargo/producer is
UNRUN; parent uses one warm target and a frozen binary for real inventory/receipt
verification, dual replay, sealing and nine-domain byte comparison. No unchanged
WASM/core/full-corpus rebuild is requested. Target mechanic correctness, remaining
prepared assets/tree/config/minions, full package/ordinary BD/Web/accuracy and
source approval/adoption still require separate acceptance.
