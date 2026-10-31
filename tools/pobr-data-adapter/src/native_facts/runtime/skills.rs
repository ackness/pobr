//! Explicit primary branches from physical facts, with exact authored level normalization.

use std::collections::{BTreeMap, BTreeSet};

use pobr_data::catalog::native_runtime::{
    NATIVE_PRIMARY_SPELL_PROJECTION_FORMAT, NativePrimarySpellProjection,
    NativePrimarySpellSelection,
};
use pobr_data::catalog::*;
use serde_json::{Value, json};

use super::super::trust::{VerifiedInput, require};
use super::{
    Files,
    inputs::{find, ids, index, row, rows, text},
    json_bytes, skills_policy,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    LegacySpark,
    AuthoredPrimary,
}

struct Projected {
    base: BaseItemDef,
    gem: SkillGemDef,
    effect: GrantedEffectDef,
    levels: Vec<SkillLevelDef>,
    sets: SkillStatSetDef,
    cost: CostTypeDef,
    selection: NativePrimarySpellSelection,
    evidence: Value,
}

pub(super) fn project(input: &VerifiedInput) -> Result<Files, String> {
    let selected = project_one(
        input,
        "Metadata/Items/Gems/SkillGemSpark",
        Scope::LegacySpark,
    )?;
    catalogs(vec![selected], Scope::LegacySpark)
}

pub(super) fn project_primary(input: &VerifiedInput, gems: &[String]) -> Result<Files, String> {
    require(!gems.is_empty(), "primary gems must be nonempty")?;
    let mut selected = Vec::new();
    let mut gem_ids = BTreeSet::new();
    let mut gem_effect_ids = BTreeSet::new();
    let mut effect_ids = BTreeSet::new();
    let mut set_ids = BTreeSet::new();
    for gem in gems {
        require(
            !gem.is_empty() && gem.trim() == gem && gem_ids.insert(gem),
            "empty/duplicate primary gem ID",
        )?;
        let projection = project_one(input, gem, Scope::AuthoredPrimary)?;
        require(
            gem_effect_ids.insert(projection.selection.gem_effect_id.clone())
                && effect_ids.insert(projection.selection.effect_id.clone())
                && set_ids.insert(projection.selection.stat_set_id.clone()),
            "aliased selected effect/primary stat set",
        )?;
        selected.push(projection);
    }
    selected.sort_by(|a, b| a.selection.gem_id.cmp(&b.selection.gem_id));
    catalogs(selected, Scope::AuthoredPrimary)
}

fn catalogs(selected: Vec<Projected>, scope: Scope) -> Result<Files, String> {
    let projection = if scope == Scope::LegacySpark {
        selected[0].evidence.clone()
    } else {
        serde_json::to_value(NativePrimarySpellProjection {
            format: NATIVE_PRIMARY_SPELL_PROJECTION_FORMAT.into(),
            primary_only: true,
            quality: 0,
            selections: selected.iter().map(|s| s.selection.clone()).collect(),
            retained_evidence: selected
                .iter()
                .map(|s| (s.selection.gem_id.clone(), s.evidence.clone()))
                .collect(),
        })
        .map_err(|e| e.to_string())?
    };
    let mut bases = Vec::new();
    let mut gems = Vec::new();
    let mut effects = Vec::new();
    let mut levels = BTreeMap::new();
    let mut sets = Vec::new();
    let mut costs = Vec::new();
    for s in selected {
        bases.push(s.base);
        gems.push(s.gem);
        effects.push(s.effect);
        levels.insert(s.selection.effect_id, s.levels);
        sets.push(s.sets);
        if costs.is_empty() {
            costs.push(s.cost);
        }
    }
    Ok(BTreeMap::from([
        ("base_items.json".into(), json_bytes(&bases)?),
        ("skill_gems.json".into(), json_bytes(&gems)?),
        ("granted_effects.json".into(), json_bytes(&effects)?),
        ("granted_effect_levels.json".into(), json_bytes(&levels)?),
        ("granted_effect_stat_sets.json".into(), json_bytes(&sets)?),
        ("cost_types.json".into(), json_bytes(&costs)?),
        (
            "native_skill_projection.json".into(),
            json_bytes(&projection)?,
        ),
    ]))
}

