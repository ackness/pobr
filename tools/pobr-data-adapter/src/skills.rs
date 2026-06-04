//! 技能宝石域适配：`SkillGems` / `GrantedEffects` / `ActiveSkills` 原始 JSON
//! → PoBR 最小 JSON（`skill_gems.json` + `granted_effects.json` + 技能名边车）。
//!
//! 外键解析方式与 base items 域一致（整型索引 → 稳定字符串 Id）：
//! - 宝石身份取自 `SkillGems.BaseItemType` → `BaseItemTypes.Id`；
//! - 授予效果的 `ActiveSkill` 整型索引 → `ActiveSkills.Id`；
//! - `ActiveSkills.GrantedEffect` 本身即字符串 Id（GrantedEffects.Id），无需查表。
//!
//! TODO（后续切片）：`SkillGems.GemEffects` FK 指向的 `GemEffects` 表当前 pipeline
//! 未导出，故宝石→授予效果的直接连边暂缺；分等级缩放
//! （`GrantedEffectsPerLevel` 的 cost / cooldown / attack time / 伤害进度）亦未接入。

use std::collections::BTreeMap;
use std::path::Path;

use pobr_data::catalog::{GrantedEffectDef, SkillGemDef};
use serde::Deserialize;

use crate::{is_placeholder, read_json, resolve};

/// 辅助宝石的 `GemType` 枚举值（GGG 原始：0=主动，1=辅助）。
const GEM_TYPE_SUPPORT: u32 = 1;

// ---- 原始 .dat JSON 行结构（只取需要的列）----

#[derive(Deserialize)]
struct RawBaseItemId {
    #[serde(rename = "_index")]
    index: usize,
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "Name", default)]
    name: String,
}

#[derive(Deserialize)]
struct RawSkillGem {
    #[serde(rename = "BaseItemType")]
    base_item_type: Option<usize>,
    #[serde(rename = "GemType")]
    gem_type: Option<u32>,
    #[serde(rename = "GemColour")]
    gem_colour: Option<u32>,
    #[serde(rename = "MinLevelReq")]
    min_level_req: Option<i64>,
    #[serde(rename = "StrengthRequirementPercent")]
    str_pct: Option<i64>,
    #[serde(rename = "DexterityRequirementPercent")]
    dex_pct: Option<i64>,
    #[serde(rename = "IntelligenceRequirementPercent")]
    int_pct: Option<i64>,
}

#[derive(Deserialize)]
struct RawGrantedEffect {
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "IsSupport")]
    is_support: Option<bool>,
    /// 整型索引 → `ActiveSkills`；辅助效果为负数/越界。
    #[serde(rename = "ActiveSkill")]
    active_skill: Option<i64>,
    #[serde(rename = "CastTime")]
    cast_time: Option<i64>,
    #[serde(rename = "AllowedActiveSkillTypes", default)]
    allowed_active_skill_types: Vec<u32>,
}

#[derive(Deserialize)]
struct RawActiveSkillName {
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "DisplayedName", default)]
    displayed_name: Option<String>,
}

#[derive(Deserialize)]
struct RawActiveSkillTwName {
    #[serde(rename = "_index")]
    index: usize,
    #[serde(rename = "DisplayedName", default)]
    displayed_name: Option<String>,
}

fn clamp_u32(v: Option<i64>) -> u32 {
    v.unwrap_or(0).max(0) as u32
}

/// 适配后的技能宝石域产物。
pub struct SkillsBundle {
    pub gems: Vec<SkillGemDef>,
    pub effects: Vec<GrantedEffectDef>,
    /// 主动技能显示名边车（`active_skill_id -> 繁中名称`）。
    pub zh_skill_names: BTreeMap<String, String>,
    pub gems_total: usize,
    pub effects_total: usize,
}

/// 从原始表适配出技能宝石 + 授予效果 + 繁中技能名。
pub fn adapt_skills(en: &Path, tw: &Path) -> Result<SkillsBundle, String> {
    // 外键解析表
    let base_rows = read_json::<Vec<RawBaseItemId>>(&en.join("BaseItemTypes.json"))?;
    let base_ids = id_lookup_from_base(&base_rows);
    let active_skills = read_json::<Vec<RawActiveSkillName>>(&en.join("ActiveSkills.json"))?;
    let active_ids: Vec<String> = active_skills.iter().map(|a| a.id.clone()).collect();

    // ---- 宝石 ----
    let raw_gems = read_json::<Vec<RawSkillGem>>(&en.join("SkillGems.json"))?;
    let gems_total = raw_gems.len();
    let mut gems = Vec::new();
    for raw in raw_gems {
        let Some(idx) = raw.base_item_type else {
            continue; // 无基底 → 开发占位
        };
        let Some((id, name)) = base_ids.get(idx).cloned() else {
            continue;
        };
        if id.is_empty() || is_placeholder(&name) {
            continue;
        }
        let is_support = raw.gem_type == Some(GEM_TYPE_SUPPORT);
        gems.push(SkillGemDef {
            id,
            gem_type: raw.gem_type,
            gem_colour: raw.gem_colour,
            min_level_req: clamp_u32(raw.min_level_req),
            str_pct: clamp_u32(raw.str_pct),
            dex_pct: clamp_u32(raw.dex_pct),
            int_pct: clamp_u32(raw.int_pct),
            is_support,
        });
    }
    gems.sort_by(|a, b| a.id.cmp(&b.id));

    // ---- 授予效果 ----
    let raw_effects = read_json::<Vec<RawGrantedEffect>>(&en.join("GrantedEffects.json"))?;
    let effects_total = raw_effects.len();
    let mut effects = Vec::new();
    for raw in raw_effects {
        if raw.id.is_empty() {
            continue;
        }
        let active_skill = raw
            .active_skill
            .filter(|&i| i >= 0)
            .and_then(|i| resolve(&active_ids, i as usize));
        let cast_time = raw.cast_time.filter(|&t| t > 0).map(|t| t as u32);
        effects.push(GrantedEffectDef {
            id: raw.id,
            is_support: raw.is_support.unwrap_or(false),
            active_skill,
            cast_time,
            allowed_active_skill_types: raw.allowed_active_skill_types,
        });
    }
    effects.sort_by(|a, b| a.id.cmp(&b.id));

    // ---- 繁中技能显示名边车（key = ActiveSkills.Id，按英文 canonical 去同名）----
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

    Ok(SkillsBundle {
        gems,
        effects,
        zh_skill_names,
        gems_total,
        effects_total,
    })
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
