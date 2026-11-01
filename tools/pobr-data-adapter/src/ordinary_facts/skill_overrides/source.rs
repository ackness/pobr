//! Checked primary-set levels and identity-only effect-level pairing.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::super::skill_stat_map::policy::identity;
use crate::native_facts::reader::{Tables, VerifiedFacts};

pub(super) struct Level {
    pub source_row: usize,
    pub raw_multiplier: i32,
}

pub(super) struct Facts {
    pub stats: BTreeMap<String, usize>,
    pub effects: BTreeMap<String, usize>,
    pub sets: BTreeMap<String, usize>,
    pub set_levels: BTreeMap<usize, BTreeMap<i32, Level>>,
    pub effect_levels: BTreeMap<usize, BTreeMap<i32, usize>>,
}

const FIELDS: [(&str, &[&str]); 5] = [
    ("Stats", &["Id"]),
    ("GrantedEffects", &["Id", "StatSet", "AdditionalStatSets"]),
    ("GrantedEffectStatSets", &["Id"]),
    (
        "GrantedEffectStatSetsPerLevel",
        &["StatSet", "GemLevel", "BaseMultiplier"],
    ),
    ("GrantedEffectsPerLevel", &["GrantedEffect", "Level"]),
];

pub(super) fn read(input: &VerifiedFacts, tables: &Tables<'_>) -> Result<Facts, String> {
    super::super::stat_set_codec::check_columns(input)?;
    for (table, field, kind, target) in [
        ("Stats", "Id", "string", None),
        (
            "GrantedEffectStatSetsPerLevel",
            "StatSet",
            "foreignrow",
            Some("GrantedEffectStatSets"),
        ),
        ("GrantedEffectStatSetsPerLevel", "GemLevel", "i32", None),
        (
            "GrantedEffectStatSetsPerLevel",
            "BaseMultiplier",
            "i32",
            None,
        ),
        (
            "GrantedEffectsPerLevel",
            "GrantedEffect",
            "foreignrow",
            Some("GrantedEffects"),
        ),
        ("GrantedEffectsPerLevel", "Level", "i32", None),
    ] {
        super::super::convert::column(
            input,
            table,
            &[field],
            &json!({
                "name":field,"type":kind,"array":false,"interval":false,
                "references":target.map(|table|json!({"table":table}))
            }),
        )?;
    }
    let stats = identities(tables, "Stats")?;
    let effects = identities(tables, "GrantedEffects")?;
    let sets = identities(tables, "GrantedEffectStatSets")?;
    let mut set_levels: BTreeMap<usize, BTreeMap<i32, Level>> = BTreeMap::new();
    for source_row in 0..tables.len("GrantedEffectStatSetsPerLevel") {
        let row = tables.row("GrantedEffectStatSetsPerLevel", source_row)?;
        let read = || -> Result<_, String> {
            let set = reference(tables, "GrantedEffectStatSets", &row["StatSet"])?;
            let level = integer(&row["GemLevel"])?;
            let raw_multiplier = integer(&row["BaseMultiplier"])?;
            Ok((set, level, raw_multiplier))
        };
        let (set, level, raw_multiplier) =
            read().map_err(|e| format!("GrantedEffectStatSetsPerLevel/{source_row}: {e}"))?;
        if set_levels
            .entry(set)
            .or_default()
            .insert(
                level,
                Level {
                    source_row,
                    raw_multiplier,
                },
            )
            .is_some()
        {
            return Err(format!("duplicate stat-set/level key: {set}/{level}"));
        }
    }
    let mut effect_levels: BTreeMap<usize, BTreeMap<i32, usize>> = BTreeMap::new();
    for source_row in 0..tables.len("GrantedEffectsPerLevel") {
        let row = tables.row("GrantedEffectsPerLevel", source_row)?;
        let read = || -> Result<_, String> {
            Ok((
                reference(tables, "GrantedEffects", &row["GrantedEffect"])?,
                integer(&row["Level"])?,
            ))
        };
        let (effect, level) =
            read().map_err(|e| format!("GrantedEffectsPerLevel/{source_row}: {e}"))?;
        if effect_levels
            .entry(effect)
            .or_default()
            .insert(level, source_row)
            .is_some()
        {
            return Err(format!("duplicate effect/level key: {effect}/{level}"));
        }
    }
    Ok(Facts {
        stats,
        effects,
        sets,
        set_levels,
        effect_levels,
    })
}