fn project_one(input: &VerifiedInput, gem_id: &str, scope: Scope) -> Result<Projected, String> {
    let (base_index, base) = if scope == Scope::LegacySpark {
        find(input, "BaseItemTypes", gem_id)?
    } else {
        unique(input, "BaseItemTypes", |v| v["Id"] == gem_id)?
    };
    let (gem_index, gem) = unique(input, "SkillGems", |v| v["BaseItemType"] == base_index)?;
    let effects = gem["GemEffects"].as_array().ok_or("gem effects missing")?;
    require(effects.len() == 1, "selected gem has ambiguous GemEffects")?;
    let gem_effect_index = index(&effects[0])?;
    let gem_effect = row(input, "GemEffects", gem_effect_index)?;
    require(
        gem["GemType"] == 0 && gem_effect["AdditionalGrantedEffects"] == json!([]),
        "selected active gem has unsupported type or additional effects",
    )?;
    let effect_index = index(&gem_effect["GrantedEffect"])?;
    let effect = row(input, "GrantedEffects", effect_index)?;
    let effect_id = text(&effect["Id"])?;
    require(
        (scope != Scope::LegacySpark || effect_id == "SparkPlayer") && effect["IsSupport"] == false,
        if scope == Scope::LegacySpark {
            "selected gem is not primary Spark"
        } else {
            "selected gem is not a primary active spell"
        },
    )?;
    let active_index = index(&effect["ActiveSkill"])?;
    let active = row(input, "ActiveSkills", active_index)?;
    require(
        active["GrantedEffect"] == effect_id
            || (scope == Scope::AuthoredPrimary && active["GrantedEffect"] == ""),
        "active skill string points to another effect",
    )?;
    let set_index = index(&effect["StatSet"])?;
    let set = row(input, "GrantedEffectStatSets", set_index)?;
    require(
        scope != Scope::LegacySpark || set["Id"] == "SparkPlayer",
        "selected primary branch changed",
    )?;
    require(
        set["IgnoredStats"] == json!([]),
        "primary ignored stats require an explicit policy",
    )?;
    if scope == Scope::AuthoredPrimary {
        for (table, index) in [
            ("BaseItemTypes", base_index),
            ("SkillGems", gem_index),
            ("GemEffects", gem_effect_index),
            ("GrantedEffects", effect_index),
            ("ActiveSkills", active_index),
            ("GrantedEffectStatSets", set_index),
            ("CostTypes", 0),
        ] {
            // SkillGems has no Id; its unique BaseItemType join above is its identity.
            if table != "SkillGems" {
                stable_row(input, table, index)?;
            }
        }
        skills_policy::static_fields(input, gem, gem_effect, effect, active, set)?;
    }
    let constant_stats = paired(input, &set["ConstantStats"], &set["ConstantStatsValues"])?;
    let implicit_stats = ids(input, "Stats", &set["ImplicitStats"])?;
    let paired_levels = if scope == Scope::LegacySpark {
        let (level_index, level) = unique(input, "GrantedEffectsPerLevel", |v| {
            v["GrantedEffect"] == effect_index && v["Level"] == 1
        })?;
        let (set_level_index, set_level) = unique(input, "GrantedEffectStatSetsPerLevel", |v| {
            v["StatSet"] == set_index && v["GemLevel"] == 1
        })?;
        vec![(1, level_index, level, set_level_index, set_level)]
    } else {
        authored_levels(input, effect_index, set_index)?
    };
    let mut levels = Vec::new();
    let mut set_levels = Vec::new();
    let mut level_evidence = Vec::new();
    for (gem_level, level_index, level, set_level_index, set_level) in &paired_levels {
        require(
            set_level["GrantedEffects"]
                .as_array()
                .is_some_and(|values| values.contains(&json!(effect_index))),
            "primary stat-set level does not declare selected effect",
        )?;
        require(
            if scope == Scope::LegacySpark {
                set_level["ActorLevel"] == level["ActorLevel"]
                    && set_level["ActorLevel"].as_f64() == Some(1.0)
            } else {
                number(&set_level["ActorLevel"])? == number(&level["ActorLevel"])?
            },
            if scope == Scope::LegacySpark {
                "selected actor level is outside fixed profile"
            } else {
                "paired actor scaling coordinates differ"
            },
        )?;
        require(
            set_level["AdditionalFlags"] == json!([]) && set_level["AttackCritChance"] == 0,
            "selected spell level has unsupported flags or attack crit",
        )?;
        let mut resolved = paired(
            input,
            &set_level["FloatStats"],
            &set_level["BaseResolvedValues"],
        )?;
        let coefficients = set_level["FloatStatsValues"]
            .as_array()
            .ok_or("float coefficients missing")?;
        require(
            coefficients.len() == resolved.len(),
            "float coefficients/resolved values length mismatch",
        )?;
        resolved.extend(paired(
            input,
            &set_level["AdditionalStats"],
            &set_level["AdditionalStatsValues"],
        )?);
        require(
            set_level["StatInterpolations"]
                .as_array()
                .is_some_and(|values| values.len() == resolved.len()),
            "stat interpolation evidence length mismatch",
        )?;
        // This bounded profile has no source-backed duplicate/override policy.
        // Never publish ambiguous amounts that downstream aggregation would sum.
        let mut seen = BTreeSet::new();
        for id in resolved
            .iter()
            .chain(&constant_stats)
            .map(|stat| stat.stat.as_str())
            .chain(implicit_stats.iter().map(String::as_str))
        {
            require(
                seen.insert(id),
                &format!("duplicate selected stat ID: {id}"),
            )?;
        }
        let cost_slots = effect["CostTypes"].as_array().ok_or("missing cost types")?;
        let amounts = level["CostAmounts"].as_array().ok_or("missing costs")?;
        require(
            cost_slots == &[json!(0)] && cost_slots.len() == amounts.len(),
            "selected cost types outside fixed Mana slot",
        )?;
        let cost = row(input, "CostTypes", 0)?;
        require(
            cost["Id"] == "Mana" && cost["Divisor"] == 1 && cost["PerMinute"] == false,
            "selected instantaneous Mana cost normalization changed",
        )?;
        levels.push(SkillLevelDef {
            level: *gem_level,
            cooldown_ms: nonzero(&level["Cooldown"])?,
            attack_time_ms: nonzero(&level["AttackTime"])?,
            cost_amounts: amounts.iter().map(unsigned).collect::<Result<_, _>>()?,
            attack_speed_multiplier: nonzero_number(&level["AttackSpeedMultiplier"])?,
            base_multiplier: None,
            crit_chance: Some(physical_i32(&set_level["SpellCritChance"])? / 100.0),
            mana_multiplier: nonneutral(&level["CostMultiplier"], 100.0)?,
            spirit_reservation_flat: nonzero_number(&level["Reservation"])?,
            reservation_multiplier: nonneutral(&level["EffectOnPlayer"], 100.0)?,
            stored_uses: nonzero(&level["StoredUses"])?,
            level_requirement: None,
        });
        set_levels.push(SkillStatSetLevel {
            gem_level: *gem_level,
            damage_multiplier: 1.0 + physical_i32(&set_level["BaseMultiplier"])? / 10000.0,
            stats: resolved,
            per_effectiveness: vec![],
        });
        if scope == Scope::AuthoredPrimary {
            skills_policy::level_fields(input, level, set_level, active)?;
            level_evidence.push(json!({"gem_level":gem_level,
            "rows":[reference("GrantedEffectsPerLevel",*level_index),reference("GrantedEffectStatSetsPerLevel",*set_level_index)],
            "actor_scaling_coordinate":number(&level["ActorLevel"])?,
            "retained_outside_projection":{
                "GrantedEffectsPerLevel":retained(level, &["Level","GrantedEffect","ActorLevel","CostAmounts","Cooldown","AttackTime","AttackSpeedMultiplier","CostMultiplier","Reservation","EffectOnPlayer","StoredUses"]),
                "GrantedEffectStatSetsPerLevel":retained(set_level, &["GemLevel","StatSet","GrantedEffects","ActorLevel","FloatStats","BaseResolvedValues","AdditionalStats","AdditionalStatsValues","SpellCritChance","BaseMultiplier","AttackCritChance","AdditionalFlags"])
            }}));
        }
    }
    let label = if set["Label"].is_null() {
        None
    } else {
        Some(text(
            &row(input, "GrantedEffectLabels", index(&set["Label"])?)?["Text"],
        )?)
    };
    let effect_definition = GrantedEffectDef {
        id: effect_id.clone(),
        is_support: false,
        active_skill: Some(text(&active["Id"])?),
        displayed_name: Some(text(&active["DisplayedName"])?),
        cast_time: nonzero(&effect["CastTime"])?,
        require_skill_types: ids(input, "ActiveSkillType", &effect["AllowedActiveSkillTypes"])?,
        add_skill_types: ids(input, "ActiveSkillType", &effect["AddedActiveSkillTypes"])?,
        exclude_skill_types: ids(
            input,
            "ActiveSkillType",
            &effect["ExcludedActiveSkillTypes"],
        )?,
        cannot_be_supported: boolean(&effect["CannotBeSupported"])?,
        support_gems_only: boolean(&effect["SupportsGemsOnly"])?,
        stat_set: Some(u32::try_from(set_index).map_err(|e| e.to_string())?),
        // Extra branches remain in the source index, not silently merged into
        // this deliberately primary-only runtime projection.
        additional_stat_set_ids: vec![],
        cost_types: vec![0],
        skill_types: ids(input, "ActiveSkillType", &active["ActiveSkillTypes"])?,
        minion_list: vec![],
        add_minion_list: vec![],
        minion_uses: vec![],
        minion_has_item_set: false,
        ignore_minion_types: boolean(&effect["IgnoreMinionTypes"])?,
    };
    let sets = SkillStatSetDef {
        effect_id: effect_id.clone(),
        sets: vec![StatSetDef {
            set_id: text(&set["Id"])?,
            label,
            vendor_set_index: None,
            base_effectiveness: number(&set["BaseEffectiveness"])?,
            user_level_scaling: None,
            constant_stats,
            skill_attack_speed_more: None,
            dot_flags: DotFlags::default(),
            explode_corpse: false,
            implicit_stats,
            levels: set_levels,
        }],
    };
    let gem_definition = SkillGemDef {
        id: text(&base["Id"])?,
        gem_type: Some(unsigned(&gem["GemType"])?),
        gem_colour: Some(unsigned(&gem["GemColour"])?),
        min_level_req: unsigned(&gem["MinLevelReq"])?,
        str_pct: unsigned(&gem["StrengthRequirementPercent"])?,
        dex_pct: unsigned(&gem["DexterityRequirementPercent"])?,
        int_pct: unsigned(&gem["IntelligenceRequirementPercent"])?,
        is_support: false,
        gem_effect_id: Some(text(&gem_effect["Id"])?),
        granted_effect_id: Some(effect_id.clone()),
        additional_granted_effect_ids: ids(
            input,
            "GrantedEffects",
            &gem_effect["AdditionalGrantedEffects"],
        )?,
        gem_tags: ids(input, "GemTags", &gem_effect["GemTags"])?,
        level_requirements: vec![],
    };
    let base_definition = BaseItemDef {
        id: text(&base["Id"])?,
        name: text(&base["Name"])?,
        item_class: text(&row(input, "ItemClasses", index(&base["ItemClass"])?)?["Id"])?,
        drop_level: unsigned(&base["DropLevel"])?,
        width: u8::try_from(unsigned(&base["Width"])?).map_err(|e| e.to_string())?,
        height: u8::try_from(unsigned(&base["Height"])?).map_err(|e| e.to_string())?,
        tags: ids(input, "Tags", &base["Tags"])?,
        implicits: ids(input, "Mods", &base["Implicit_Mods"])?,
        mod_domain: unsigned(&base["ModDomain"])?,
        weapon: None,
        armour: None,
        spirit: None,
        charm_buff: vec![],
        utility: None,
        req_str: 0,
        req_dex: 0,
        req_int: 0,
    };
    let cost = row(input, "CostTypes", 0)?;
    let (_, level_index, _, set_level_index, set_level) = paired_levels[0];
    let evidence = json!({"format":"pobr-native-skill-projection/v1","primary_only":true,
        "rows": [reference("BaseItemTypes",base_index),reference("SkillGems",gem_index),reference("GemEffects",gem_effect_index),
            reference("GrantedEffects",effect_index),reference("ActiveSkills",active_index),reference("GrantedEffectStatSets",set_index),
            reference("GrantedEffectsPerLevel",level_index),reference("GrantedEffectStatSetsPerLevel",set_level_index),reference("CostTypes",0)],
        "damage":{"source":"GrantedEffectStatSetsPerLevel.BaseResolvedValues","stat_ids":"FloatStats -> Stats.Id",
            "coefficients_used_as_damage":false,"effectiveness_applied_again":false},
        "retained_outside_projection":{"additional_stat_sets":effect["AdditionalStatSets"],"qualities":"all source quality scopes",
            "interpolation_bases":set_level["InterpolationBases"],"stat_interpolations":set_level["StatInterpolations"],
            "float_coefficients":set_level["FloatStatsValues"]},
        "missing":"General requirements, actor constants, timing/gameplay and exact branch/dataset admission."
    });
    let selection = NativePrimarySpellSelection {
        gem_id: gem_id.into(),
        gem_effect_id: text(&gem_effect["Id"])?,
        effect_id,
        active_skill_id: text(&active["Id"])?,
        stat_set_id: text(&set["Id"])?,
        gem_levels: paired_levels.iter().map(|(level, ..)| *level).collect(),
    };
    let evidence = if scope == Scope::LegacySpark {
        evidence
    } else {
        let additional_stat_sets = ids(
            input,
            "GrantedEffectStatSets",
            &effect["AdditionalStatSets"],
        )?;
        let mut seen = BTreeSet::new();
        for id in &additional_stat_sets {
            require(
                !id.is_empty() && id != &selection.stat_set_id && seen.insert(id),
                "invalid additional stat set identity",
            )?;
            unique(input, "GrantedEffectStatSets", |v| v["Id"] == *id)?;
        }
        json!({
            "rows":[reference("BaseItemTypes",base_index),reference("SkillGems",gem_index),reference("GemEffects",gem_effect_index),
                reference("GrantedEffects",effect_index),reference("ActiveSkills",active_index),reference("GrantedEffectStatSets",set_index),reference("CostTypes",0)],
            "levels":level_evidence, "active_reverse_granted_effect":active["GrantedEffect"],
            "additional_stat_sets":additional_stat_sets,
            "retained_outside_projection":{
                "BaseItemTypes":retained(base, &["Id","Name","ItemClass","DropLevel","Width","Height","Tags","Implicit_Mods","ModDomain"]),
                "SkillGems":retained(gem, &["BaseItemType","GemEffects","GemType","GemColour","MinLevelReq","StrengthRequirementPercent","DexterityRequirementPercent","IntelligenceRequirementPercent"]),
                "GemEffects":retained(gem_effect, &["Id","GrantedEffect","AdditionalGrantedEffects","GemTags"]),
                "GrantedEffects":retained(effect, &["Id","IsSupport","ActiveSkill","StatSet","CastTime","CostTypes","AllowedActiveSkillTypes","AddedActiveSkillTypes","ExcludedActiveSkillTypes","CannotBeSupported","SupportsGemsOnly","IgnoreMinionTypes"]),
                "ActiveSkills":retained(active, &["Id","GrantedEffect","DisplayedName","ActiveSkillTypes"]),
                "GrantedEffectStatSets":retained(set, &["Id","ConstantStats","ConstantStatsValues","ImplicitStats","IgnoredStats","Label"]),
                "CostTypes":retained(cost, &["Id","Divisor","PerMinute"]),
                "quality":"all source quality scopes retained in the pinned archive; quality zero only",
                "damage":"BaseResolvedValues paired with FloatStats is used once; coefficients and effectiveness are not damage inputs"
            }
        })
    };
    Ok(Projected {
        base: base_definition,
        gem: gem_definition,
        effect: effect_definition,
        levels,
        sets,
        cost: CostTypeDef {
            id: text(&cost["Id"])?,
            divisor: 1,
            per_minute: false,
        },
        selection,
        evidence,
    })
}

