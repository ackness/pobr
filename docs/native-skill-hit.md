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

All admitted profiles select exact Character7
`Metadata/Characters/Int/IntFourb` at actor level 1, with envelope locations 0/0
and quality 0. The two historical profiles retain the fixed gem
`Metadata/Items/Gems/SkillGemSpark` → `SparkPlayer` → primary `SparkPlayer`,
exact gem level 1 and their original closed stat inventories.

The opt-in primary-spell profile selects one entry and an exact authored level
from a sealed `NativePrimarySpellProjection` v2. Its initial inventory is Spark,
Fireball (`Metadata/Items/Gem/SkillGemFireball`, singular `Gem`) and Frostbolt.
Catalog gem/effect/primary-set joins and all request mirrors must agree. No display
name, vendor ordinal, quality branch, additional set, nearby row, interpolation or
extrapolation can select a hit. Authored levels are independent of the actor's
level and do not certify requirements or obtainability.

`NativeData::load` checks the typed primary-only/quality-zero inventory against
sealed input-lock `primary_gems`, reference-index `skill_selection` and coverage
`projected.primary_spells`. It validates unique gem/GemEffect/effect/primary-set
identities, complete unique positive paired catalog levels, retained level identities,
ordered physical references and their archive resource inventory. ActiveSkills rows
may be shared by effects. An empty retained reverse effect is accepted through
the authoritative forward join; a nonempty disagreement rejects. Excluded additional
sets remain ordered unique stable IDs and never enter the projected primary catalog.
Retained raw coefficients, scaling coordinates and cooldown categories stay archival;
calculation never reads raw tables or reinterprets those values. Unknown/duplicate
projection fields and inconsistent evidence reject. Historical projection loading
remains unchanged.

Display labels never classify the skill. Source categorical IDs select its element;
`skill_type_flags` plus `stat_set_hit_flags` select delivery. Effect-wide Area on
Fireball/Frostbolt does not turn their primary projectile into an explosion.
Unknown or duplicated explicit/implicit stats, nonfinite values, wrong source channels,
and incomplete or unordered elemental min/max pairs reject. Relevant unresolved
actor-policy dispositions also reject.

The model assumes ordinary stationary self-cast, explicit Act1 with no quests,
and a successful damaging hit. Only these exact input/interpretation/actor-policy
combinations are admitted (both normalization and rules use the interpretation):

| Input profile | Interpretation | Actor policy (version 1) |
|---|---|---|
| `sorceress-spark-level1-q0-v1` | `spark-level1-q0` | `selected-player-generic-ot-v1` |
| `sorceress-spark-level1-q0-ordinary-damage-passives-v1` | `spark-level1-q0-ordinary-damage-passives` | `selected-player-generic-ot-ordinary-damage-passives-v1` |
| `selected-player-primary-spell-hit-q0-passives-v1` | `primary-spell-hit-q0-passives` | `selected-player-generic-ot-primary-spell-hit-passives-v1` |

Mixed combinations fail. The historical tuples require output/evaluation
`conditional-skill-hit-v1`; only the primary-spell tuple requires
`conditional-skill-hit-v2`. Manifest 4, player contract 2, normalization/rules 2
and actor artifact 1 remain unchanged. The original no-allocation profile still
rejects every allocation. Each expanded profile requires its separately assembled
candidate and actual full policy hash; neither patch text nor UI `tree_version`
can widen old saved pins. All policies retain the source-parsed critical bonus
under an explicit generic-OT applicability assumption, without asserting AO-to-OT
inheritance. Starting relations grant nothing. `class_name` stays display text.

All Build, BuildConfig, SocketGroup and GemSkillRef fields are destructured
explicitly, forcing new fields through scope review. Admission rejects equipment,
jewels, limbs, both utility states, other weapons/allocations, masteries, attribute choices,
ascendancy, party data, extra groups/gems including disabled extras, config
conditions/multipliers/global text, and both raw input maps even at false/zero.
Bandit must be None, campaign Some(Act1), and enemy tier absent. Classification
must be neutral false/false/None for the primary-spell profile. Historical profiles
also accept exactly false/true/Lightning; classification never derives core scope. Source skill types and implicit stats derive that scope instead.

