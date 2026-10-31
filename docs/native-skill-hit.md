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

The first profile selects exact Character7
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
and a successful damaging hit. The named
`selected-player-generic-ot-v1`/version 1 policy supplies source-parsed critical
bonus under an explicit generic-OT applicability assumption. It does not assert
AO-to-OT inheritance. Starting passive choices grant nothing. Compatibility
`class_name` is retained as display text and never supplies actor stats.

All Build, BuildConfig, SocketGroup and GemSkillRef fields are destructured
explicitly, forcing new fields through scope review. Admission rejects equipment,
jewels, limbs, both utility states, other weapons/allocations, all passive choices,
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
stats cannot pass. Only damage BASE modifiers enter the shared
`calculate_conditional_hit` operation. Its target remains None and its required
critical base comes from the selected rows/policy. Existing core rounding applies.
No complete build, pool, accuracy, enemy, use-time, cost or server-tick path runs.
The core entry permits damage-type tags only on typed BASE ranges and
typed/elemental INC names. Generic/source damage and critical increases with
those tags fail before queries, since their shared readers use an untyped context.

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
modifier/query evidence. Modifier origins and rule provenance are preserved. The
critical trace projects actual core nodes/edges; intrinsic mapping readers are
labelled separately from core queries. The sealed physical source archive stays
in the pack. Detailed actor dispositions are loaded once with their sealed policy;
each result references that artifact instead of cloning and serializing its entire
inventory for every calculation.

Unavailable outputs include pools, requirements, hit probability/count, effective
cadence, DPS, cost/second, sustain, mitigation, ailments, defence, recovery and EHP.
No optional OutputTable with fabricated zero values is returned. An explicit
[native CLI consumer](native-cli.md) uses this API; ordinary native
BuildData/ruleset guards closed. WASM/Web dispatch is not exposed yet.

Coordinator verification commands (Rust execution is required before claiming a pass):

```sh
mise run test build lib native_skill::
POBR_NATIVE_SKILL_PACK=<immutable-pack> mise run test build lib native_skill::tests::supplied_actual_pack_save_reopen_and_conditional_outputs -- --ignored
```

The default suite reuses the existing synthetic player archive and a small
source-pinned skill projection, with separately varied damage/chance/bonus and
intrinsic inputs. The actual-pack test is explicitly ignored by default, requires
the supplied path when invoked, and never silently skips a missing input. Neither
fixtures nor test expectations constitute source-acquisition approval.