pub(super) fn implicit_evidence(
    input: &VerifiedFacts,
    tables: &Tables<'_>,
    primary: usize,
    selected: usize,
) -> Result<Value, String> {
    let mut evidence = Vec::new();
    for row in [primary, selected] {
        let raw = tables.row("GrantedEffectStatSets", row)?;
        let mut fields = BTreeMap::new();
        for field in ["ImplicitStats", "IgnoredStats"] {
            super::super::convert::column(
                input,
                "GrantedEffectStatSets",
                &[field],
                &json!({
                "name":field,"type":"foreignrow","array":true,"interval":false,"references":{"table":"Stats"}}),
            )?;
            let refs = raw[field]
                .as_array()
                .ok_or("implicit/ignored stat refs must be arrays")?;
            let occurrences = refs.iter().enumerate().map(|(position,value)| {
                let source_row = reference(tables,"Stats",value)?;
                Ok(json!({"position":position,"source_row":source_row,"id":tables.row("Stats",source_row)?["Id"]}))
            }).collect::<Result<Vec<Value>,String>>()?;
            fields.insert(field, json!({"raw":refs,"occurrences":occurrences}));
        }
        evidence.push(json!({"source_row":row,"id":raw["Id"],"fields":fields}));
    }
    Ok(json!({"primary_then_selected":evidence,
        "consumer":"ordinary-skills -> existing SkillStatSetDef.implicit_stats; existing primary/additional inheritance and IgnoredStats normalization stays authoritative",
        "scope":"raw arrays and occurrences only; no historical export-index inference or new stat merge law"}))
}

fn identities(tables: &Tables<'_>, table: &str) -> Result<BTreeMap<String, usize>, String> {
    (0..tables.len(table))
        .map(|row| {
            let id = tables.row(table, row)?["Id"]
                .as_str()
                .ok_or("invalid source Id")?;
            identity(id, &format!("{table}/{row}.Id"))?;
            Ok((id.into(), row))
        })
        .collect()
}

fn reference(tables: &Tables<'_>, table: &str, value: &Value) -> Result<usize, String> {
    let row = value
        .as_u64()
        .and_then(|v| usize::try_from(v).ok())
        .ok_or("required physical FK must be usize")?;
    tables.row(table, row)?;
    Ok(row)
}

fn integer(value: &Value) -> Result<i32, String> {
    value
        .as_i64()
        .and_then(|v| i32::try_from(v).ok())
        .ok_or_else(|| "exact i32 source value required".into())
}

pub(super) fn unmapped(
    input: &VerifiedFacts,
    tables: &Tables<'_>,
    delegated_implicit: bool,
) -> Result<Vec<Value>, String> {
    FIELDS.iter().map(|(table, used)| {
        let mut used = used.to_vec();
        if *table == "GrantedEffectStatSets" && delegated_implicit { used.extend(["ImplicitStats","IgnoredStats"]); }
        let path = format!("tables/{table}.json");
        let columns: Vec<_> = input.documents[&path]["columns"].as_array().ok_or("missing columns")?.iter()
            .filter(|c| c["physical_fields"].as_array().is_some_and(|fields|fields.iter().any(|f|!used.iter().any(|name| f.as_str()==Some(*name))))).collect();
        Ok(json!({"table":table,"payload":path,"consumed_columns":used,"columns":columns,
            "source_row_range":{"start":0,"end_exclusive":tables.len(table)},
            "scope":"all identity and level keys checked; only entry-referenced owners/primary numerics and retained codec bindings applied; unused references/columns remain unapplied, not effectless"}))
    }).collect()
}
