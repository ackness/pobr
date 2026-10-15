//! How a calculation arrives at a displayed stat: the steps from its starting
//! value and the modifier lines its stage reads, in a serializable shape.
//!
//! The web contract (`calculateBuildJson`'s `breakdowns` and `main_skill`)
//! and the CLI (`calculate-build --breakdown`) both read a finished
//! [`CalculationSession`] through [`stat_breakdown`] and
//! [`main_skill_breakdown`]; the field names and order are that contract.

use pobr_core::ModEvaluation;
use pobr_core::calc::{CalculationSession, OutputTable, StatFormula};
use serde::Serialize;

use crate::{Build, BuildData};

/// The displayed stats the breakdown covers (PoB2's always-present sidebar
/// stats). Each stat is evaluated over the modifier names its calculation
/// stage aggregates (`CalculationSession::stat_mod_names`). `TotalDPS` and
/// `TotalHitAvg` have no single aggregation: they carry steps only, and the
/// average hit a per-damage-type table (`damage`). `CombinedDPS` and
/// `TotalDotDPS` likewise carry steps only, the latter with the skill's own
/// damage over time per type (`skill_dot`). The three ailment stats list the
/// ailment magnitude lines and carry what the steps are built from (`ailment`).
/// With two weapon passes, the stats each pass computes on its own carry the
/// steps that merge both hands and each hand's own breakdown (`hands`).
pub const BREAKDOWN_STATS: &[&str] = &[
    "CombinedDPS",
    "TotalDPS",
    "TotalHitAvg",
    "TotalDotDPS",
    "IgniteDPS",
    "BleedDPS",
    "PoisonDPS",
    "Life",
    "Mana",
    "EnergyShield",
    "Spirit",
    "Armour",
    "Evasion",
    "FireResist",
    "ColdResist",
    "LightningResist",
    "ChaosResist",
    "ActionRate",
    "CritChance",
    "CritMultiplier",
    "Accuracy",
    "HitChance",
];

