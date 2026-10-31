//! Bounded A0 physical projections; raw channels carry no application policy.

use serde::{Deserialize, Serialize};
use serde_json::{Number, Value};

use super::convert::{Tables, array, decode};
use super::model::{Identity, InputRef};
use super::trust::{VerifiedInput, exact_keys, require};

pub(super) const TABLES: [&str; 7] = [
    "GrantedEffectLabels",
    "ActiveSkills",
    "ActiveSkillType",
    "CostTypes",
    "GrantedEffectsPerLevel",
    "GrantedEffectStatSetsPerLevel",
    "EffectivenessCostConstants",
];

/// Stable IDs are scoped by the sealed input identity, with physical evidence attached.
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct StableRef {
    pub id: String,
    pub raw: InputRef,
}

// One typed physical projection supports ephemeral producer identities and PoBR
// stable references. Vectors retain physical order, nullable slots and duplicates.
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct CoreSkills<R> {
    pub labels: Vec<Label<R>>,
    pub stat_set_labels: Vec<StatSetLabel<R>>,
    pub active_skill_types: Vec<ActiveSkillType<R>>,
    pub active_skills: Vec<ActiveSkill<R>>,
    pub effect_metadata: Vec<EffectMetadata<R>>,
    pub cost_types: Vec<CostType<R>>,
    pub interpolation_constants: Vec<InterpolationConstant<R>>,
    pub effect_levels: Vec<EffectLevel<R>>,
    pub stat_set_levels: Vec<StatSetLevel<R>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Label<R> {
    pub label: R,
    pub text: String,
    pub raw: InputRef,
}
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct StatSetLabel<R> {
    pub stat_set: R,
    pub label: Option<R>,
    pub raw: InputRef,
}
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct ActiveSkillType<R> {
    pub skill_type: R,
    pub flag_stat: Option<R>,
    pub raw: InputRef,
}
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct ActiveSkill<R> {
    pub active_skill: R,
    pub granted_effect_string_raw: String,
    pub raw: InputRef,
}
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct EffectMetadata<R> {
    pub effect: R,
    pub active_skill: Option<R>,
    pub cost_types: Vec<Option<R>>,
    pub raw: InputRef,
}
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct CostType<R> {
    pub cost_type: R,
    pub stat: Option<R>,
    pub divisor_raw: Number,
    pub per_minute_raw: bool,
    pub format_text: String,
    pub raw: InputRef,
}
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct InterpolationConstant<R> {
    pub constant: R,
    pub multiplier_raw: Number,
    pub raw: InputRef,
}
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Cost<R> {
    pub slot: usize,
    pub cost_type: Option<R>,
    pub amount_raw: Number,
}
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct EffectLevel<R> {
    pub row: usize,
    pub effect: R,
    pub level_raw: Number,
    pub actor_level_raw: Number,
    pub cost_amounts_raw: Vec<Number>,
    // None means unresolved alignment, not zero cost or an empty array.
    pub costs: Option<Vec<Cost<R>>>,
    pub raw: InputRef,
}
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct StatSetLevel<R> {
    pub row: usize,
    pub stat_set: R,
    pub gem_level_raw: Number,
    pub actor_level_raw: Number,
    pub spell_crit_chance_raw: Number,
    pub attack_crit_chance_raw: Number,
    pub base_multiplier_raw: Number,
    pub base_resolved_values_raw: Vec<Number>,
    pub additional_stats_values_raw: Vec<Number>,
    pub float_stats_values_raw: Vec<Number>,
    pub additional_stats: Vec<Option<R>>,
    pub float_stats: Vec<Option<R>>,
    pub interpolation_bases: Vec<Option<R>>,
    pub stat_interpolations_raw: Vec<Number>,
    pub raw: InputRef,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CoreSummary {
    pub labels: usize,
    pub active_skills: usize,
    pub active_skill_types: usize,
    pub cost_types: usize,
    pub effect_level_rows: usize,
    pub stat_set_level_rows: usize,
    pub interpolation_constants: usize,
}

pub(super) fn normalize(
    input: &VerifiedInput,
    tables: &Tables<'_>,
) -> Result<(CoreSkills<StableRef>, CoreSummary), String> {
    // Recreate only the declared physical view, never execute producer code or
    // interpolate values. Check every projected field, row order and raw pointer.
    let expected = project(tables, |_, id| id)?;
    let source = &input.documents["skill_core.json"];
    let names = [
        "labels",
        "stat_set_labels",
        "active_skill_types",
        "active_skills",
        "effect_metadata",
        "cost_types",
        "interpolation_constants",
        "effect_levels",
        "stat_set_levels",
    ];
    exact_keys(source, &names.map(String::from))?;
    check_collection(source, "labels", &expected.labels)?;
    check_collection(source, "stat_set_labels", &expected.stat_set_labels)?;
    check_collection(source, "active_skill_types", &expected.active_skill_types)?;
    check_collection(source, "active_skills", &expected.active_skills)?;
    check_collection(source, "effect_metadata", &expected.effect_metadata)?;
    check_collection(source, "cost_types", &expected.cost_types)?;
    check_collection(
        source,
        "interpolation_constants",
        &expected.interpolation_constants,
    )?;
    check_collection(source, "effect_levels", &expected.effect_levels)?;
    check_collection(source, "stat_set_levels", &expected.stat_set_levels)?;
    let summary = CoreSummary {
        labels: expected.labels.len(),
        active_skills: expected.active_skills.len(),
        active_skill_types: expected.active_skill_types.len(),
        cost_types: expected.cost_types.len(),
        effect_level_rows: expected.effect_levels.len(),
        stat_set_level_rows: expected.stat_set_levels.len(),
        interpolation_constants: expected.interpolation_constants.len(),
    };
    let candidate = project(tables, |table, id| StableRef {
        id: id.id,
        raw: InputRef::row(table, id.row),
    })?;
    Ok((candidate, summary))
}

fn check_collection(source: &Value, name: &str, expected: &[impl Serialize]) -> Result<(), String> {
    let rows = array(&source[name])?;
    require(
        rows.len() == expected.len(),
        &format!("skill-core physical projection mismatch: {name}"),
    )?;
    for (row, expected) in rows.iter().zip(expected) {
        let value = serde_json::to_value(expected).map_err(|e| e.to_string())?;
        require(
            same_facts(&value, row),
            &format!("skill-core physical projection mismatch: {name}"),
        )?;
    }
    Ok(())
}

// JSON number equality alone equates negative and positive zero. Raw binary64
// channels must retain that bit as well as every other bit of the parsed value.
fn same_facts(expected: &Value, actual: &Value) -> bool {
    match (expected, actual) {
        (Value::Number(a), Value::Number(b)) if a.is_f64() || b.is_f64() => {
            a.is_f64() == b.is_f64() && a.as_f64().map(f64::to_bits) == b.as_f64().map(f64::to_bits)
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_facts(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, a)| b.get(key).is_some_and(|b| same_facts(a, b)))
        }
        _ => expected == actual,
    }
}

pub(super) fn required_fields(table: &str) -> &'static [&'static str] {
    match table {
        "GrantedEffectLabels" => &["Id", "Text"],
        "GrantedEffectStatSets" => &["Id", "Label"],
        "ActiveSkillType" => &["Id", "FlagStat"],
        "ActiveSkills" => &["Id", "GrantedEffect"],
        "GrantedEffects" => &["Id", "ActiveSkill", "CostTypes"],
        "CostTypes" => &["Id", "Stat", "Divisor", "PerMinute", "FormatText"],
        "EffectivenessCostConstants" => &["Id", "Multiplier"],
        "GrantedEffectsPerLevel" => &["GrantedEffect", "Level", "ActorLevel", "CostAmounts"],
        "GrantedEffectStatSetsPerLevel" => &[
            "StatSet",
            "GemLevel",
            "ActorLevel",
            "SpellCritChance",
            "AttackCritChance",
            "BaseMultiplier",
            "BaseResolvedValues",
            "AdditionalStatsValues",
            "FloatStatsValues",
            "AdditionalStats",
            "FloatStats",
            "InterpolationBases",
            "StatInterpolations",
        ],
        _ => &[],
    }
}

