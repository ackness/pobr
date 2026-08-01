//! Granted effect table adapter (`GrantedEffects` -> `granted_effects.json`)
//! plus a Traditional Chinese skill-name sidecar plus resource cost types
//! (`CostTypes` -> `cost_types.json`).
//!
//! A granted effect's `ActiveSkill` integer index -> `ActiveSkills.Id`;
//! skill-type flags (Attack/Spell...) come from the linked ActiveSkills row,
//! whose indices resolve to names via `ActiveSkillType`.

use std::collections::BTreeMap;
use std::path::Path;

use pobr_data::catalog::{CostTypeDef, GrantedEffectDef};
use serde::Deserialize;

use crate::{is_placeholder, read_json, resolve};

use super::RawActiveSkillName;

#[derive(Deserialize)]
struct RawGrantedEffect {
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "IsSupport")]
    is_support: Option<bool>,
    /// Integer index -> `ActiveSkills`; support effects have a negative/out-of-range value.
    #[serde(rename = "ActiveSkill")]
    active_skill: Option<i64>,
    #[serde(rename = "CastTime")]
    cast_time: Option<i64>,
    /// The require postfix expression (a sequence of FK indices ->
    /// `ActiveSkillType`; AND/OR/NOT are special rows). = PoB2 spec.lua
    /// grantedeffects's `SupportTypes` column.
    #[serde(rename = "AllowedActiveSkillTypes", default)]
    allowed_active_skill_types: Vec<u32>,
    /// The type list that lets a support merge compatibility into an active
    /// skill (= PoB2 spec's `AddTypes`).
    #[serde(rename = "AddedActiveSkillTypes", default)]
    added_active_skill_types: Vec<u32>,
    /// The exclude postfix expression (= PoB2 spec's `ExcludeTypes`).
    #[serde(rename = "ExcludedActiveSkillTypes", default)]
    excluded_active_skill_types: Vec<u32>,
    /// The active effect can't be supported by any support gem (spec column 9).
    #[serde(rename = "CannotBeSupported", default)]
    cannot_be_supported: bool,
    /// This support can only support skills granted by a gem (spec column 7; the community schema's column name has a trailing s).
    #[serde(rename = "SupportsGemsOnly", default)]
    supports_gems_only: bool,
    /// `GrantedEffectStatSets` foreign-key index (negative/out-of-range normalizes to None).
    #[serde(rename = "StatSet")]
    stat_set: Option<i64>,
    /// **Additional** statSet foreign-key indices (FK -> `GrantedEffectStatSets`;
    /// verified during W0 that the target is the statSet table, not another
    /// GrantedEffects row). Column order is preserved.
    #[serde(rename = "AdditionalStatSets", default)]
    additional_stat_sets: Vec<i64>,
    /// The list of cost-type foreign-key indices (e.g. `[0]`).
    #[serde(rename = "CostTypes", default)]
    cost_types: Vec<u32>,
}

#[derive(Deserialize)]
struct RawActiveSkillTwName {
    #[serde(rename = "_index")]
    index: usize,
    #[serde(rename = "DisplayedName", default)]
    displayed_name: Option<String>,
}

