# Ordinary skills from pinned facts

`--ordinary-domain skills` converts authenticated F physical tables through the
existing `skills/{gems,effects,levels,stat_sets}.rs` converters and ordinary DTOs.
It considers all skill families, including attacks, supports, minions and
compound skills. It adds no skill whitelist, runtime profile or calculation rule.

```sh
mise exec -- cargo run --locked -p pobr-data-adapter -- \
  --ordinary-facts /absolute/path/to/F \
  --facts-manifest-sha256 56e71e00a72c8079d100b64861fc030206b12aaeb5ad655ccb4ab4e6726332d0 \
  --version 0.5.5.5 --ordinary-domain skills \
  --out /absolute/path/to/existing-parent/fresh-skills
```

The output parent must exist and the leaf must be fresh and outside `data/`.
The external manifest pin, selected payloads, embedded lock and target patch are
checked by `native_facts::reader::open`. `Tables::selected` supplies the existing
physical row/identity checks. All validation and serialization complete before
the protected output directory is reserved. No vendor, download, implicit package
fallback, production manifest or CURRENT update is involved.

The selected tables are BaseItemTypes, SkillGems, GemEffects, GemTags,
ItemExperienceTypes, ItemExperiencePerLevel, GrantedEffects, ActiveSkills,
ActiveSkillType, GrantedEffectsPerLevel, GrantedEffectStatSets,
GrantedEffectStatSetsPerLevel, GrantedEffectLabels, Stats and CostTypes. Other
source tables and their foreign keys are not certified by this domain.

## Shared conversion and field contracts

The tool-local `skills::SkillTables::read_table<T>` supplies the existing typed
rows. Its `Path` implementation still calls the original JSON reader. The public
`adapt_skills`, `adapt_stat_sets` and `adapt_cost_types` Path wrappers retain their
behavior. The ordinary physical implementation validates before projecting only
consumed fields into those typed rows in memory. `_index` comes from the verified
physical `_row`, never a values field or stable ID. No flattened raw directory is
written or reopened by the producer.

| Output | Source and existing semantics |
| --- | --- |
| `skill_gems.json` | BaseItemTypes.Id, GemType/Colour/MinLevelReq/three attribute percentages; exactly one GemEffects row supplies variant ID, primary/additional effect IDs and GemTags in order; ItemExperienceType joins the real ItemExperiencePerLevel table for requirements |
| `granted_effects.json` | Original effect ID, active ID/name/types, CastTime, support require/add/exclude token streams and support flags; raw primary stat-set/cost indices and stable additional set IDs |
| `granted_effect_levels.json` | Original cooldown/attack time/costs, signed attack speed multiplier, crit attribution and offhand override, CostMultiplier − 100, Reservation, EffectOnPlayer − 100 and stored-use normalization |
| `granted_effect_stat_sets.json` | Stable primary/additional set identity and order; real labels; constants/implicit/ignored stats; all FloatStats/BaseResolvedValues, AdditionalStats/values and flags; original effectiveness, signed multiplier, actor scaling and per-effectiveness rules |
| `cost_types.json` | Physical index order, stable Id, Divisor and PerMinute; nonpositive Divisor uses the original normalization to 1 and is reported |

`GrantedEffects.IgnoreMinionTypes` additionally fills the same-purpose existing
`ignore_minion_types` boolean in this mode. This direct-source addition is listed
in the receipt; the old raw wrapper keeps its historical false/overlay behavior.
No other DTO or algorithm change is made.

The strict physical boundary checks consumed column type/array/reference metadata,
required values, numeric representation, every consumed FK and declared array
pair, including values on later-filtered rows. Source i32 values must fit i32;
unsigned DTO inputs must also be nonnegative. Signed stat values, crit and
multipliers remain signed. Source floats must be finite and fit f32. Null is
allowed for ActiveSkill, Label and DisplayedName, preserving absence separately
from physical row zero and empty text. Other required numeric facts do not become
invented zero through the legacy clamp/default path.