fn project<R>(
    tables: &Tables<'_>,
    reference: impl Fn(&str, Identity) -> R,
) -> Result<CoreSkills<R>, String> {
    let identity = |table: &str, row: usize| -> Result<R, String> {
        let id = decode(&tables.row(table, row)?["Id"])?;
        Ok(reference(table, Identity { row, id }))
    };
    let foreign = |table: &str, value: &Value| -> Result<R, String> {
        let row = value
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .ok_or("invalid physical foreign row")?;
        identity(table, row)
    };
    let optional = |table: &str, value: &Value| -> Result<Option<R>, String> {
        if value.is_null() {
            Ok(None)
        } else {
            foreign(table, value).map(Some)
        }
    };
    let refs = |table: &str, value: &Value| -> Result<Vec<Option<R>>, String> {
        array(value)?.iter().map(|v| optional(table, v)).collect()
    };
    let mut core = CoreSkills {
        labels: Vec::new(),
        stat_set_labels: Vec::new(),
        active_skill_types: Vec::new(),
        active_skills: Vec::new(),
        effect_metadata: Vec::new(),
        cost_types: Vec::new(),
        interpolation_constants: Vec::new(),
        effect_levels: Vec::new(),
        stat_set_levels: Vec::new(),
    };
    for row in 0..tables.len("GrantedEffectLabels") {
        let raw = tables.row("GrantedEffectLabels", row)?;
        core.labels.push(Label {
            label: identity("GrantedEffectLabels", row)?,
            text: decode(&raw["Text"])?,
            raw: InputRef::row("GrantedEffectLabels", row),
        });
    }
    for row in 0..tables.len("GrantedEffectStatSets") {
        let raw = tables.row("GrantedEffectStatSets", row)?;
        core.stat_set_labels.push(StatSetLabel {
            stat_set: identity("GrantedEffectStatSets", row)?,
            label: optional("GrantedEffectLabels", &raw["Label"])?,
            raw: InputRef::row("GrantedEffectStatSets", row),
        });
    }
    for row in 0..tables.len("ActiveSkillType") {
        let raw = tables.row("ActiveSkillType", row)?;
        core.active_skill_types.push(ActiveSkillType {
            skill_type: identity("ActiveSkillType", row)?,
            flag_stat: optional("Stats", &raw["FlagStat"])?,
            raw: InputRef::row("ActiveSkillType", row),
        });
    }
    for row in 0..tables.len("ActiveSkills") {
        let raw = tables.row("ActiveSkills", row)?;
        core.active_skills.push(ActiveSkill {
            active_skill: identity("ActiveSkills", row)?,
            granted_effect_string_raw: decode(&raw["GrantedEffect"])?,
            raw: InputRef::row("ActiveSkills", row),
        });
    }
    for row in 0..tables.len("GrantedEffects") {
        let raw = tables.row("GrantedEffects", row)?;
        core.effect_metadata.push(EffectMetadata {
            effect: identity("GrantedEffects", row)?,
            active_skill: optional("ActiveSkills", &raw["ActiveSkill"])?,
            cost_types: refs("CostTypes", &raw["CostTypes"])?,
            raw: InputRef::row("GrantedEffects", row),
        });
    }
    for row in 0..tables.len("CostTypes") {
        let raw = tables.row("CostTypes", row)?;
        core.cost_types.push(CostType {
            cost_type: identity("CostTypes", row)?,
            stat: optional("Stats", &raw["Stat"])?,
            divisor_raw: decode(&raw["Divisor"])?,
            per_minute_raw: decode(&raw["PerMinute"])?,
            format_text: decode(&raw["FormatText"])?,
            raw: InputRef::row("CostTypes", row),
        });
    }
    for row in 0..tables.len("EffectivenessCostConstants") {
        let raw = tables.row("EffectivenessCostConstants", row)?;
        core.interpolation_constants.push(InterpolationConstant {
            constant: identity("EffectivenessCostConstants", row)?,
            multiplier_raw: decode(&raw["Multiplier"])?,
            raw: InputRef::row("EffectivenessCostConstants", row),
        });
    }
    for row in 0..tables.len("GrantedEffectsPerLevel") {
        let raw = tables.row("GrantedEffectsPerLevel", row)?;
        let effect_row = raw["GrantedEffect"]
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .ok_or("invalid effect level owner")?;
        let types = array(&tables.row("GrantedEffects", effect_row)?["CostTypes"])?;
        let amounts: Vec<Number> = decode(&raw["CostAmounts"])?;
        let costs = if types.len() == amounts.len() {
            Some(
                types
                    .iter()
                    .zip(&amounts)
                    .enumerate()
                    .map(|(slot, (kind, amount))| {
                        Ok(Cost {
                            slot,
                            cost_type: optional("CostTypes", kind)?,
                            amount_raw: amount.clone(),
                        })
                    })
                    .collect::<Result<_, String>>()?,
            )
        } else {
            None
        };
        core.effect_levels.push(EffectLevel {
            row,
            effect: identity("GrantedEffects", effect_row)?,
            level_raw: decode(&raw["Level"])?,
            actor_level_raw: decode(&raw["ActorLevel"])?,
            cost_amounts_raw: amounts,
            costs,
            raw: InputRef::row("GrantedEffectsPerLevel", row),
        });
    }
    for row in 0..tables.len("GrantedEffectStatSetsPerLevel") {
        let raw = tables.row("GrantedEffectStatSetsPerLevel", row)?;
        core.stat_set_levels.push(StatSetLevel {
            row,
            stat_set: foreign("GrantedEffectStatSets", &raw["StatSet"])?,
            gem_level_raw: decode(&raw["GemLevel"])?,
            actor_level_raw: decode(&raw["ActorLevel"])?,
            spell_crit_chance_raw: decode(&raw["SpellCritChance"])?,
            attack_crit_chance_raw: decode(&raw["AttackCritChance"])?,
            base_multiplier_raw: decode(&raw["BaseMultiplier"])?,
            base_resolved_values_raw: decode(&raw["BaseResolvedValues"])?,
            additional_stats_values_raw: decode(&raw["AdditionalStatsValues"])?,
            float_stats_values_raw: decode(&raw["FloatStatsValues"])?,
            additional_stats: refs("Stats", &raw["AdditionalStats"])?,
            float_stats: refs("Stats", &raw["FloatStats"])?,
            interpolation_bases: refs("EffectivenessCostConstants", &raw["InterpolationBases"])?,
            stat_interpolations_raw: decode(&raw["StatInterpolations"])?,
            raw: InputRef::row("GrantedEffectStatSetsPerLevel", row),
        });
    }
    Ok(core)
}
