//! 技能宝石域适配：`SkillGems` / `GrantedEffects` / `GrantedEffectsPerLevel` /
//! `ActiveSkills` 原始 JSON → PoBR 最小 JSON（`skill_gems.json` +
//! `granted_effects.json` + `granted_effect_levels.json` + 技能名边车）。
//!
//! 外键解析方式与 base items 域一致（整型索引 → 稳定字符串 Id）：
//! - 宝石身份取自 `SkillGems.BaseItemType` → `BaseItemTypes.Id`；
//! - 授予效果的 `ActiveSkill` 整型索引 → `ActiveSkills.Id`；
//! - `GrantedEffectsPerLevel.GrantedEffect` 整型 `_index` → `GrantedEffects.Id`；
//! - `GrantedEffects.StatSet` / `CostTypes` 保留为原始索引（其目标表
//!   `GrantedEffectStatSets*` 当前未下载，待重下后按 `stat_set` 解析分等级伤害 stat）。
//!
//! 已接入分等级 cost / cooldown / attack time（`granted_effect_levels.json`）
//! 及分等级**伤害 stat 值**（`granted_effect_stat_sets.json`，见 [`adapt_stat_sets`]）。
//!
//! 模块边界：本文件只做**编排与共享 Raw 类型**；域逻辑按表族
//! 拆分在 [`gems`] / [`effects`] / [`levels`] / [`stat_sets`] / [`quality`] 五个子模块。

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

// 跨子模块共享的原始 .dat JSON 行结构（只取需要的列）

/// `_index` + `Id`（+ 可选 `Name`）三列行——`BaseItemTypes` / `ActiveSkillType`
/// 等外键目标表的通用读取结构。
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
    /// 技能类型标志（`SkillType` 枚举值：1=Attack、2=Spell…）。
    #[serde(rename = "ActiveSkillTypes", default)]
    pub(crate) active_skill_types: Vec<u32>,
}

pub(crate) fn clamp_u32(v: Option<i64>) -> u32 {
    v.unwrap_or(0).max(0) as u32
}

/// 把 base item 行建成 `_index -> (Id, Name)` 查表（`_index` 连续，越界返回 None）。
fn id_lookup_from_base(rows: &[RawBaseItemId]) -> Vec<(String, String)> {
    let max = rows.iter().map(|r| r.index).max().map_or(0, |m| m + 1);
    let mut table = vec![(String::new(), String::new()); max];
    for r in rows {
        table[r.index] = (r.id.clone(), r.name.clone());
    }
    table
}

/// 适配后的技能宝石域产物。
pub struct SkillsBundle {
    pub gems: Vec<SkillGemDef>,
    pub effects: Vec<GrantedEffectDef>,
    /// 分等级参数：`granted_effect_id -> 升序等级数组`。
    pub levels: BTreeMap<String, Vec<SkillLevelDef>>,
    /// 主动技能显示名边车（`active_skill_id -> 繁中名称`）。
    pub zh_skill_names: BTreeMap<String, String>,
    pub gems_total: usize,
    pub effects_total: usize,
    pub level_rows_total: usize,
}

/// 从原始表适配出技能宝石 + 授予效果 + 繁中技能名（编排入口）。
pub fn adapt_skills(en: &Path, tw: &Path) -> Result<SkillsBundle, String> {
    // 跨子模块共享的外键解析表
    let base_rows = read_json::<Vec<RawBaseItemId>>(&en.join("BaseItemTypes.json"))?;
    let base_ids = id_lookup_from_base(&base_rows);
    let active_skills = read_json::<Vec<RawActiveSkillName>>(&en.join("ActiveSkills.json"))?;
    // 技能类型枚举：行索引 → 名称（如 `Attack`/`Spell`/`Projectile`）。
    let skill_type_names: Vec<String> =
        read_json::<Vec<RawBaseItemId>>(&en.join("ActiveSkillType.json"))?
            .into_iter()
            .map(|r| r.id)
            .collect();

    let (gems, gems_total) = gems::adapt_gems(en, &base_ids)?;
    let (effects, effects_total, effect_id_by_index) =
        effects::adapt_effects(en, &active_skills, &skill_type_names)?;
    // 暴击率挂在 stat-set 维度，先建查表再按 (effect, level) join。
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
