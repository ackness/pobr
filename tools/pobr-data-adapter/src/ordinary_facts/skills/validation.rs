//! Strict physical-input checks before using the deliberately tolerant raw converters.

use std::collections::BTreeSet;

use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::native_facts::reader::{Tables, VerifiedFacts};
use crate::skills::SkillTables;

use super::super::convert::column;

pub(super) const FIELDS: [(&str, &str); 15] = [
    ("BaseItemTypes", "Id Name"),
    (
        "SkillGems",
        "BaseItemType GemType GemColour MinLevelReq StrengthRequirementPercent DexterityRequirementPercent IntelligenceRequirementPercent GemEffects ItemExperienceType",
    ),
    (
        "GemEffects",
        "Id GrantedEffect AdditionalGrantedEffects GemTags",
    ),
    ("GemTags", "Id"),
    ("ItemExperienceTypes", "Id"),
    (
        "ItemExperiencePerLevel",
        "ItemExperienceType ItemCurrentLevel Level",
    ),
    (
        "GrantedEffects",
        "Id IsSupport ActiveSkill CastTime AllowedActiveSkillTypes AddedActiveSkillTypes ExcludedActiveSkillTypes CannotBeSupported SupportsGemsOnly IgnoreMinionTypes StatSet AdditionalStatSets CostTypes",
    ),
    ("ActiveSkills", "Id DisplayedName ActiveSkillTypes"),
    ("ActiveSkillType", "Id"),
    (
        "GrantedEffectsPerLevel",
        "GrantedEffect Level Cooldown AttackTime CostAmounts AttackSpeedMultiplier CostMultiplier Reservation EffectOnPlayer StoredUses",
    ),
    (
        "GrantedEffectStatSets",
        "Id Label BaseEffectiveness IncrementalEffectiveness DamageIncrementalEffectiveness ImplicitStats ConstantStats ConstantStatsValues IgnoredStats",
    ),
    (
        "GrantedEffectStatSetsPerLevel",
        "StatSet GemLevel FloatStats BaseResolvedValues FloatStatsValues AdditionalStats AdditionalStatsValues AdditionalFlags BaseMultiplier SpellCritChance AttackCritChance",
    ),
    ("GrantedEffectLabels", "Text"),
    ("Stats", "Id"),
    ("CostTypes", "Id Divisor PerMinute"),
];

pub(super) struct PhysicalSkills<'a> {
    pub tables: Tables<'a>,
    pub optional_level_multiplier: bool,
}

impl<'a> PhysicalSkills<'a> {
    pub fn checked(input: &'a VerifiedFacts) -> Result<Self, String> {
        let tables = Tables::selected(input, &super::TABLES)?;
        for (table, fields) in FIELDS {
            for field in fields.split_whitespace() {
                let (kind, array, target) = schema(field);
                let references = target.map(|table| json!({"table":table}));
                column(
                    input,
                    table,
                    &[field],
                    &json!({
                        "name":field, "type":kind, "array":array,
                        "interval":false, "references":references
                    }),
                )?;
                for row in 0..tables.len(table) {
                    let value = &tables.row(table, row)?[field];
                    let checked = if array {
                        list(value).and_then(|values| {
                            values
                                .iter()
                                .try_for_each(|v| scalar(&tables, field, kind, target, v))
                        })
                    } else {
                        scalar(&tables, field, kind, target, value)
                    };
                    checked.map_err(|error| format!("{table}/{row}.{field}: {error}"))?;
                }
            }
        }
        // Legacy GPL snapshots may carry this optional fallback. F does not.
        // Check it when declared rather than requiring an invented column or
        // allowing an unvalidated value to reach the shared serde row.
        let doc = &input.documents["tables/GrantedEffectsPerLevel.json"];
        let optional = doc["columns"]
            .as_array()
            .ok_or("missing columns")?
            .iter()
            .find(|c| c["physical_fields"] == json!(["BaseMultiplier"]));
        if list(&doc["physical_fields"])?.contains(&json!("BaseMultiplier")) && optional.is_none() {
            return Err("GrantedEffectsPerLevel: invalid BaseMultiplier column mapping".into());
        }
        if let Some(c) = optional {
            let kind = c["schema"]["type"]
                .as_str()
                .ok_or("invalid BaseMultiplier type")?;
            if !matches!(kind, "i32" | "f32") {
                return Err("GrantedEffectsPerLevel.BaseMultiplier must be numeric".into());
            }
            column(
                input,
                "GrantedEffectsPerLevel",
                &["BaseMultiplier"],
                &json!({
                    "name":"BaseMultiplier", "type":kind, "array":false,
                    "interval":false, "references":null
                }),
            )?;
            for row in 0..tables.len("GrantedEffectsPerLevel") {
                let value = &tables.row("GrantedEffectsPerLevel", row)?["BaseMultiplier"];
                if !value.is_null() {
                    scalar(&tables, "BaseMultiplier", kind, None, value)
                        .map_err(|e| format!("GrantedEffectsPerLevel/{row}.BaseMultiplier: {e}"))?;
                }
            }
        }
        validate_joins(&tables)?;
        Ok(Self {
            tables,
            optional_level_multiplier: optional.is_some(),
        })
    }
}

