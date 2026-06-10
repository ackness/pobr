//! 授予效果表适配（`GrantedEffects` → `granted_effects.json`）+ 繁中技能名边车 +
//! 消耗资源类型（`CostTypes` → `cost_types.json`）。
//!
//! 授予效果的 `ActiveSkill` 整型索引 → `ActiveSkills.Id`；技能类型标志
//! （Attack/Spell…）取自关联 ActiveSkills 行，索引经 `ActiveSkillType` 解析为名称。

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
    /// 整型索引 → `ActiveSkills`；辅助效果为负数/越界。
    #[serde(rename = "ActiveSkill")]
    active_skill: Option<i64>,
    #[serde(rename = "CastTime")]
    cast_time: Option<i64>,
    /// require 后缀表达式（FK 索引序列 → `ActiveSkillType`；AND/OR/NOT 是特殊行）。
    /// = PoB2 spec.lua grantedeffects 列 `SupportTypes`。
    #[serde(rename = "AllowedActiveSkillTypes", default)]
    allowed_active_skill_types: Vec<u32>,
    /// 兼容 support 并入主动技能的类型名单（= PoB2 spec `AddTypes`）。
    #[serde(rename = "AddedActiveSkillTypes", default)]
    added_active_skill_types: Vec<u32>,
    /// exclude 后缀表达式（= PoB2 spec `ExcludeTypes`）。
    #[serde(rename = "ExcludedActiveSkillTypes", default)]
    excluded_active_skill_types: Vec<u32>,
    /// 主动效果不可被任何 support 支援（spec 列 9）。
    #[serde(rename = "CannotBeSupported", default)]
    cannot_be_supported: bool,
    /// 该 support 仅能支援宝石授予的技能（spec 列 7；社区 schema 列名带 s）。
    #[serde(rename = "SupportsGemsOnly", default)]
    supports_gems_only: bool,
    /// `GrantedEffectStatSets` 外键索引（负数/越界归一化为 None）。
    #[serde(rename = "StatSet")]
    stat_set: Option<i64>,
    /// 消耗类型外键索引列表（如 `[0]`）。
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

/// 适配 `GrantedEffects` 表为按 id 排序的效果定义。
///
/// 返回 `(条目, 原始总行数, _index → Id 查表)`——查表供 per-level 行解析
/// `GrantedEffect` 外键（见 [`super::levels`]）。
pub(super) fn adapt_effects(
    en: &Path,
    active_skills: &[RawActiveSkillName],
    skill_type_names: &[String],
) -> Result<(Vec<GrantedEffectDef>, usize, Vec<String>), String> {
    let active_ids: Vec<String> = active_skills.iter().map(|a| a.id.clone()).collect();

    // raw_effects 按文件顺序即 `_index` 顺序；先建 `_index -> Id` 表供 per-level FK 解析。
    let raw_effects = read_json::<Vec<RawGrantedEffect>>(&en.join("GrantedEffects.json"))?;
    let effects_total = raw_effects.len();
    let effect_id_by_index: Vec<String> = raw_effects.iter().map(|r| r.id.clone()).collect();

    // 类型表达式 FK 解析：索引 → `ActiveSkillType.Id` 名称（AND/OR/NOT 即特殊行），
    // 保留 token 顺序（后缀表达式语义依赖顺序）。悬空 FK 跳过并计数（外键质量报表，
    // 见蓝图 §5：>0 时列入 commit message）。
    let mut dangling_type_fk = 0usize;
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
        // 技能类型标志（Attack/Spell…）取自关联 ActiveSkills 行，索引 → 名称。
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
            skill_types,
            cost_types: raw.cost_types,
        });
    }
    if dangling_type_fk > 0 {
        eprintln!("granted_effects: 类型表达式悬空 FK {dangling_type_fk} 处（已跳过）");
    }
    effects.sort_by(|a, b| a.id.cmp(&b.id));
    Ok((effects, effects_total, effect_id_by_index))
}

/// 繁中技能显示名边车（key = ActiveSkills.Id，按英文 canonical 去同名）。
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

// ---- 消耗资源类型（CostTypes 域）----

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

/// 适配 `CostTypes.dat` 为按索引升序的资源类型数组（[`GrantedEffectDef::cost_types`]
/// 外键的目标表）。索引连续，直接落位为有序 Vec。
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
