//! Gem-quality rows -> the existing quality domain (shared by the ordinary
//! physical-facts producer).
//!
//! A reviewed compatibility decision fixes the supported effect scope. Neither
//! vendor Lua nor an old output is consulted. Stat-set scope columns are
//! retained as diagnostics: current consumers apply quality effect-wide,
//! matching the pinned migration reference. Adding scope semantics or
//! additional effects is a separate calculation change.

use std::collections::{BTreeMap, BTreeSet};

use pobr_data::catalog::{GemQualityStatDef, QualityStat};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Compatibility {
    pub reference_vendor_commit: String,
    pub reference_sha256: String,
    pub scope_policy: String,
    pub effects: Vec<String>,
    pub excluded_effects: BTreeMap<String, String>,
}

#[derive(Deserialize)]
pub(crate) struct IndexedId {
    #[serde(rename = "_index")]
    pub index: usize,
    #[serde(rename = "Id")]
    pub id: String,
}

#[derive(Deserialize)]
pub(crate) struct RawQuality {
    #[serde(rename = "GrantedEffect")]
    pub effect: usize,
    #[serde(rename = "Stats")]
    pub stats: Vec<usize>,
    #[serde(rename = "StatsValuesPermille")]
    pub values: Vec<i32>,
    #[serde(rename = "ApplyToStatSets")]
    pub sets: Vec<u32>,
    #[serde(rename = "AltStats")]
    pub alt_stats: Vec<usize>,
    #[serde(rename = "AltStatValuesPermille")]
    pub alt_values: Vec<i32>,
    #[serde(rename = "AltApplyToStatSets")]
    pub alt_sets: Vec<u32>,
}

#[derive(Serialize)]
pub(crate) struct Scope {
    pub main: Vec<u32>,
    pub alt: Vec<u32>,
}

pub(crate) fn index(rows: Vec<IndexedId>, table: &str) -> Result<BTreeMap<usize, String>, String> {
    let mut ids = BTreeSet::new();
    let mut by_index = BTreeMap::new();
    for row in rows {
        if row.id.is_empty()
            || !ids.insert(row.id.clone())
            || by_index.insert(row.index, row.id).is_some()
        {
            return Err(format!("{table}: empty or duplicate ID / row index"));
        }
    }
    Ok(by_index)
}

/// Validate the existing effect-wide model decision without selecting by gem/support kind.
pub(crate) fn validate_compatibility(compatibility: &Compatibility) -> Result<(), String> {
    if compatibility.scope_policy != "preserve_effect_wide_quality" {
        return Err("unsupported quality scope policy".into());
    }
    let enabled: BTreeSet<_> = compatibility.effects.iter().cloned().collect();
    let excluded = &compatibility.excluded_effects;
    if enabled.is_empty()
        || enabled.len() != compatibility.effects.len()
        || enabled
            .iter()
            .any(|id| id.is_empty() || excluded.contains_key(id))
        || excluded
            .iter()
            .any(|(id, reason)| id.is_empty() || reason.trim().is_empty())
    {
        return Err("invalid or overlapping quality compatibility scope".into());
    }
    Ok(())
}

pub(crate) struct Converted {
    pub effects: Vec<GemQualityStatDef>,
    pub scopes: BTreeMap<String, Scope>,
}

/// Shared parsed-row conversion; raw and authenticated physical inputs retain identical math.
pub(crate) fn convert_rows(
    effects: &BTreeMap<usize, String>,
    stats: &BTreeMap<usize, String>,
    rows: Vec<RawQuality>,
    compatibility: &Compatibility,
) -> Result<Converted, String> {
    validate_compatibility(compatibility)?;
    let enabled: BTreeSet<_> = compatibility.effects.iter().cloned().collect();
    let excluded = &compatibility.excluded_effects;
    let mut seen = BTreeSet::new();
    let mut result = BTreeMap::new();
    let mut scopes = BTreeMap::new();
    for row in rows {
        let id = effects
            .get(&row.effect)
            .ok_or_else(|| format!("quality: dangling GrantedEffect {}", row.effect))?;
        if !seen.insert(id.clone()) {
            return Err(format!("quality: duplicate effect {id}"));
        }
        if !enabled.contains(id) && !excluded.contains_key(id) {
            return Err(format!(
                "quality: effect {id} has no reviewed compatibility decision"
            ));
        }
        let mut quality = Vec::new();
        for (keys, values, alt) in [
            (&row.stats, &row.values, false),
            (&row.alt_stats, &row.alt_values, true),
        ] {
            if keys.len() != values.len() {
                return Err(format!(
                    "quality {id}: stat/value length mismatch (alt={alt})"
                ));
            }
            for (&key, &value) in keys.iter().zip(values) {
                let stat = stats
                    .get(&key)
                    .ok_or_else(|| format!("quality {id}: dangling stat {key}"))?;
                quality.push(QualityStat {
                    stat: stat.clone(),
                    per_quality_rate: f64::from(value) / 1000.0,
                    alt,
                });
            }
        }
        if !row.sets.is_empty() || !row.alt_sets.is_empty() {
            scopes.insert(
                id.clone(),
                Scope {
                    main: row.sets,
                    alt: row.alt_sets,
                },
            );
        }
        if enabled.contains(id) {
            if quality.is_empty() {
                return Err(format!(
                    "quality {id}: reviewed effect has no quality stats"
                ));
            }
            result.insert(
                id.clone(),
                GemQualityStatDef {
                    effect_id: id.clone(),
                    stats: quality,
                },
            );
        }
    }
    for id in enabled.iter().chain(excluded.keys()) {
        if !seen.contains(id) {
            return Err(format!(
                "quality: reviewed effect {id} disappeared from the input"
            ));
        }
    }
    Ok(Converted {
        effects: result.into_values().collect(),
        scopes,
    })
}

#[cfg(test)]
mod tests;