fn paired(
    input: &VerifiedInput,
    stat_rows: &Value,
    values: &Value,
) -> Result<Vec<SkillDamageStat>, String> {
    let stats = ids(input, "Stats", stat_rows)?;
    let values = values.as_array().ok_or("stat values must be an array")?;
    require(stats.len() == values.len(), "stat/value length mismatch")?;
    stats
        .into_iter()
        .zip(values)
        .map(|(stat, value)| {
            Ok(SkillDamageStat {
                stat,
                // These three physical channels are signed i32 arrays. Reject
                // a re-sealed wider/fractional value instead of losing precision.
                value: f64::from(
                    value
                        .as_i64()
                        .and_then(|n| i32::try_from(n).ok())
                        .ok_or("resolved stat is not a signed i32")?,
                ),
            })
        })
        .collect()
}

fn unique<'a>(
    input: &'a VerifiedInput,
    table: &str,
    select: impl Fn(&Value) -> bool,
) -> Result<(usize, &'a Value), String> {
    let selected = rows(input, table)?
        .iter()
        .enumerate()
        .filter(|(_, row)| select(&row["values"]))
        .map(|(i, row)| (i, &row["values"]))
        .collect::<Vec<_>>();
    require(
        selected.len() == 1,
        &format!("expected exactly one selected {table} row"),
    )?;
    Ok(selected[0])
}

