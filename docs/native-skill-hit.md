# Native conditional skill hit

The public API is `pobr_build::native_skill`:

```rust,ignore
pub fn admit_native_skill(
    data: &pobr_gamedata::NativeData,
    request: &NativeBuildRequest,
) -> Result<AdmittedNativeSkill, NativeAdmissionError>;

pub fn calculate_native_skill(
    admitted: &AdmittedNativeSkill,
) -> Result<NativeSkillResult, NativeCalculationError>;
```

The caller loads an explicitly supplied immutable pack through `NativeData::load`
and constructs or strictly reopens a [native build document](native-build-document.md).
Admission compares all six saved dataset fields with the native snapshot: game,
realm, patch, tree resource SHA, assembly key and final runtime identity SHA.
It requires contract v2, player facts, the conditional output/input/interpretation
profiles, and the exact actor-policy name/version/artifact SHA. It never searches
CURRENT or substitutes a same-patch pack. Queryable v1 data is not admitted.

Both admitted profiles select exact Character7
`Metadata/Characters/Int/IntFourb`, actor level 1, and gem
`Metadata/Items/Gems/SkillGemSpark` → `SparkPlayer` → primary stable set
`SparkPlayer`, exact level 1/quality 0. The envelope locations are 0/0. Stable
catalog joins must be unique; no nearest-level, cold branch or vendor ordinal
fallback exists. Selected explicit/implicit stat inventories are closed: new,
missing, duplicated, nonfinite or unmodelled selected channels fail admission.
Relevant unresolved actor-policy dispositions also fail.
Display names, item-class labels and form labels never classify or select the skill;
the native base/gem/effect/stat-set identities and source categorical IDs do.

The model assumes ordinary stationary self-cast, explicit Act1 with no quests,
and a successful damaging hit. Only these exact input/interpretation/actor-policy
combinations are admitted (both normalization and rules use the interpretation):

| Input profile | Interpretation | Actor policy (version 1) |
|---|---|---|
| `sorceress-spark-level1-q0-v1` | `spark-level1-q0` | `selected-player-generic-ot-v1` |
| `sorceress-spark-level1-q0-ordinary-damage-passives-v1` | `spark-level1-q0-ordinary-damage-passives` | `selected-player-generic-ot-ordinary-damage-passives-v1` |

Mixed combinations fail. The old profile still rejects every allocation. The new
profile requires a separately assembled immutable candidate and its actual policy
hash; it is never selected by default, patch text or UI `tree_version`. Both
policies supply source-parsed critical bonus under an explicit generic-OT
applicability assumption. Neither policy asserts AO-to-OT inheritance. Starting passive choices grant nothing. Compatibility
`class_name` is retained as display text and never supplies actor stats.

All Build, BuildConfig, SocketGroup and GemSkillRef fields are destructured
explicitly, forcing new fields through scope review. Admission rejects equipment,
jewels, limbs, both utility states, other weapons/allocations, masteries, attribute choices,
ascendancy, party data, extra groups/gems including disabled extras, config
conditions/multipliers/global text, and both raw input maps even at false/zero.
Bandit must be None, campaign Some(Act1), and enemy tier absent. Classification
must be false/false/None or exactly false/true/Lightning; it never derives core
scope. Source skill types and implicit stats derive that scope instead.

The single group/gem must be enabled with present, agreeing active mirrors.
Build main group and group main effect are absent or 1. Source, granted-level
and reservation overrides are absent. Weapon binding is absent or matches the
active set. Group/gem counts are absent or 1, Full DPS inclusion is false, and
global switches, prepared levels, corruption, name/minion/vendor selectors are
excluded. Requirement levels are empty or `[1]` and retained without certifying
requirements. View, slot and tree-version UI state remains in the fingerprinted
owned request.

Nonzero consumed stats pass through the existing owned mapper with catalog None;
admission inspects mapping observations and the complete returned modifier shape.
A zero damage minimum is explicitly checked against its owned zero rule and
recorded as consumed despite the mapper's deliberate zero elision. Unknown zero
stats cannot pass. Skill damage BASE modifiers and, under the new profile, admitted
passive damage INC modifiers enter the shared `calculate_conditional_hit` operation.
Its target remains None and its required critical base comes from the selected
rows/policy. Existing core rounding applies.
No complete build, pool, accuracy, enemy, use-time, cost or server-tick path runs.
The core entry permits damage-type tags only on typed BASE ranges and
typed/elemental INC names. Generic/source damage and critical increases with
those tags fail before queries, since their shared readers use an untyped context.

Under the new profile, `build.tree.allocated_nodes` names unique graph IDs in
`NativeData::passive_nodes()`. Duplicate request IDs and ambiguous catalog graph
IDs fail. Nodes are sorted by graph ID for numerical addition and evidence; the
original document array order and complete request hash are retained. No node is
granted automatically, including a player's starting relations. Names, class
labels and tree-version UI labels cannot choose a definition or replacement.