The single group/gem must be enabled with present, agreeing active mirrors.
Build main group and group main effect are absent or 1. Source, granted-level
and reservation overrides are absent. Weapon binding is absent or matches the
active set. Group/gem counts are absent or 1, Full DPS inclusion is false, and
global switches, prepared levels, corruption, name/minion/vendor selectors are
excluded. Prepared requirement levels must be empty for the primary-spell profile. Historical
profiles retain their empty-or-`[1]` behavior without certifying requirements. View, slot and tree-version UI state remains in the fingerprinted
owned request.

Nonzero consumed stats pass through the existing owned mapper with catalog None;
admission inspects mapping observations and the complete returned modifier shape.
A zero damage minimum is explicitly checked against its owned zero rule and
recorded as consumed despite the mapper's deliberate zero elision. Unknown zero
stats cannot pass. Skill Fire/Cold/Lightning damage BASE modifiers and, under either passive-enabled
profile, admitted passive damage INC modifiers enter the shared `calculate_conditional_hit` operation.
Its target remains None and its required critical base comes from the selected
rows/policy. Existing core rounding applies.
No complete build, pool, accuracy, enemy, use-time, cost or server-tick path runs.
The core entry permits damage-type tags only on typed BASE ranges and
typed/elemental INC names. Generic/source damage and critical increases with
those tags fail before queries, since their shared readers use an untyped context.

Under either passive-enabled profile, `build.tree.allocated_nodes` names unique graph IDs in
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

Intrinsic count uses the owned projectile BASE mapping when an explicit source stat
exists. Projectile lifetime uses the duration BASE mapping only for the established
stable Spark primary interpretation backed by its CSD role. V1 still requires both
values and serializes the same numeric fields. V2 uses `Option<f64>` internally:
Fireball/Frostbolt have no count stat and report `total_projectiles: null`;
Fireball has no duration and reports lifetime null. Frostbolt retains its raw
4000 ms skill duration with an outside-output disposition: its projectile-lifetime
role is unestablished, so lifetime is also null. Each missing quantity has an explicit
`unavailable` reason. No projectile flag supplies count 1, absent duration supplies
0, or generic duration-to-lifetime assumption fills the gap.

Every recognized source channel receives a disposition. Stationary movement stats
and Fireball infusion are inactive under the excluded-movement/no-infusion model.
Frostbolt chill magnitude remains outside hit damage and never supplies hit MORE.
Contact, aiming, range, path, pierce and re-hit facts do not multiply one successful
hit. Additional sets stay excluded. Raw nonzero CooldownGroup/Bypass categories are
retained; absent normalized cooldown duration does not mean all raw cooldown metadata
is zero. These dispositions make no broader effectlessness claim.

Pre-resolved damage is used once; BaseEffectiveness remains reference evidence.
Positive cast milliseconds are analytical. Cost requires a complete positional
pair, an existing uniquely identified Mana resource, a positive divisor and
instantaneous semantics; amount per use is raw/divisor. The new projection binds
source Mana slot 0/divisor 1. No fallback exists. These values certify neither
affordability nor requirements, repeated use or payment timing.

`NativeSkillResult` derives Serialize and contains typed per-damage ranges/means,
critical probability/multiplier, conditional weighted mean, analytical use
milliseconds, cost per use, optional intrinsic projectile total/lifetime, complete selection/dataset/
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
signed values are explicitly synthetic. The primary-spell regressions additionally use an explicitly synthetic sealed
selection/index fixture with varied damage, cost, critical chance and physical rows;
it does not certify a real package or producer.
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

The separate primary-spell actual-pack case requires both explicit inputs:

```sh
POBR_NATIVE_PRIMARY_SPELL_PACK=<new-immutable-pack> \
POBR_NATIVE_PRIMARY_SPELL_REQUEST=<new-bound-document> \
  mise run test build lib native_skill::tests::primary_spells::supplied_actual_primary_spell_request_reopens_and_matches_memory -- --ignored
```

This and the portable tests are coordinator verification inputs, not evidence of
an executed Rust, candidate, CLI or WASM pass. Real immutable assembly and public
CLI/rebuilt WASM acceptance belong to integrated validation.