fn number(value: &Value) -> Result<f64, String> {
    value
        .as_f64()
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("expected finite number, got {value}"))
}
fn unsigned(value: &Value) -> Result<u32, String> {
    value
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| format!("expected u32, got {value}"))
}
fn nonzero(value: &Value) -> Result<Option<u32>, String> {
    let n = unsigned(value)?;
    Ok((n != 0).then_some(n))
}
fn boolean(value: &Value) -> Result<bool, String> {
    value.as_bool().ok_or("expected boolean".into())
}
fn nonzero_number(value: &Value) -> Result<Option<f64>, String> {
    let n = physical_i32(value)?;
    Ok((n != 0.0).then_some(n))
}
fn nonneutral(value: &Value, neutral: f64) -> Result<Option<f64>, String> {
    let n = physical_i32(value)?;
    Ok((n != neutral).then_some(n - neutral))
}
fn physical_i32(value: &Value) -> Result<f64, String> {
    value
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
        .map(f64::from)
        .ok_or_else(|| format!("expected physical signed i32, got {value}"))
}
fn reference(table: &str, index: usize) -> Value {
    json!({"payload":format!("tables/{table}.json"),"pointer":format!("/rows/{index}/values")})
}

fn retained(value: &Value, consumed: &[&str]) -> Value {
    Value::Object(
        value
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(key, _)| !consumed.contains(&key.as_str()))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    )
}

