# Ordinary passive tree from target facts

`--ordinary-domain passive-tree --tree-reference ... --tree-descriptions ...`
produces the existing `PassiveNodeDef[]` and `PassiveTreeMeta` from the pinned
RePoE Default graph and physical tables. The ordinary loader, BuildData, text
parser and native stat mapper are unchanged. This is a domain candidate;
assembly, source approval and adoption remain separate.

```sh
/path/to/fixed/pobr-data-adapter \
  --ordinary-facts /absolute/path/to/F \
  --facts-manifest-sha256 56e71e00a72c8079d100b64861fc030206b12aaeb5ad655ccb4ab4e6726332d0 \
  --version 0.5.5.5 --ordinary-domain passive-tree \
  --codec-source /absolute/path/to/data/0.5.5.4.2/base/passive_tree.json \
  --codec-source-sha256 7e021c17f9679b3ff4d72cacdba20ee3d17bf8d0acf7ea02311dd29886996851 \
  --codec-policy /absolute/path/to/ordinary/inputs/passive-tree/codec-policy/0.5.5.5-complete-text-tree-policy.json \
  --codec-policy-sha256 <externally-pinned-policy-sha256> \
  --tree-reference /absolute/path/to/official-tree-bd87e651.json \
  --tree-descriptions /absolute/path/to/descriptions.json \
  --out /absolute/path/to/existing-parent/fresh-tree-candidate
```

The codec options are required. The two tree options must be supplied together,
only for this domain. The policy pins the reference, CSD payload and adjacent CSD
manifest. The target patch must agree across F, policy and CSD manifest. Selected
payloads are authenticated before parsing, using the existing strict reader.
No client, Lua, network acquisition or implicit translation input is invoked.

## Field owners

| Output | Owner and boundary |
| --- | --- |
| Node membership and ordered connections | Authenticated Default graph instances, cross-checked against physical PassiveSkills and graph topology. Source-bound eligibility exclusions remain explicit compatibility. Edges to those excluded records are recorded and omitted; every emitted edge must resolve. |
| Stable ID, name, category | Target PassiveSkills; physical row numbers are evidence, not business IDs. The original numeric-slug alias is tied to its exact prepared entry. IsJustIcon identifies mastery icons; MasteryGroup alone does not. |
| Group, orbit and slot | Target graph. Groups use the existing one-based DTO convention. |
| x/y | Target group centres, rounded to the reference's six significant digits, plus prepared physical placement offsets (including coordinate absence), anchored to the reference group. New nodes require explicit reference offsets. Exact group/orbit/slot identity is required. A new or changed binding without offsets fails; raw radius is not interpreted as physical distance. |
| Complete stat text | Target passive CSD scope and its explicit general include, using the complete ordered source stat/value list. Unsupported whole-node text uses a declared compatibility binding. |
| Skill and point grant text | v4 uses source ClientStrings templates, SkillGems FKs and BaseItemTypes names alongside the complete CSD stat packet. Passive buffs still require whole-text compatibility. |
| Class replacements | Target class relations determine membership, aliases, names and complete text. Fixed class bindings identify old entries to replace/remove. Other variants remain guarded complete compatibility. |
| Base class attributes | Target Characters values. |
| Ascendancy IDs, names, character/node relationships and replacements | With v2 policy and supplemental Ascendancy/AscendancyPassiveSkillOverrides, target physical rows; v1 retains prepared compatibility. |
| Unlock node and ascendancy identities | v3 resolves target UnlockedBy and VisibleForAscendancy FKs into the existing all-prerequisites DTO. v1/v2 retain prepared compatibility. Requirement is always opaque ClientStrings context. |
| Armour, unknown extensions | Fixed prepared fields and metadata. Source context changes fail pending an explicit interpretation. |

The policy records the prepared source's original version and manifest. Its
embedded metadata is the actual input; historical metadata paths are authoring
references, not files implicitly opened by the producer. The official reference
has unknown patch applicability and supplies group anchors and explicit new-node offsets, not target numeric stats.

The fixed F has 5152 graph instances. Of these, 239 have explicit raw-definition
hash exclusions for layout or unreleased/conditional eligibility. The output
contains 4913 nodes, including 368 mastery icons; graph presence is not a claim
that a node can be allocated. Changing an excluded definition requires a new
eligibility decision. New normal members with supported context and an explicit
geometry binding can be emitted; removed members disappear. These are policy
observations, not compiled node IDs or a fixed output count.

## Supplemental ascendancy source

The current materialized v4 policy includes the v2 ownership rules. Add
`--supplement /absolute/selected-tables --supplement-manifest-sha256 "$SUPPLEMENT_SHA"`
to the invocation above. The [selected-table envelope](ordinary-catalysts.md)
must match F's game/patch/realm/schema and known producer. The v2 policy requires
the supplement; v1 rejects it and remains reproducible.