impl SkillTables for PhysicalSkills<'_> {
    fn read_table<T: DeserializeOwned>(&self, table: &str) -> Result<Vec<T>, String> {
        let fields = FIELDS
            .iter()
            .find(|(name, _)| *name == table)
            .ok_or_else(|| format!("unselected skills table: {table}"))?
            .1;
        (0..self.tables.len(table))
            .map(|index| {
                let raw = self.tables.row(table, index)?;
                let mut row = serde_json::Map::new();
                for field in fields.split_whitespace() {
                    row.insert(field.into(), raw[field].clone());
                }
                if table == "GrantedEffectsPerLevel" && self.optional_level_multiplier {
                    row.insert("BaseMultiplier".into(), raw["BaseMultiplier"].clone());
                }
                // Tables::selected has already proved _row == index. Never copy a
                // values._index over this identity, or use the stable Id as an index.
                row.insert("_index".into(), json!(index));
                serde_json::from_value(Value::Object(row))
                    .map_err(|error| format!("{table}/{index}: typed skill row: {error}"))
            })
            .collect()
    }
}

fn schema(field: &str) -> (&'static str, bool, Option<&'static str>) {
    match field {
        "Id" | "Name" | "DisplayedName" | "Text" => ("string", false, None),
        "IsSupport" | "CannotBeSupported" | "SupportsGemsOnly" | "IgnoreMinionTypes"
        | "PerMinute" => ("bool", false, None),
        "BaseItemType" => ("foreignrow", false, Some("BaseItemTypes")),
        "GemEffects" => ("foreignrow", true, Some("GemEffects")),
        "ItemExperienceType" => ("foreignrow", false, Some("ItemExperienceTypes")),
        "GrantedEffect" => ("foreignrow", false, Some("GrantedEffects")),
        "AdditionalGrantedEffects" => ("foreignrow", true, Some("GrantedEffects")),
        "GemTags" => ("foreignrow", true, Some("GemTags")),
        "ActiveSkill" => ("foreignrow", false, Some("ActiveSkills")),
        "AllowedActiveSkillTypes"
        | "AddedActiveSkillTypes"
        | "ExcludedActiveSkillTypes"
        | "ActiveSkillTypes" => ("foreignrow", true, Some("ActiveSkillType")),
        "StatSet" => ("foreignrow", false, Some("GrantedEffectStatSets")),
        "AdditionalStatSets" => ("foreignrow", true, Some("GrantedEffectStatSets")),
        "CostTypes" => ("foreignrow", true, Some("CostTypes")),
        "Label" => ("foreignrow", false, Some("GrantedEffectLabels")),
        "ImplicitStats" | "ConstantStats" | "IgnoredStats" | "FloatStats" | "AdditionalStats"
        | "AdditionalFlags" => ("foreignrow", true, Some("Stats")),
        "BaseEffectiveness" | "IncrementalEffectiveness" | "DamageIncrementalEffectiveness" => {
            ("f32", false, None)
        }
        "FloatStatsValues" => ("f32", true, None),
        "CostAmounts" | "ConstantStatsValues" | "BaseResolvedValues" | "AdditionalStatsValues" => {
            ("i32", true, None)
        }
        _ => ("i32", false, None),
    }
}