fn stable_row(input: &VerifiedInput, table: &str, physical: usize) -> Result<(), String> {
    let id = text(&row(input, table, physical)?["Id"])?;
    require(!id.is_empty(), "empty selected stable ID")?;
    let (found, _) = unique(input, table, |v| v["Id"] == id)?;
    require(
        found == physical,
        "selected stable/physical identity mismatch",
    )
}

type LevelPair<'a> = (u32, usize, &'a Value, usize, &'a Value);

fn authored_levels(
    input: &VerifiedInput,
    effect: usize,
    set: usize,
) -> Result<Vec<LevelPair<'_>>, String> {
    fn collect<'a>(
        input: &'a VerifiedInput,
        table: &str,
        relation: &str,
        index: usize,
        level_key: &str,
    ) -> Result<BTreeMap<u32, (usize, &'a Value)>, String> {
        let mut selected = BTreeMap::new();
        for (physical, entry) in rows(input, table)?.iter().enumerate() {
            let value = &entry["values"];
            if value[relation] != index {
                continue;
            }
            let level = unsigned(&value[level_key])?;
            require(
                level > 0
                    && i32::try_from(level).is_ok()
                    && selected.insert(level, (physical, value)).is_none(),
                "invalid/duplicate authored gem level",
            )?;
        }
        require(!selected.is_empty(), "missing authored gem levels")?;
        Ok(selected)
    }
    let effects = collect(
        input,
        "GrantedEffectsPerLevel",
        "GrantedEffect",
        effect,
        "Level",
    )?;
    let sets = collect(
        input,
        "GrantedEffectStatSetsPerLevel",
        "StatSet",
        set,
        "GemLevel",
    )?;
    require(
        effects.keys().eq(sets.keys()),
        "missing paired authored level",
    )?;
    Ok(effects
        .into_iter()
        .map(|(level, (ei, ev))| {
            let (si, sv) = sets[&level];
            (level, ei, ev, si, sv)
        })
        .collect())
}