Target `Ascendancy.Character`, IDs and names regenerate the metadata membership;
`PassiveSkills.Ascendancy` regenerates node ownership. The 14 exact placeholder
exclusions are explicit eligibility policy, not a `Disabled`-only release law.
New supported members propagate, removed members disappear or reject unresolved
consumers, and changed/unused exclusions fail. Existing metadata extensions and
relative ordering survive for retained IDs. `BaseAscendancy`, ClassNo and UI data
remain in the receipt, without inventing allocation or historical ordinal laws.

`AscendancyPassiveSkillOverrides` supplies replacement owners and targets through
the same complete CSD/guarded text path as class overrides. Removal, renaming,
new values and target changes propagate. A new unsupported target cannot inherit
the former target's old text. All 13 source relationships remain in the receipt,
including any not used by current emitted nodes. Only the source-owned Ascendancy
field leaves the v2 compatibility context guard; `VisibleForAscendancy`,
Requirement and other unknown conditions remain guarded.

## Source-owned unlock identities

The materialized v4 policy includes v3 unlock ownership with
the same supplemental input as v2. `PassiveSkills.UnlockedBy` is an ordered array
of physical **PassiveSkills row references**. Each referenced definition supplies
its `PassiveSkillGraphId`, in the same node namespace as the graph and ordinary
Build Code. `VisibleForAscendancy` is a physical **Ascendancy row reference**;
the supplemental table supplies its stable `Id`. Neither reference is interpreted
as an ordinal in a runtime array. Every emitted prerequisite must resolve to an
emitted node, and every visibility owner must be an available target ascendancy.

These identities populate `PassiveUnlockConstraint.nodes` and `.ascendancy`.
The DTO contract and `AllocationTree::eligible` already require **all** listed
nodes and the matching ascendancy. This conversion preserves that allocation
rule; it adds no alternative-prerequisite interpretation. The fixed source's
200 constraints additionally agree with the prepared input, including member
order, as a baseline cross-check rather than the basis for the rule.

Presence, removal, ordered member changes and visibility FK changes regenerate
the fields. Unknown extensions within a prepared constraint survive. v3 removes
only `UnlockedBy` and `VisibleForAscendancy` from v2's opaque-context guard;
`Requirement`, quest conditions and every other unowned field remain guarded.
`Requirement` references ClientStrings and is never used to create a predicate.
The receipt retains each physical pointer, target row and resolved identity.
Replacement variants have no unlock fields in their DTO, so v3 explicitly
rejects any nonempty prerequisite list or visibility owner on a replacement;
node ownership never waives an unrepresentable variant condition.
v1/v2 policies keep their earlier behavior and original pins.

## Complete non-stat grant text

The current policy was materialized from a small PoBR template and the pinned
external retained version bindings; the producer now runs in the owner repository.
The old complete v1-v4 mapping files are absent from PoBR Git. Its generated
`inputs/passive-tree/codec-policy/0.5.5.5-complete-text-tree-policy.json` uses v4.
Use the same command with the pinned eight-table selected supplement that also
contains ClientStrings, SkillGems and BaseItemTypes. The policy pins the entire
supplement manifest. Each consumed duplicate table must additionally have the
**exact same raw resource identity** as F; matching game/patch/realm alone is
insufficient. The existing selected-table reader still authenticates and checks
the envelope and physical row shapes.

`PassiveSkills.GrantedSkill` references **SkillGems**, whose `BaseItemType`
references BaseItemTypes. Its source `Name` fills ClientStrings
`ItemDisplayGrantedSkillNoScaling`. SkillPointsGranted and WeaponPointsGranted
fill the singular/plural `PassiveNodeGrantsPassivePoint(s)` and
`PassiveNodeGrantsSpecialisationPoint(s)` templates. These are source text
conversions into the existing ordinary text channel, with no new grant or
allocation calculation. The shared renderer owns template escaping and formats;
replacement strings are not parsed again.

All stat text and all known non-stat effects must succeed together. Any
PassiveSkillBuffs value, unsupported stat or template keeps the **entire** node
on exact whole-text compatibility. Skill names do not replace unknown buff
effects. Negative point grants have no proven template and reject changed input.
Removing grants propagates when the remaining source still explains the node;
a source with no text for a nonempty prepared node remains guarded rather than
silently erasing unexplained text.

v4 separates two guards. The original v3 `context_sha256` remains unchanged for
fallback, including every original non-stat field value. A new
`render_context_sha256` omits only GrantedSkill, SkillPointsGranted and
WeaponPointsGranted, permitting their supported values to change. Passive buffs,
Requirement and all other unowned context stay in both guards. A changed grant
cannot reuse old text merely because another effect needs fallback. The pinned
supplement also binds the resolved skill names and templates used by this policy.