Every selected node must be ordinary, untransformed and unconditional-player,
with a complete single-stat packet. Packet patch/tree compatibility tag, graph
ID, definition slug, passive identity and whole display text must agree. Source
references must be the exact tree-parent manifest's PassiveSkills/Stats resources,
hashes and row pointers. The loader separately validates packet consistency with
the full sealed reference index. Membership in `allowed_source_manifests` alone
cannot authorize mixing passive sources.
Within that exact parent and resource inventory, a PassiveSkills row must keep
one definition/graph identity and a Stats row must keep one stable stat ID.
Different passive rows can share the same stat ID and Stats row. Both exact
references must occur once; their array order is retained as evidence and does
not affect eligibility.

Admission calls `select_native_passive` and accepts only its successful native
modifier, with numeric, tag/flag-free INC semantics for PhysicalDamage,
FireDamage, ColdDamage, LightningDamage, ChaosDamage, ElementalDamage, SpellDamage
or ProjectileDamage. Signed and known-zero source values survive. Missing,
unknown (including zero), incomplete, multi-stat, context-invalid, transformed
and resistance-only packets fail the entire selection before evaluation. A valid
text line cannot rescue a rejected packet. No legacy text parser or partial
application runs. The number of eligible packets in one source pack is not an
allow-list or an admission condition.

**Allocation legality is unavailable.** Selected eligible nodes are explicitly
assumed active for conditional comparison. Point budget, reachability and
progression are not certified, even for actor level 1 or disconnected selections.
No free starting node, connecting path or level-derived point grant is invented.
The result records this assumption and an `allocation-legality` unavailable reason.

The owned total-projectile BASE and duration BASE mappings supply intrinsic
outputs directly from their numeric payloads. There is no added default
projectile and no multiplication of hit damage by projectiles, lifetime or a
re-hit interval. BaseEffectiveness is reference evidence; resolved damage is not
scaled by it again. Positive cast milliseconds are analytical. Cost requires a
complete positional pair, an existing uniquely identified Mana resource, a
positive divisor and instantaneous semantics; amount per use is raw/divisor.
No resource-zero or divisor fallback exists. These values certify neither
affordability nor requirements, repeated use or payment timing.

`NativeSkillResult` derives Serialize and contains typed per-damage ranges/means,
critical probability/multiplier, conditional weighted mean, analytical use
milliseconds, cost per use, projectile total/lifetime, complete selection/dataset/
request pins, `conditional-model` status, assumptions and unavailable reasons.
Provenance retains the small native contract (including source, acquisition,
normalization, rules and reference-index pins), exact runtime JSON pointers,
the actor-disposition artifact reference and coverage counts, every selected
skill fact's disposition and actual shared-core
modifier/query evidence. `skill_facts` remains skill-only. Passive evidence keeps
the exact `base/passive_tree.json#/<index>/native_stats/stats/0` reference and the
owned `node.<graph-id>` origin including `origin.native_stat`; two nodes with the
same stat/value remain distinct. Selected node references join the existing
`provenance.selection_sources`. Modifier origins and rule provenance are preserved.
For hit queries, `counted` means that a nonzero input entered an aggregation, not
that it changed the final damage. PhysicalDamage INC may count against the core's
zero Physical baseline without changing Lightning damage. Fire/Cold/Chaos INC
cannot create a base component; their actual reader evidence is retained. Known
zero modifiers remain visible with `counted: false`. The critical trace projects
actual core nodes/edges; intrinsic mapping readers are
labelled separately from core queries. The sealed physical source archive stays
in the pack. Detailed actor dispositions are loaded once with their sealed policy;
each result references that artifact instead of cloning and serializing its entire
inventory for every calculation.

Unavailable outputs include pools, requirements, hit probability/count, effective
cadence, DPS, cost/second, sustain, mitigation, ailments, defence, recovery and EHP.
No optional OutputTable with fabricated zero values is returned. An explicit
[native CLI consumer](native-cli.md) and explicit [native WASM exports](native-wasm.md)
use this API; ordinary native BuildData/ruleset guards remain closed. Web UI
dispatch is not exposed.

Coordinator verification commands (Rust execution is required before claiming a pass):

```sh
mise run test build lib native_skill::
POBR_NATIVE_SKILL_PACK=<immutable-pack> mise run test build lib native_skill::tests::supplied_actual_pack_save_reopen_and_conditional_outputs -- --ignored
```

The default suite reuses the existing synthetic player archive and a small
source-pinned skill projection, with separately varied damage/chance/bonus and
intrinsic inputs. The actual-pack test is explicitly ignored by default, requires
the supplied path when invoked, and never silently skips a missing input.
Portable passive tests isolate packet admission, owned mapping, shared aggregation
and public evidence from the new-profile loader; their hashes/rows and varied
signed values are explicitly synthetic. They do not forge a new policy/index fixture.
A separate actual-profile case requires a genuinely assembled candidate and a
complete baseline document bound to its generated pins:

```sh
POBR_NATIVE_PASSIVE_PACK=<new-immutable-pack> \
POBR_NATIVE_PASSIVE_REQUEST=<new-bound-baseline-document> \
  mise run test build lib native_skill::tests::supplied_actual_passive_pack_edits_save_remove_and_reopen -- --ignored
```

The actual case checks empty, one-node, two-equal-stat and four-scope selections,
remove/reopen, node-specific evidence and whole-selection failure. It preserves
the old ignored actual-pack baseline contract. Neither
fixtures nor test expectations constitute source-acquisition approval.
