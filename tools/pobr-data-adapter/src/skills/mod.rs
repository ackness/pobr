//! Skill gem domain adapter: adapts the raw `SkillGems` / `GrantedEffects` /
//! `GrantedEffectsPerLevel` / `ActiveSkills` JSON into PoBR's minimal JSON
//! (`skill_gems.json` + `granted_effects.json` + `granted_effect_levels.json` + a skill-name sidecar).
//!
//! Foreign-key resolution follows the same approach as the base items
//! domain (integer index -> stable string Id):
//! - Gem identity comes from `SkillGems.BaseItemType` -> `BaseItemTypes.Id`;
//! - A granted effect's `ActiveSkill` integer index -> `ActiveSkills.Id`;
//! - `GrantedEffectsPerLevel.GrantedEffect`'s integer `_index` -> `GrantedEffects.Id`;
//! - `GrantedEffects.StatSet` / `CostTypes` stay as raw indices (their
//!   target table `GrantedEffectStatSets*` isn't downloaded yet; once it
//!   is, per-level damage stats will be resolved via `stat_set`).
//!
//! Already wired up: per-level cost / cooldown / attack time
//! (`granted_effect_levels.json`) and per-level **damage stat values**
//! (`granted_effect_stat_sets.json`, see [`adapt_stat_sets`]).
//!
//! Module boundary: this file only handles **orchestration and shared Raw
//! types**; domain logic is split by table family across the five
//! submodules [`gems`] / [`effects`] / [`levels`] / [`stat_sets`] / [`quality`].

mod effects;
mod gems;
mod levels;
mod quality;
mod stat_sets;

use std::collections::BTreeMap;
use std::path::Path;

use pobr_data::catalog::{GrantedEffectDef, SkillGemDef, SkillLevelDef};
use serde::Deserialize;

use crate::read_json;

pub use effects::adapt_cost_types;
pub use stat_sets::adapt_stat_sets;

// Raw .dat JSON row structures shared across submodules (only the columns we need)

/// A three-column `_index` + `Id` (+ optional `Name`) row — the generic read
/// structure for foreign-key target tables like `BaseItemTypes` / `ActiveSkillType`.
#[derive(Deserialize)]
pub(crate) struct RawBaseItemId {
    #[serde(rename = "_index")]
    pub(crate) index: usize,
    #[serde(rename = "Id")]
    pub(crate) id: String,
    #[serde(rename = "Name", default)]
    pub(crate) name: String,
}

#[derive(Deserialize)]
pub(crate) struct RawActiveSkillName {
    #[serde(rename = "Id")]
    pub(crate) id: String,
    #[serde(rename = "DisplayedName", default)]
    pub(crate) displayed_name: Option<String>,
    /// Skill-type flags (`SkillType` enum values: 1=Attack, 2=Spell...).
    #[serde(rename = "ActiveSkillTypes", default)]
    pub(crate) active_skill_types: Vec<u32>,
}

pub(crate) fn clamp_u32(v: Option<i64>) -> u32 {
    v.unwrap_or(0).max(0) as u32
}

/// Builds an `_index -> (Id, Name)` lookup from base item rows (`_index` is contiguous; out-of-range returns None).
fn id_lookup_from_base(rows: &[RawBaseItemId]) -> Vec<(String, String)> {
    let max = rows.iter().map(|r| r.index).max().map_or(0, |m| m + 1);
    let mut table = vec![(String::new(), String::new()); max];
    for r in rows {
        table[r.index] = (r.id.clone(), r.name.clone());
    }
    table
}

/// The adapted skill gem domain output.
pub struct SkillsBundle {
    pub gems: Vec<SkillGemDef>,
    pub effects: Vec<GrantedEffectDef>,
    /// Per-level parameters: `granted_effect_id -> ascending level array`.
    pub levels: BTreeMap<String, Vec<SkillLevelDef>>,
    /// Active skill display-name sidecar (`active_skill_id -> Traditional Chinese name`).
    pub zh_skill_names: BTreeMap<String, String>,
    pub gems_total: usize,
    pub effects_total: usize,
    pub level_rows_total: usize,
}

/// Adapts skill gems + granted effects + Traditional Chinese skill names from the raw tables (the orchestration entry point).
pub fn adapt_skills(en: &Path, tw: &Path) -> Result<SkillsBundle, String> {
    // Foreign-key resolution tables shared across submodules
    let base_rows = read_json::<Vec<RawBaseItemId>>(&en.join("BaseItemTypes.json"))?;
    let base_ids = id_lookup_from_base(&base_rows);
    let active_skills = read_json::<Vec<RawActiveSkillName>>(&en.join("ActiveSkills.json"))?;
    // Skill-type enum: row index -> name (e.g. `Attack`/`Spell`/`Projectile`).
    let skill_type_names: Vec<String> =
        read_json::<Vec<RawBaseItemId>>(&en.join("ActiveSkillType.json"))?
            .into_iter()
            .map(|r| r.id)
            .collect();

    let (gems, gems_total) = gems::adapt_gems(en, &base_ids)?;
    let (effects, effects_total, effect_id_by_index) =
        effects::adapt_effects(en, &active_skills, &skill_type_names)?;
    // Crit chance lives at the stat-set level, so build the lookup first and then join by (effect, level).
    let crit_by_effect = stat_sets::crit_from_statset_levels(en)?;
    let (levels, level_rows_total) =
        levels::adapt_levels(en, &effect_id_by_index, &crit_by_effect)?;
    let zh_skill_names = effects::adapt_zh_skill_names(tw, &active_skills)?;

    Ok(SkillsBundle {
        gems,
        effects,
        levels,
        zh_skill_names,
        gems_total,
        effects_total,
        level_rows_total,
    })
}