fn scalar(
    tables: &Tables<'_>,
    field: &str,
    kind: &str,
    target: Option<&str>,
    value: &Value,
) -> Result<(), String> {
    if value.is_null() && matches!(field, "ActiveSkill" | "Label" | "DisplayedName") {
        return Ok(());
    }
    match kind {
        "foreignrow" => {
            let row = index(value)?;
            // All consumed numeric FK DTOs are at most u32.
            u32::try_from(row).map_err(|_| "reference exceeds u32")?;
            tables.row(target.ok_or("missing foreign key target")?, row)?;
        }
        "string" if value.is_string() => (),
        "bool" if value.is_boolean() => (),
        "i32" => {
            let number = value
                .as_i64()
                .and_then(|v| i32::try_from(v).ok())
                .ok_or("expected signed i32")?;
            if matches!(
                field,
                "GemType"
                    | "GemColour"
                    | "MinLevelReq"
                    | "StrengthRequirementPercent"
                    | "DexterityRequirementPercent"
                    | "IntelligenceRequirementPercent"
                    | "ItemCurrentLevel"
                    | "Level"
                    | "GemLevel"
                    | "CastTime"
                    | "Cooldown"
                    | "AttackTime"
                    | "CostAmounts"
                    | "StoredUses"
            ) && number < 0
            {
                return Err("negative value for unsigned ordinary field".into());
            }
        }
        "f32"
            if value
                .as_f64()
                .is_some_and(|v| v.is_finite() && v.abs() <= f64::from(f32::MAX)) => {}
        _ => return Err(format!("expected {kind}, got {value}")),
    }
    Ok(())
}

pub(super) fn list(value: &Value) -> Result<&[Value], String> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| "expected array".into())
}

pub(super) fn index(value: &Value) -> Result<usize, String> {
    value
        .as_u64()
        .and_then(|v| usize::try_from(v).ok())
        .ok_or_else(|| format!("invalid physical reference: {value}"))
}

fn validate_joins(tables: &Tables<'_>) -> Result<(), String> {
    let mut bases = BTreeSet::new();
    for row in 0..tables.len("SkillGems") {
        let raw = tables.row("SkillGems", row)?;
        if !bases.insert(index(&raw["BaseItemType"])?) {
            return Err(format!("SkillGems/{row}: duplicate BaseItemType"));
        }
        if list(&raw["GemEffects"])?.len() != 1 {
            return Err(format!(
                "SkillGems/{row}: expected exactly one GemEffects reference"
            ));
        }
    }
    for (table, key, level) in [
        (
            "ItemExperiencePerLevel",
            "ItemExperienceType",
            "ItemCurrentLevel",
        ),
        ("GrantedEffectsPerLevel", "GrantedEffect", "Level"),
        ("GrantedEffectStatSetsPerLevel", "StatSet", "GemLevel"),
    ] {
        let mut seen = BTreeSet::new();
        for row in 0..tables.len(table) {
            let raw = tables.row(table, row)?;
            let identity = (index(&raw[key])?, index(&raw[level])?);
            if !seen.insert(identity) {
                return Err(format!("{table}/{row}: duplicate ({key}, {level}) key"));
            }
            if table == "GrantedEffectsPerLevel" {
                let effect = tables.row("GrantedEffects", identity.0)?;
                paired(table, row, &raw["CostAmounts"], &effect["CostTypes"])?;
            }
        }
    }
    for (table, left, right) in [
        (
            "GrantedEffectStatSets",
            "ConstantStats",
            "ConstantStatsValues",
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            "FloatStats",
            "BaseResolvedValues",
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            "FloatStats",
            "FloatStatsValues",
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            "AdditionalStats",
            "AdditionalStatsValues",
        ),
    ] {
        for row in 0..tables.len(table) {
            let raw = tables.row(table, row)?;
            paired(table, row, &raw[left], &raw[right])
                .map_err(|e| format!("{left}/{right}: {e}"))?;
        }
    }
    Ok(())
}

fn paired(table: &str, row: usize, left: &Value, right: &Value) -> Result<(), String> {
    if list(left)?.len() != list(right)?.len() {
        return Err(format!("{table}/{row}: paired array length mismatch"));
    }
    Ok(())
}