GPL `BaseMultiplier` is an optional legacy fallback column, absent in this F.
It is not required or fabricated. If declared, its scalar numeric metadata and
values are checked; absence/null keeps the original None behavior. The real
stat-set `BaseMultiplier` is always consumed under its declared i32 contract.

Duplicate table identities/physical keys, gem base identities, and the
(experience type, item level), (effect, level) and (stat set, gem level) keys fail.
Ordered reference lists preserve repetition. In this fixed F, effect
`MAASBaronEndgameBasic` at physical row 1334 has primary 1524 and additional
`[1525, 1525, 1525]`: the existing converter retains a primary plus three resulting
sets. Receipt `repeated_set_references` records the owner, source/target payloads,
target refs and occurrence positions. These positions are not import ordinals.

No new positive-speed, positive-crit, monotonic-source-order or unique-reference
rule is imposed. Stat/value array lengths must match; costs match their owner's
CostTypes by position. Duplicate stat entries remain ordered data. Existing
ignored-stat handling affects the first occurrence; the same shared merge code
handles primary/additional rows. Additional rows pair with primary rows in
**physical source order before output level sorting**, retaining original missing
pair/default effectiveness/multiplier fallbacks. The existing fewer-than-five
source rows rule controls actor-level scaling.

## Receipt and pending compatibility

```text
fresh-skills/
  _domain_candidate.json
  base/skill_gems.json
  base/granted_effects.json
  base/granted_effect_levels.json
  base/granted_effect_stat_sets.json
  base/cost_types.json
  source-manifest.json
  source-lock.json
  producer-receipt.json
```

The receipt binds actual selected source bytes, patch, output hashes and compiled
implementation revision, including all shared/new conversion sources and the
original `catalog/skills.rs`. Existing evidence and receipts keep their original
identities. Default Stats/Mods and explicit base-items retain their files and
receipt contents apart from the actual implementation revision change.

`conversion.provenance` links stable gem/variant, effect, set and experience IDs
to physical source rows and ordered half-open ranges of level rows. Together with
the effect's ordered set references, these ranges reconstruct positional merge
origins without copying tens of thousands of raw level records. The per-table
`unmapped_fields` inventory names unconsumed columns, row scope and how affected
IDs resolve through that provenance or the pinned identity table.

Original placeholder gem exclusions, all-empty stat-set effects, zero/trivial
level omissions, nondecreasing requirement normalization and cost-divisor
normalization retain source identities/ranges and reasons. Omitting an all-empty
stat-set artifact does not delete its GrantedEffect definition. Source stats are
not dropped for being unknown to the current stat mapper.

`pending_compatibility` explicitly lists vendor/import set indices, quality
effect-versus-set applicability, target-patch stat-map applicability, prepared
skill overrides/baseMods and their
derived fields, full minion/equipment mappings, language assets and effect display
ordering outside the existing SkillGemDef. Empty/false compatibility fields do
not certify that those behaviors are absent. No quality, language or vendor
sidecar is produced and no old patch's prepared values are substituted.

The acquisition remains draft. The existing marker prevents sealing the directory
as a complete runtime; receipt states `complete_runtime: false` and
`adoption: not-adopted`. This slice alone does not establish a complete new package
or historical BD/Web coverage.

## Regressions and validation

The small synthetic fixture uses real consumed-column metadata and invented rows,
including attacks, support postfix tokens, minion scaling, compound effects,
reordered physical references/levels, repeated stats/flags, signed/zero/null
values, labels, placeholders and omitted compatibility fields. Regressions check
all five original DTOs, old Path-wrapper equivalence (except the explicit
IgnoreMinionTypes addition), legacy tolerance, strict physical failures, ordered
reference multiplicity, source pin/version/domain/overwrite rejection and the
older domains' deterministic behavior.

Parent validation, **not executed by the implementation worker**:

```sh
mise exec -- cargo test --locked -p pobr-data-adapter --bin pobr-data-adapter
```

Parent reuses its warm target and frozen producer for actual F double replay,
source-field/merge/compatibility comparison and sealing checks. Authored Rust
regressions and actual producer execution remain UNRUN in this worker's handoff.