/// Adapts the `GrantedEffects` table into id-sorted effect definitions.
///
/// Returns `(entries, raw row total, _index -> Id lookup)` — the lookup is
/// used by per-level rows to resolve the `GrantedEffect` foreign key (see
/// [`super::levels`]).
pub(super) fn adapt_effects(
    en: &Path,
    active_skills: &[RawActiveSkillName],
    skill_type_names: &[String],
) -> Result<(Vec<GrantedEffectDef>, usize, Vec<String>), String> {
    let active_ids: Vec<String> = active_skills.iter().map(|a| a.id.clone()).collect();

    // raw_effects is in file order, which is `_index` order; build the `_index -> Id` table first for per-level rows to resolve.
    let raw_effects = read_json::<Vec<RawGrantedEffect>>(&en.join("GrantedEffects.json"))?;
    let effects_total = raw_effects.len();
    let effect_id_by_index: Vec<String> = raw_effects.iter().map(|r| r.id.clone()).collect();

    // statSet `_index -> Id` lookup (resolves the AdditionalStatSets FK to a stable id, T5.2).
    #[derive(Deserialize)]
    struct RawStatSetId {
        #[serde(rename = "Id")]
        id: String,
    }
    let stat_set_ids: Vec<String> =
        read_json::<Vec<RawStatSetId>>(&en.join("GrantedEffectStatSets.json"))?
            .into_iter()
            .map(|r| r.id)
            .collect();

    // Type-expression FK resolution: index -> `ActiveSkillType.Id` name
    // (AND/OR/NOT are special rows), preserving token order (postfix
    // expression semantics depend on it). A dangling FK is skipped and
    // counted (a foreign-key quality metric — see below: when >0, it goes into the commit message).
    let mut dangling_type_fk = 0usize;
    let mut dangling_statset_fk = 0usize;
    let mut resolve_type_tokens = |idxs: &[u32]| -> Vec<String> {
        idxs.iter()
            .filter_map(|&t| match skill_type_names.get(t as usize) {
                Some(name) if !name.is_empty() => Some(name.clone()),
                _ => {
                    dangling_type_fk += 1;
                    None
                }
            })
            .collect()
    };

    let mut effects = Vec::new();
    for raw in raw_effects {
        if raw.id.is_empty() {
            continue;
        }
        let active_idx = raw.active_skill.filter(|&i| i >= 0).map(|i| i as usize);
        let active_skill = active_idx.and_then(|i| resolve(&active_ids, i));
        // Skill-type flags (Attack/Spell...) come from the linked ActiveSkills row, index -> name.
        let skill_types = active_idx
            .and_then(|i| active_skills.get(i))
            .map(|a| {
                a.active_skill_types
                    .iter()
                    .filter_map(|&t| skill_type_names.get(t as usize).cloned())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let cast_time = raw.cast_time.filter(|&t| t > 0).map(|t| t as u32);
        let stat_set = raw.stat_set.filter(|&i| i >= 0).map(|i| i as u32);
        // Additional statSet: FK index -> stable id (a dangling FK is skipped and counted toward the foreign-key quality metric).
        let additional_stat_set_ids: Vec<String> = raw
            .additional_stat_sets
            .iter()
            .filter_map(|&i| {
                let resolved = usize::try_from(i)
                    .ok()
                    .and_then(|i| stat_set_ids.get(i).filter(|s| !s.is_empty()).cloned());
                if resolved.is_none() {
                    dangling_statset_fk += 1;
                }
                resolved
            })
            .collect();
        effects.push(GrantedEffectDef {
            id: raw.id,
            is_support: raw.is_support.unwrap_or(false),
            active_skill,
            cast_time,
            require_skill_types: resolve_type_tokens(&raw.allowed_active_skill_types),
            add_skill_types: resolve_type_tokens(&raw.added_active_skill_types),
            exclude_skill_types: resolve_type_tokens(&raw.excluded_active_skill_types),
            cannot_be_supported: raw.cannot_be_supported,
            support_gems_only: raw.supports_gems_only,
            stat_set,
            additional_stat_set_ids,
            skill_types,
            cost_types: raw.cost_types,
            // Minion foreign-key fields: always empty in the base artifact,
            // merged in from overlay/granted_effect_minions.json during
            // gamedata loading (the adapter doesn't produce these).
            minion_list: Vec::new(),
            add_minion_list: Vec::new(),
            minion_uses: Vec::new(),
            minion_has_item_set: false,
        });
    }
    if dangling_type_fk > 0 {
        eprintln!("granted_effects: {dangling_type_fk} dangling type-expression FK(s) (skipped)");
    }
    if dangling_statset_fk > 0 {
        eprintln!(
            "granted_effects: {dangling_statset_fk} dangling AdditionalStatSets FK(s) (skipped)"
        );
    }
    effects.sort_by(|a, b| a.id.cmp(&b.id));
    Ok((effects, effects_total, effect_id_by_index))
}

/// Traditional Chinese skill display-name sidecar (key = ActiveSkills.Id, deduplicated against the English canonical name).
pub(super) fn adapt_zh_skill_names(
    tw: &Path,
    active_skills: &[RawActiveSkillName],
) -> Result<BTreeMap<String, String>, String> {
    let tw_rows = read_json::<Vec<RawActiveSkillTwName>>(&tw.join("ActiveSkills.json"))?;
    let tw_by_index: BTreeMap<usize, String> = tw_rows
        .into_iter()
        .filter_map(|r| r.displayed_name.map(|n| (r.index, n)))
        .collect();
    let mut zh_skill_names: BTreeMap<String, String> = BTreeMap::new();
    for (index, a) in active_skills.iter().enumerate() {
        let en_name = a.displayed_name.as_deref().unwrap_or_default();
        if a.id.is_empty() || is_placeholder(en_name) {
            continue;
        }
        if let Some(zh) = tw_by_index.get(&index)
            && !zh.is_empty()
            && zh != en_name
        {
            zh_skill_names.insert(a.id.clone(), zh.clone());
        }
    }
    Ok(zh_skill_names)
}

// Resource cost types (the CostTypes domain)

#[derive(Deserialize)]
struct RawCostType {
    #[serde(rename = "_index")]
    index: usize,
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "Divisor")]
    divisor: Option<i64>,
    #[serde(rename = "PerMinute", default)]
    per_minute: bool,
}

/// Adapts `CostTypes.dat` into an index-ascending resource-type array (the
/// target table for the [`GrantedEffectDef::cost_types`] foreign key). Indices are contiguous, so this lands directly into an ordered Vec.
pub fn adapt_cost_types(en: &Path) -> Result<Vec<CostTypeDef>, String> {
    let raw = read_json::<Vec<RawCostType>>(&en.join("CostTypes.json"))?;
    let max = raw.iter().map(|r| r.index).max().map_or(0, |m| m + 1);
    let mut out = vec![
        CostTypeDef {
            id: String::new(),
            divisor: 1,
            per_minute: false,
        };
        max
    ];
    for r in raw {
        out[r.index] = CostTypeDef {
            id: r.id,
            divisor: r.divisor.filter(|&d| d > 0).unwrap_or(1) as u32,
            per_minute: r.per_minute,
        };
    }
    Ok(out)
}