Receipts label complete combined text `target-csd-and-client-strings` and retain
both CSD and non-stat evidence. The current CSD-only translation selector skips
this owner; the new client text has no additional language-coverage claim.
v1/v2/v3 retain their original policies and behavior.

## Complete text and compatibility

The renderer preserves signed source integers and selects `#` or exact/range
conditions before formatting. It supports indexed placeholders, checked integer
`negate`/`double`/`negate_and_double`, and these display-only unit handlers:
`divide_by_one_hundred`, `divide_by_ten_1dp_if_required`,
`per_minute_to_per_second`, `per_minute_to_per_second_2dp_if_required`,
`milliseconds_to_seconds`, `milliseconds_to_seconds_1dp`, and
`milliseconds_to_seconds_2dp_if_required`. Their precision follows the pinned
PyPoE numeric-format definitions; the checked-in reference cases cover signed
limits, rounding and zero. `canonical_line` is a zero-argument display marker.
Raw values and formatted literals are both recorded. These are text formatting
operations, not new calculation rules.

Ordinary passive/item callers exclude only the known gem-quality-only branch.
Unknown syntax, restrictions or transforms still reject the complete text.
Duplicate descriptors can share an owner only when their complete English
semantics agree after removing source positions; all original pointers/hashes
remain in the receipt. Other language disagreements still go through the
translation library's ambiguity checks. Every compound member must select the
same effective descriptor; a parent compound cannot consume a child-owned or
ambiguous member, regardless of source order. Missing compound values are never
invented as zero. `no_description` is not an effectless law. v1/v2/v3 non-stat
grants, and v4 effects outside the complete grant-text contract above, require
complete compatibility text.

A fallback requires exact hashes of both the target non-owned context and the
complete ordered stat/value list. Class replacement fallback additionally requires
the prepared variant alias to identify the current target. A changed target may
use complete target rendering, but cannot borrow the previous target's text.
A changed unsupported value is an error,
never a silent old numeric line or partial native packet. A renderable value or
name can change without passing an old-value equality gate. Unknown prepared
node and variant fields retain their original JSON value/presence.

For the fixed inputs, 4803 nodes use target CSD text (including empty-text layout
icons), 1228 of those are multi-stat nodes, and 110 use guarded complete text.
These counts are production observations, not calculation-accuracy percentages.
The existing mapper still produces 177 eligible single-stat native packets as
an optimization. Multi-stat nodes use the same ordinary parser as other text;
there is no new calculation engine, per-node rule table or profile expansion.

## Pins, receipts and future updates

The reader selects five physical tables (`PassiveSkills`, `PassiveSkillTrees`,
`Stats`, `Characters`, `ClassPassiveSkillOverrides`), `graph.json`,
`passives.json` and the source lock. Only the instance projection from
`passives.json` is consumed; definitions and class overrides are resolved from
the physical tables. Raw graph roots, splines and group flags are validated or
retained in source evidence, not interpreted as new allocation rules.

The domain output remains eight files (the supplemental manifest and full raw
relationship inventory are also embedded in the receipt): `_domain_candidate.json`, two base JSON files,
verbatim `legacy-passive-tree.json` and `codec-policy.json`, source manifest and
lock, and `producer-receipt.json`. All conversion precedes output reservation.
Existing protections reject overwrite, symlinks and output inside input or
production data directories. CURRENT and runtime guards are unchanged.

The receipt binds selected payloads, auxiliary inputs, compiled source identity,
each node's row, complete stats, chosen CSD record/branch/raw-slice hash,
fallback reason, geometry ownership, variants, excluded edges and output hash.
CSD and source acquisition retain their draft status. Compatibility fingerprints
are change detectors, not proof that older text or offsets apply to the target
patch. Future policy updates must separately justify compatibility changes;
simply refreshing all guard hashes would remove that protection.

Regression tests mutate names, multiple values, edges, membership, group centres,
class replacements and exclusions. They require propagation or explicit refusal.
Real validation must additionally assemble a new ordinary package and exercise
BuildData with its new identity. A synthetic value-mutation package is a test
fixture, not a newly acquired official source.

## Historical prepared replay

Omitting both tree options selects the original prepared-preserving producer
with `0.5.5.5-prepared-compat-policy.json` (SHA256
`93078d21dbe2e894061f5e1b9330535adfd7ddf2c0679fd8b7957595c872e260`).
That path reads the original four tables, preserves complete prepared entries
and its 15 exact outside-connection exceptions, and only updates qualified
native packets plus base class attributes. Its source/policy and frozen results
remain reproducible. It is not the target graph/text migration mode above.