#[derive(Debug, Clone, Serialize)]
pub struct BreakdownMod {
    /// `BASE` / `INC` / `MORE` / `FLAG` / `OVERRIDE` / `LIST`.
    pub mod_type: &'static str,
    /// The numeric view (`null` for Flag/Text mod lines). For an active
    /// BASE/INC/MORE line this is the applied number (per-stat scaling and
    /// shared caps included); an inactive line keeps its stored number.
    pub value: Option<f64>,
    /// Whether the inspected skill's calculation counts this line. `false`
    /// means its condition, skill type or weapon requirement is not met.
    pub active: bool,
    /// The mod line's raw text (the source it was parsed from).
    pub source_text: Option<String>,
    /// The attribution source category (`SourceKind`'s Debug name, e.g. `PassiveNode` / `ItemAffix`).
    pub origin_kind: Option<String>,
    /// The attribution source's stable id (node id / item slot / gem id).
    pub origin_id: Option<String>,
    /// The source slot (for equipment mod lines).
    pub slot: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatBreakdown {
    /// The active BASE mod-line total (no class/base injection).
    pub base_total: f64,
    /// The active INC mod-line total (in percentage points).
    pub inc_total: f64,
    /// The product of the active MORE mod lines, as a multiplier (`1.0` when there are none).
    pub more_total: f64,
    /// The per-mod-line source list, inactive lines included.
    pub mods: Vec<BreakdownMod>,
    /// How the calculation gets from the starting value to the displayed
    /// stat (`null` = the steps are not available for this stat).
    pub formula: Option<BreakdownFormula>,
    /// `TotalHitAvg` only: how each damage type of the hit is built.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub damage: Option<HitDamageTable>,
    /// `TotalDPS` only: the `enemy_mitigation` step per damage type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mitigation: Option<Vec<HitMitigationRow>>,
    /// `TotalDotDPS` only: the `skill_dot` step per damage type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_dot: Option<Vec<SkillDotRow>>,
    /// `IgniteDPS` / `BleedDPS` / `PoisonDPS` only: the hit the ailment comes
    /// from and how many instances are active.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ailment: Option<AilmentBreakdown>,
    /// `ActionRate` / `TotalDPS` only: the cycle behind the `reload` step.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reload: Option<ReloadBreakdown>,
    /// `Armour` / `Evasion` / `EnergyShield` only: the `slot_scaled` step
    /// per item slot. The `extra` amounts add up to that step's operand.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub slots: Vec<DefenceSlotBreakdown>,
    /// Two weapon passes only, for the stats each pass computes on its own
    /// (`ActionRate`, `HitChance`, `CritChance`, `CritMultiplier`,
    /// `TotalHitAvg`, `TotalDPS` and an ailment both hands apply): main hand
    /// then off hand. `formula` then holds the steps that merge the two
    /// hands' values, and `damage` / `mitigation` / `ailment` are per hand.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hands: Vec<HandBreakdown>,
}

/// One weapon pass's own breakdown of a stat.
#[derive(Debug, Clone, Serialize)]
pub struct HandBreakdown {
    /// `main_hand` / `off_hand`.
    pub hand: &'static str,
    /// The totals, lines and steps as that pass evaluates them; `formula`
    /// ends at the pass's own value (the operand of the matching
    /// `main_hand` / `off_hand_*` / `stronger_hand` / `weaker_hand` step).
    #[serde(flatten)]
    pub detail: StatBreakdown,
}

/// What the lines scoped to one item slot add to a defence.
#[derive(Debug, Clone, Serialize)]
pub struct DefenceSlotBreakdown {
    /// The slot name, as in `mods[].slot`.
    pub slot: String,
    /// The amount in that slot the lines scale: the item's own value after
    /// keystones and conversion. An amount converted in from another
    /// defence of the same item is its own row.
    pub base: f64,
    /// The INC total (percentage points) and MORE product of the lines
    /// scoped to this slot; they add to and multiply with the global ones.
    pub increased: f64,
    pub more: f64,
    /// `base × ((1 + (global increased + increased) / 100) × global more ×
    /// more − (1 + global increased / 100) × global more)`.
    pub extra: f64,
}

/// The crossbow reload cycle: firing a magazine, then reloading.
#[derive(Debug, Clone, Serialize)]
pub struct ReloadBreakdown {
    /// Uses per second while firing (the value before the `reload` step).
    pub firing_rate: f64,
    /// Bolts fired per magazine, the chance not to consume one included
    /// (`null` when a bolt is never consumed).
    pub bolts: Option<f64>,
    /// Seconds to fire the magazine: `bolts / firing_rate`.
    pub firing_time: f64,
    /// Seconds to reload, the chance to reload instantly included.
    pub reload_time: f64,
    /// `bolts / (firing_time + reload_time)`: uses per second over the cycle.
    /// The `reload` step multiplies by `rate / firing_rate`.
    pub rate: f64,
}

fn mod_entries(rows: &[ModEvaluation<'_>]) -> Vec<BreakdownMod> {
    let mut entries: Vec<BreakdownMod> = rows
        .iter()
        .map(|row| {
            let m = row.modifier;
            BreakdownMod {
                mod_type: m.mod_type.as_trace_label(),
                value: row.value,
                active: row.active,
                source_text: m.source.clone(),
                origin_kind: m.origin.as_ref().map(|o| format!("{:?}", o.source_id.kind)),
                origin_id: m.origin.as_ref().map(|o| o.source_id.id.clone()),
                slot: m.origin.as_ref().and_then(|o| o.slot.clone()),
            }
        })
        .collect();
    // Establish a fixed order: ModDb iteration order is affected by the
    // underlying HashMap instance, so different data backends can produce
    // different orderings; the output is sorted by (type, origin, mod-line
    // text, value) to keep the contract byte-deterministic and the UI display stable.
    entries.sort_by(|a, b| {
        (a.mod_type, &a.origin_kind, &a.origin_id, &a.source_text)
            .cmp(&(b.mod_type, &b.origin_kind, &b.origin_id, &b.source_text))
            .then(
                a.value
                    .partial_cmp(&b.value)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(b.active.cmp(&a.active))
    });
    entries
}

/// One damage type of the main skill's hit, before the enemy's mitigation.
#[derive(Debug, Clone, Serialize)]
pub struct HitDamageRow {
    /// `Physical` / `Fire` / `Cold` / `Lightning` / `Chaos`.
    pub damage_type: String,
    /// The range before increases: the weapon or skill base, added damage
    /// and what conversion and gain moved into this type.
    pub base_min: f64,
    pub base_max: f64,
    /// The INC total (in percentage points).
    pub increased: f64,
    /// The MORE product, as a multiplier.
    pub more: f64,
    /// MORE multipliers that apply to the minimum or the maximum only.
    pub more_min: f64,
    pub more_max: f64,
    /// The range of a non-critical hit:
    /// `base × (1 + increased / 100) × more × more_min|max × damage_effect`.
    pub min: f64,
    pub max: f64,
    /// The average non-critical hit (`(min + max) / 2` unless rolls are lucky).
    pub avg: f64,
    /// The average critical hit.
    pub crit_avg: f64,
    /// The INC / MORE lines that scale this type only (its own damage name
    /// and, for an element, elemental damage), inactive lines included.
    /// With `shared_mods`, the active INC values add up to `increased` and
    /// the active MORE values multiply to `more` (which the calculation
    /// rounds to two decimals per modifier name).
    pub mods: Vec<BreakdownMod>,
    /// The MORE lines behind `more_min` and `more_max`.
    pub min_mods: Vec<BreakdownMod>,
    pub max_mods: Vec<BreakdownMod>,
}

/// One damage type of the hit before conversion and gain.
#[derive(Debug, Clone, Serialize)]
pub struct HitDamageSource {
    /// `Physical` / `Fire` / `Cold` / `Lightning` / `Chaos`.
    pub damage_type: String,
    /// The range before conversion: for physical the weapon's or skill's
    /// own hit plus `added × added_effect`, otherwise `added × added_effect`.
    pub min: f64,
    pub max: f64,
    /// The totals of the added damage lines, before `added_effect`.
    pub added_min: f64,
    pub added_max: f64,
    /// The added damage effectiveness, `(1 + INC / 100) × MORE` of `added_effect_mods`.
    pub added_effect: f64,
    /// The BASE lines of added minimum and maximum damage (a skill's own
    /// damage of a non-physical type is one of them), inactive lines included.
    pub added_min_mods: Vec<BreakdownMod>,
    pub added_max_mods: Vec<BreakdownMod>,
    pub added_effect_mods: Vec<BreakdownMod>,
    /// The lines that convert this type to another (the skill's own
    /// conversion, then global conversion) and the lines that add a share
    /// of it as another type. How much each one moves is not listed: with
    /// either list active, `rows[].base_min|max` differ from these ranges.
    pub convert_mods: Vec<BreakdownMod>,
    pub gain_mods: Vec<BreakdownMod>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HitDamageTable {
    pub rows: Vec<HitDamageRow>,
    /// Each damage type the hit starts with, before conversion and gain
    /// move it into the `rows` ranges. Types without damage are left out.
    pub sources: Vec<HitDamageSource>,
    /// The INC / MORE lines that scale every damage type of the hit
    /// (damage, attack / spell / projectile / area / melee damage and the
    /// skill's keyword damage names), inactive lines included.
    pub shared_mods: Vec<BreakdownMod>,
    /// The double / triple damage multiplier every range includes (`1.0` without one).
    pub damage_effect: f64,
}

/// One damage type of the average hit against the configured enemy.
#[derive(Debug, Clone, Serialize)]
pub struct HitMitigationRow {
    /// `Physical` / `Fire` / `Cold` / `Lightning` / `Chaos`.
    pub damage_type: String,
    /// The type's share of the average hit (crits included), before the enemy.
    pub average: f64,
    /// The enemy's resistance before penetration (`null` for physical).
    pub resist: Option<f64>,
    /// What a non-critical hit is reduced by, in percent: the resistance
    /// after penetration, or for physical the reduction from the enemy's
    /// armour, flat reduction and Overwhelm. Negative means more damage.
    pub reduction: f64,
    /// The enemy's damage-taken multiplier for this type.
    pub taken: f64,
    /// `mitigated / average`. Equals `taken × (1 − reduction / 100)` unless
    /// critical hits are mitigated differently (armour depends on hit size).
    pub multiplier: f64,
    /// The share after the enemy. The rows add up to the result of the
    /// `enemy_mitigation` step.
    pub mitigated: f64,
    /// The percentage of hits that treat the resistance as inverted
    /// (`0` without such a modifier). `resist` already includes it:
    /// the lines' total times `1 − 2 × resist_inverted / 100`.
    pub resist_inverted: f64,
    /// The enemy's own lines behind `resist` (exposure and curses included;
    /// an active OVERRIDE line replaces the others, and the total is kept
    /// within the resistance limits), or for physical its armour and flat
    /// reduction lines. Inactive lines included.
    pub defence_mods: Vec<BreakdownMod>,
    /// The enemy's own lines behind `taken`: `(1 + INC / 100) × MORE`.
    pub taken_mods: Vec<BreakdownMod>,
}

/// One damage type of the skill's own damage over time (not an ailment).
#[derive(Debug, Clone, Serialize)]
pub struct SkillDotRow {
    /// `Physical` / `Fire` / `Cold` / `Lightning` / `Chaos`.
    pub damage_type: String,
    /// Damage per second before scaling, from the skill gem.
    pub base: f64,
    /// The INC total (in percentage points).
    pub increased: f64,
    /// The MORE product, as a multiplier.
    pub more: f64,
    /// The damage over time multiplier (in percentage points).
    pub dot_multiplier: f64,
    /// The aura effect multiplier (`1.0` unless the skill is an aura).
    pub aura: f64,
    /// The enemy's resistance and damage taken as one multiplier.
    pub enemy: f64,
    /// `base × (1 + increased / 100) × more × (1 + dot_multiplier / 100) × aura × enemy`.
    /// The rows add up to the operand of the `skill_dot` step.
    pub total: f64,
    /// The INC / MORE damage lines and the BASE damage over time multiplier
    /// lines this type reads, inactive lines included. The active INC values
    /// add up to `increased`, the active MORE values multiply to `more`
    /// (rounded to two decimals per modifier name by the calculation).
    pub mods: Vec<BreakdownMod>,
}

/// What a damaging ailment's steps are built from.
#[derive(Debug, Clone, Serialize)]
pub struct AilmentBreakdown {
    /// The non-critical and critical hit the ailment is applied by (the
    /// damage types that can apply it, before the enemy's mitigation), taken
    /// at `damage_roll` percent of the damage range.
    pub source_hit: f64,
    pub source_crit: f64,
    /// 50 = the average hit. Higher when more instances are applied than can
    /// be active, because only the strongest ones count.
    pub damage_roll: f64,
    /// The percentage of instances applied by a critical hit: the step's
    /// starting hit is `source_hit` and `source_crit` averaged with this share.
    pub crit_share: f64,
    /// The chance for a hit to apply the ailment, in percent.
    pub apply_chance: f64,
    /// The chance to hit, in percent.
    pub hit_chance: f64,
    /// How long one instance lasts, in seconds: its duration stretched by
    /// the enemy and divided by the `ailment_rate` step (damage dealt faster
    /// ends sooner).
    pub duration: f64,
    /// Hits per second (uses per second and the skill's damage per second multiplier).
    pub hits_per_second: f64,
    /// `hit_chance × apply_chance × duration × hits_per_second`: the average
    /// number of instances on the enemy. `0` when the skill has no hit rate;
    /// the steps then use `chance_to_apply` instead of `active_stacks`.
    pub stacks: f64,
    /// The most instances that deal damage at once. The `active_stacks` step
    /// multiplies by the smaller of `stacks` and this.
    pub max_stacks: f64,
    /// `source_hit` and `source_crit` per damage type that can apply the
    /// ailment; the rows add up to the two totals.
    pub source: Vec<AilmentSourceRow>,
    /// `duration` before modifiers, in seconds, and the multiplier from how
    /// fast effects expire on the enemy: `duration` is `base_duration`
    /// scaled by the `duration_mods` lines, times `enemy_duration`, divided
    /// by `rate`.
    pub base_duration: f64,
    pub enemy_duration: f64,
    /// How much faster the ailment deals its damage (the `ailment_rate`
    /// step): the same damage in less time.
    pub rate: f64,
    /// The player's lines behind `apply_chance` (the ailment's own chance
    /// name and `AilmentChance`), `duration`, the `ailment_effect` step and
    /// the `ailment_rate` step, inactive lines included. Modifiers on the
    /// enemy that add to these are not listed.
    pub chance_mods: Vec<BreakdownMod>,
    pub duration_mods: Vec<BreakdownMod>,
    pub effect_mods: Vec<BreakdownMod>,
    pub rate_mods: Vec<BreakdownMod>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AilmentSourceRow {
    /// `Physical` / `Fire` / `Cold` / `Lightning` / `Chaos`.
    pub damage_type: String,
    pub hit: f64,
    pub crit: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BreakdownFormulaStep {
    /// `base` / `flat` / `enemy_flat` / `converted` / `increased` / `more` / `added_time` /
    /// `action_speed` / `override` / `cap` / `cooldown_cap` /
    /// `server_tick_cap` / `floor` / `bonus_to_multiplier` / `gear_adjusted` /
    /// `gained` / `slot_scaled` / `flat_after` / `round` / `hit_chance` /
    /// `lucky` / `bifurcate` / `inevitable` / `inevitable_penalty` /
    /// `bifurcate_bonus` / `enemy_increased` / `accuracy_rating` /
    /// `vs_evasion` / `always_hits` / `enemy_block` / `non_crit_hit` /
    /// `crit_hits` / `average_hit` / `enemy_mitigation` / `uses_per_second` /
    /// `chance_to_hit` / `dps_multiplier` / `skill_dot` / `poison_dps` /
    /// `ignite_dps` / `bleed_dps` / `hit_dps` / `dot_dps` / `ailment_source` /
    /// `per_second` / `active_stacks` / `chance_to_apply` / `ailment_effect` /
    /// `ailment_rate` / `enemy_increase_added` / `never_crit` /
    /// `no_crit_bonus` / `main_hand` / `off_hand_average` / `off_hand_added` /
    /// `off_hand_either` / `off_hand_alternating` / `stronger_hand` /
    /// `stronger_share` / `weaker_hand` / `reload` / `hit_interval`.
    pub op: &'static str,
    /// The amount the step applies (percentage points for `increased`,
    /// `converted`, `hit_chance`, `enemy_block`, `inevitable_penalty`,
    /// `bifurcate_bonus` and `enemy_increased`, the enemy's evasion for
    /// `vs_evasion`, a multiplier for `more` and `action_speed`, seconds
    /// for `added_time`, the limit for caps and the floor, the number of
    /// rolls for `lucky` and `bifurcate`, the percentage of hits that crit
    /// for `crit_hits`, the chance to hit for `chance_to_hit`, a multiplier for `enemy_mitigation`,
    /// `uses_per_second`, `dps_multiplier`, `ailment_effect` and `ailment_rate`,
    /// the starting damage per second for `skill_dot` and `hit_dps`, the hit the
    /// ailment starts from for `ailment_source`, the percentage of it dealt per
    /// second for `per_second`, the number of instances for `active_stacks`, the
    /// chance in percent for `chance_to_apply`, the enemy's increase in
    /// percentage points for `enemy_increase_added`, the value the step sets
    /// for `never_crit` and `no_crit_bonus`, a hand's own value for
    /// `main_hand`, the `off_hand_*` steps, `stronger_hand` and
    /// `weaker_hand`, the percentage of instances from the stronger hand for
    /// `stronger_share`, a multiplier for `reload`, the seconds before a
    /// target can be hit again for `hit_interval`, the amount added otherwise).
    pub operand: f64,
    /// The running value after the step.
    pub result: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BreakdownFormula {
    pub steps: Vec<BreakdownFormulaStep>,
    /// The displayed stat value the steps end at.
    pub value: f64,
}

fn formula_steps(formula: StatFormula) -> BreakdownFormula {
    BreakdownFormula {
        steps: formula
            .steps
            .iter()
            .map(|step| BreakdownFormulaStep {
                op: step.op.as_str(),
                operand: step.operand + 0.0,
                result: step.result + 0.0,
            })
            .collect(),
        value: formula.value + 0.0,
    }
}

/// What pass `pass` of the ailment stage computed `stat` with.
fn ailment_breakdown(
    session: &CalculationSession,
    stat: &str,
    pass: usize,
) -> Option<AilmentBreakdown> {
    let stage = session.ailment_stage_terms(stat)?;
    let (_, terms) = stage.passes.get(pass)?;
    let lines = session.ailment_lines(stat)?;
    Some(AilmentBreakdown {
        source_hit: terms.source_hit,
        source_crit: terms.source_crit,
        damage_roll: terms.damage_roll,
        crit_share: terms.crit_weight() * 100.0,
        apply_chance: terms.apply_chance * 100.0,
        hit_chance: terms.hit_chance * 100.0,
        duration: terms.duration,
        hits_per_second: terms.hits_per_second,
        stacks: terms.stacks,
        max_stacks: terms.max_stacks,
        source: stage
            .sources
            .get(pass)
            .into_iter()
            .flatten()
            .map(|part| AilmentSourceRow {
                damage_type: format!("{:?}", part.damage_type),
                hit: part.hit,
                crit: part.crit,
            })
            .collect(),
        base_duration: terms.base_duration,
        enemy_duration: terms.enemy_duration,
        rate: terms.rate,
        chance_mods: mod_entries(&lines.chance),
        duration_mods: mod_entries(&lines.duration),
        effect_mods: mod_entries(&lines.effect),
        rate_mods: mod_entries(&lines.rate),
    })
}

/// The per-type table of offence pass `pass`'s average hit.
fn hit_damage_table(session: &CalculationSession, pass: usize) -> Option<HitDamageTable> {
    let terms = session.hit_damage_terms(pass)?;
    // The physical row is always computed; leave it out when empty.
    let parts: Vec<_> = terms
        .parts
        .iter()
        .filter(|part| part.base_max != 0.0 || part.max != 0.0)
        .collect();
    let types: Vec<_> = parts.iter().map(|part| part.damage_type).collect();
    let lines = session.hit_damage_mods(pass, &types);
    let sources: Vec<_> = terms
        .sources
        .iter()
        .filter(|source| source.min != 0.0 || source.max != 0.0)
        .collect();
    let source_types: Vec<_> = sources.iter().map(|source| source.damage_type).collect();
    let source_lines = session.hit_source_mods(pass, &source_types);
    Some(HitDamageTable {
        sources: sources
            .iter()
            .zip(&source_lines)
            .map(|(source, lines)| HitDamageSource {
                damage_type: format!("{:?}", source.damage_type),
                min: source.min,
                max: source.max,
                added_min: source.added_min,
                added_max: source.added_max,
                added_effect: source.added_effect,
                added_min_mods: mod_entries(&lines.added_min),
                added_max_mods: mod_entries(&lines.added_max),
                added_effect_mods: mod_entries(&lines.added_effect),
                convert_mods: mod_entries(&lines.convert),
                gain_mods: mod_entries(&lines.gain),
            })
            .collect(),
        rows: parts
            .iter()
            .zip(&lines.by_type)
            .zip(&lines.range_more)
            .map(|((part, (_, mods)), (min_mods, max_mods))| HitDamageRow {
                damage_type: format!("{:?}", part.damage_type),
                base_min: part.base_min,
                base_max: part.base_max,
                increased: part.inc + 0.0,
                more: part.more,
                more_min: part.more_min,
                more_max: part.more_max,
                min: part.min,
                max: part.max,
                avg: part.avg,
                crit_avg: part.crit_avg,
                mods: mod_entries(mods),
                min_mods: mod_entries(min_mods),
                max_mods: mod_entries(max_mods),
            })
            .collect(),
        shared_mods: mod_entries(&lines.shared),
        damage_effect: terms.damage_effect,
    })
}

/// What the enemy does to each damage type of offence pass `pass`'s average hit.
fn hit_mitigation_rows(session: &CalculationSession, pass: usize) -> Option<Vec<HitMitigationRow>> {
    let parts = session.hit_mitigation_terms(pass)?;
    let types: Vec<_> = parts.iter().map(|part| part.damage_type).collect();
    let lines = session.hit_mitigation_mods(pass, &types);
    Some(
        parts
            .iter()
            .zip(&lines)
            .filter(|(part, _)| part.average != 0.0)
            .map(|(part, (_, lines))| HitMitigationRow {
                damage_type: format!("{:?}", part.damage_type),
                average: part.average,
                resist: part.resist.map(|resist| resist + 0.0),
                reduction: part.reduction + 0.0,
                taken: part.taken,
                multiplier: part.mitigated / part.average,
                mitigated: part.mitigated,
                resist_inverted: part.invert * 100.0,
                defence_mods: mod_entries(&lines.defence),
                taken_mods: mod_entries(&lines.taken),
            })
            .collect(),
    )
}

fn hand_label(hand: pobr_core::HandTag) -> &'static str {
    match hand {
        pobr_core::HandTag::MainHand => "main_hand",
        pobr_core::HandTag::OffHand => "off_hand",
        pobr_core::HandTag::Single => "skill",
    }
}

/// Each weapon pass's own breakdown of `stat`, when two passes are merged
/// into its displayed value.
fn hand_breakdowns(session: &CalculationSession, stat: &str) -> Vec<HandBreakdown> {
    if let Some(terms) = session.ailment_stage_terms(stat) {
        if terms.merge.is_none() {
            return Vec::new();
        }
        let (base_total, inc_total, more_total) = session.stat_totals(stat);
        return terms
            .passes
            .iter()
            .enumerate()
            .map(|(pass, (hand, terms))| HandBreakdown {
                hand: hand_label(*hand),
                detail: StatBreakdown {
                    base_total: base_total + 0.0,
                    inc_total: inc_total + 0.0,
                    more_total,
                    // Both hands read the lines listed for the ailment itself.
                    mods: Vec::new(),
                    formula: Some(formula_steps(terms.formula())),
                    damage: None,
                    mitigation: None,
                    skill_dot: None,
                    ailment: ailment_breakdown(session, stat, pass),
                    reload: None,
                    slots: Vec::new(),
                    hands: Vec::new(),
                },
            })
            .collect();
    }
    if !matches!(
        stat,
        "ActionRate" | "HitChance" | "CritChance" | "CritMultiplier" | "TotalHitAvg" | "TotalDPS"
    ) {
        return Vec::new();
    }
    session
        .combined_hands()
        .into_iter()
        .enumerate()
        .map(|(pass, hand)| {
            let (base_total, inc_total, more_total) = session.hand_stat_totals(stat, pass);
            HandBreakdown {
                hand: hand_label(hand),
                detail: StatBreakdown {
                    base_total: base_total + 0.0,
                    inc_total: inc_total + 0.0,
                    more_total,
                    mods: mod_entries(&session.hand_stat_mods(stat, pass)),
                    formula: session.hand_stat_formula(stat, pass).map(formula_steps),
                    damage: (stat == "TotalHitAvg")
                        .then(|| hit_damage_table(session, pass))
                        .flatten(),
                    mitigation: (stat == "TotalDPS")
                        .then(|| hit_mitigation_rows(session, pass))
                        .flatten(),
                    skill_dot: None,
                    ailment: None,
                    reload: None,
                    slots: Vec::new(),
                    hands: Vec::new(),
                },
            }
        })
        .collect()
}

/// How the finished `session` arrives at `stat`: a display stat id (see
/// [`BREAKDOWN_STATS`] for the ones the panel shows) or any modifier name,
/// which is aggregated under its own name. `None` when the calculation has
/// neither modifier lines nor steps for it, or the stat is a damaging
/// ailment the skill does not apply.
pub fn stat_breakdown(session: &CalculationSession, stat: &str) -> Option<StatBreakdown> {
    // EHP and the maximum hits list the enemy inputs they use, which are
    // other names than the stat. Their lines do not add up to a total of
    // the stat, so these stats are not in `BREAKDOWN_STATS` (the panel shows
    // a base total); a caller asks for them by name.
    let rows = match stat {
        "TotalEHP" => session.enemy_hit_mods(true),
        "PhysicalMaximumHitTaken"
        | "FireMaximumHitTaken"
        | "ColdMaximumHitTaken"
        | "LightningMaximumHitTaken"
        | "ChaosMaximumHitTaken" => session.enemy_hit_mods(false),
        _ => session.evaluated_stat_mods(stat),
    };
    let formula = session.stat_formula(stat).map(formula_steps);
    // A stat without modifier lines is still explained by its starting value
    // (a weapon's base crit chance, a skill's base rate).
    if rows.is_empty() && formula.as_ref().is_none_or(|formula| formula.value == 0.0) {
        return None;
    }
    let output = session.output();
    let ailment_dps = match stat {
        "IgniteDPS" => Some(output.ignite_dps),
        "BleedDPS" => Some(output.bleed_dps),
        "PoisonDPS" => Some(output.poison_dps),
        _ => None,
    };
    // An ailment the skill does not apply has nothing to explain.
    if ailment_dps == Some(0.0) {
        return None;
    }
    let hands = hand_breakdowns(session, stat);
    // With two weapon passes these are per hand.
    let single_pass = hands.is_empty();
    let ailment = formula
        .as_ref()
        .and_then(|_| session.ailment_stage_terms(stat))
        .and_then(|terms| match terms.passes.as_slice() {
            [_] => ailment_breakdown(session, stat, 0),
            _ => None,
        });
    let damage = (stat == "TotalHitAvg" && single_pass)
        .then(|| hit_damage_table(session, 0))
        .flatten();
    let mitigation = (stat == "TotalDPS" && single_pass)
        .then(|| hit_mitigation_rows(session, 0))
        .flatten();
    let reload = formula
        .as_ref()
        .filter(|formula| formula.steps.iter().any(|step| step.op == "reload"))
        .and_then(|_| session.crossbow_reload())
        .map(|reload| ReloadBreakdown {
            firing_rate: reload.firing_rate,
            bolts: reload
                .effective_bolt_count
                .is_finite()
                .then_some(reload.effective_bolt_count),
            firing_time: reload.total_firing_time,
            reload_time: reload.effective_reload_time,
            rate: reload.effective_rate,
        });
    let skill_dot = (stat == "TotalDotDPS")
        .then(|| session.skill_dot_parts())
        .flatten()
        .filter(|parts| !parts.is_empty())
        .map(|parts| {
            let types: Vec<_> = parts.iter().map(|part| part.damage_type).collect();
            let lines = session.skill_dot_mods(&types);
            parts
                .iter()
                .zip(&lines.by_type)
                .map(|(part, (_, mods))| SkillDotRow {
                    damage_type: format!("{:?}", part.damage_type),
                    base: part.base,
                    increased: part.inc + 0.0,
                    more: part.more,
                    dot_multiplier: part.dot_multiplier + 0.0,
                    aura: part.aura,
                    enemy: part.enemy,
                    total: part.total,
                    mods: mod_entries(mods),
                })
                .collect()
        });
    let slots = formula
        .as_ref()
        .map(|_| session.defence_slot_parts(stat))
        .unwrap_or_default()
        .into_iter()
        .map(|part| DefenceSlotBreakdown {
            slot: part.slot,
            base: part.base,
            increased: part.inc + 0.0,
            more: part.more,
            extra: part.extra,
        })
        .collect();
    let entries = mod_entries(&rows);
    let (base_total, inc_total, more_total) = session.stat_totals(stat);
    Some(StatBreakdown {
        // An empty float sum is `-0.0`; adding zero keeps it out of the JSON.
        base_total: base_total + 0.0,
        inc_total: inc_total + 0.0,
        more_total,
        mods: entries,
        formula,
        damage,
        mitigation,
        skill_dot,
        ailment,
        reload,
        slots,
        hands,
    })
}

/// [`stat_breakdown`] for a minion's `stat` (minion `index` in the order the
/// calculation created it): the lines of the minion's own modifiers that the
/// stat reads, including what the character grants it, and for `Life` the
/// steps from the monster's base Life. `None` without such lines.
pub fn minion_stat_breakdown(
    session: &CalculationSession,
    index: usize,
    stat: &str,
) -> Option<StatBreakdown> {
    let rows = session.minion_stat_mods(index, stat);
    let formula = match stat {
        "Life" => session.minion_life_formula(index),
        _ => session.minion_stat_formula(index, stat),
    }
    .map(formula_steps);
    if rows.is_empty() && formula.is_none() {
        return None;
    }
    let (base_total, inc_total, more_total) = session.minion_stat_totals(index, stat);
    Some(StatBreakdown {
        base_total: base_total + 0.0,
        inc_total: inc_total + 0.0,
        more_total,
        mods: mod_entries(&rows),
        formula,
        damage: None,
        mitigation: None,
        skill_dot: None,
        ailment: None,
        reload: None,
        slots: Vec::new(),
        hands: Vec::new(),
    })
}

/// One damage-type component of the main skill's hit damage (a non-crit
/// leg, player-side, before enemy damage reduction — matches PoB2's Calcs
/// page damage-breakdown basis; `avg` is used for share display).
#[derive(Debug, Clone, Serialize)]
pub struct MainSkillHitPart {
    /// `Physical` / `Fire` / `Cold` / `Lightning` / `Chaos`.
    pub damage_type: String,
    pub min: f64,
    pub max: f64,
    /// `(min + max) / 2`.
    pub avg: f64,
}

/// The main skill's identity (the skill the engine's calculation actually
/// centers on) plus its damage breakdown (the counterpart to PoB2's Main
/// Skill sidebar plus its Calcs damage-breakdown area).
#[derive(Debug, Clone, Serialize)]
pub struct MainSkillBreakdown {
    /// The selected skill group (0-based, aligned with the build's socket groups).
    pub group_index: usize,
    /// PoB's 1-based ordinal among all active effects, including support-granted skills.
    pub active_skill_index: usize,
    /// That group's main skill's granted effect id.
    pub skill_id: String,
    /// The hit-damage components, split by damage type.
    pub hit_damage: Vec<MainSkillHitPart>,
    /// Hit DPS (`TotalDPS`).
    pub hit_dps: f64,
    /// The total of every damage-over-time source (`TotalDotDPS`).
    pub dot_dps: f64,
    /// Combined DPS (`CombinedDPS`).
    pub combined_dps: f64,
}

/// The main skill of `build` and the hit damage `output` gives it by type.
/// `None` when the build has no main skill the calculation can resolve.
pub fn main_skill_breakdown(
    build: &Build,
    data: &BuildData,
    output: &OutputTable,
) -> Option<MainSkillBreakdown> {
    let (group_index, active_skill_index, skill_id) =
        crate::resolve_main_skill_selection(build, data)?;
    let hit_damage = output
        .damage_components
        .iter()
        .filter(|c| c.kind == pobr_data::prelude::DamageKind::Hit)
        .map(|c| MainSkillHitPart {
            damage_type: format!("{:?}", c.damage_type),
            min: c.min,
            max: c.max,
            avg: (c.min + c.max) / 2.0,
        })
        .collect();
    Some(MainSkillBreakdown {
        group_index,
        active_skill_index,
        skill_id,
        hit_damage,
        hit_dps: output.dps,
        dot_dps: output.total_dot_dps,
        combined_dps: output.combined_dps,
    })
}
