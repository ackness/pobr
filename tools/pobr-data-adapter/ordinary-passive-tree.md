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
  --codec-policy /absolute/path/to/pipeline/passive-tree/0.5.5.5-target-tree-policy.json \
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
| Class replacements | Target class relations determine membership, aliases, names and complete text. Fixed class bindings identify old entries to replace/remove. Other variants remain guarded complete compatibility. |
| Base class attributes | Target Characters values. |
| Ascendancy IDs, unlock predicates, armour, unknown extensions | Fixed prepared fields and metadata. Source context changes fail pending an explicit interpretation. No Ascendancy ordinal or Requirement predicate is guessed. |

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

## Complete text and compatibility

The renderer handles integer values, `#`, exact/range conditions, indexed integer
placeholders and the explicitly supported integer transforms `negate`, `double`
and `negate_and_double`. It verifies lexical handler coverage. Unknown syntax,
restrictions, transforms, ambiguous descriptor ownership, missing compound
members or overlapping coverage reject the whole node's rendering.
`no_description` is not treated as an effectless law. Non-stat grants also
require complete compatibility text.

A fallback requires exact hashes of both the target non-owned context and the
complete ordered stat/value list. A changed unsupported value is an error,
never a silent old numeric line or partial native packet. A renderable value or
name can change without passing an old-value equality gate. Unknown prepared
node and variant fields retain their original JSON value/presence.

For the fixed inputs, 4624 nodes use target CSD text (including empty-text layout
icons), 1129 of those are multi-stat nodes, and 289 use guarded complete text.
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

The output remains eight files: `_domain_candidate.json`, two base JSON files,
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
