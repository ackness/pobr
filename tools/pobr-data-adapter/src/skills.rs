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

use std::collections::BTreeMap;
use std::path::Path;

use pobr_data::catalog::{
    CostTypeDef, GrantedEffectDef, SkillDamageStat, SkillGemDef, SkillLevelDef, SkillStatSetDef,
    SkillStatSetLevel,
};
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
    /// `GrantedEffectStatSets` 外键索引（负数/越界归一化为 None）。
    #[serde(rename = "StatSet")]
    stat_set: Option<i64>,
    /// 消耗类型外键索引列表（如 `[0]`）。
    #[serde(rename = "CostTypes", default)]
    cost_types: Vec<u32>,
}

#[derive(Deserialize)]
struct RawGrantedEffectPerLevel {
    /// `GrantedEffects` 的 `_index`（0-based 外键）。
    #[serde(rename = "GrantedEffect")]
    granted_effect: Option<usize>,
    #[serde(rename = "Level")]
    level: Option<i64>,
    #[serde(rename = "Cooldown")]
    cooldown: Option<i64>,
    #[serde(rename = "AttackTime")]
    attack_time: Option<i64>,
    #[serde(rename = "CostAmounts", default)]
    cost_amounts: Vec<i64>,
    /// 攻击速度乘数（百分点，可负；作用于武器攻击速率）。如 Flicker Strike -50。
    #[serde(rename = "AttackSpeedMultiplier")]
    attack_speed_multiplier: Option<i64>,
    /// 伤害基础倍率（PoB `baseMultiplier`，stat-set BaseMultiplier 缺失时的回退源）。
    #[serde(rename = "BaseMultiplier")]
    base_multiplier: Option<f64>,
}

#[derive(Deserialize)]
struct RawActiveSkillName {
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "DisplayedName", default)]
    displayed_name: Option<String>,
    /// 技能类型标志（`SkillType` 枚举值：1=Attack、2=Spell…）。
    #[serde(rename = "ActiveSkillTypes", default)]
    active_skill_types: Vec<u32>,
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
    /// 分等级参数：`granted_effect_id -> 升序等级数组`。
    pub levels: BTreeMap<String, Vec<SkillLevelDef>>,
    /// 主动技能显示名边车（`active_skill_id -> 繁中名称`）。
    pub zh_skill_names: BTreeMap<String, String>,
    pub gems_total: usize,
    pub effects_total: usize,
    pub level_rows_total: usize,
}

/// 从原始表适配出技能宝石 + 授予效果 + 繁中技能名。
pub fn adapt_skills(en: &Path, tw: &Path) -> Result<SkillsBundle, String> {
    // 外键解析表
    let base_rows = read_json::<Vec<RawBaseItemId>>(&en.join("BaseItemTypes.json"))?;
    let base_ids = id_lookup_from_base(&base_rows);
    let active_skills = read_json::<Vec<RawActiveSkillName>>(&en.join("ActiveSkills.json"))?;
    let active_ids: Vec<String> = active_skills.iter().map(|a| a.id.clone()).collect();
    // 技能类型枚举：行索引 → 名称（如 `Attack`/`Spell`/`Projectile`）。
    let skill_type_names: Vec<String> =
        read_json::<Vec<RawBaseItemId>>(&en.join("ActiveSkillType.json"))?
            .into_iter()
            .map(|r| r.id)
            .collect();

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
    // raw_effects 按文件顺序即 `_index` 顺序；先建 `_index -> Id` 表供 per-level FK 解析。
    let raw_effects = read_json::<Vec<RawGrantedEffect>>(&en.join("GrantedEffects.json"))?;
    let effects_total = raw_effects.len();
    let effect_id_by_index: Vec<String> = raw_effects.iter().map(|r| r.id.clone()).collect();

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
            allowed_active_skill_types: raw.allowed_active_skill_types,
            stat_set,
            skill_types,
            cost_types: raw.cost_types,
        });
    }
    effects.sort_by(|a, b| a.id.cmp(&b.id));

    // ---- 分等级参数（GrantedEffectsPerLevel）----
    let raw_levels =
        read_json::<Vec<RawGrantedEffectPerLevel>>(&en.join("GrantedEffectsPerLevel.json"))?;
    let level_rows_total = raw_levels.len();
    let mut levels: BTreeMap<String, Vec<SkillLevelDef>> = BTreeMap::new();
    for raw in raw_levels {
        let Some(level) = raw.level.filter(|&l| l > 0).map(|l| l as u32) else {
            continue; // 等级 0 / 缺失 → 占位行，跳过
        };
        let Some(idx) = raw.granted_effect else {
            continue;
        };
        let Some(id) = effect_id_by_index
            .get(idx)
            .filter(|s| !s.is_empty())
            .cloned()
        else {
            continue;
        };
        levels.entry(id).or_default().push(SkillLevelDef {
            level,
            cooldown_ms: raw.cooldown.filter(|&c| c > 0).map(|c| c as u32),
            attack_time_ms: raw.attack_time.filter(|&t| t > 0).map(|t| t as u32),
            cost_amounts: raw
                .cost_amounts
                .into_iter()
                .map(|c| c.max(0) as u32)
                .collect(),
            attack_speed_multiplier: raw
                .attack_speed_multiplier
                .filter(|&m| m != 0)
                .map(|m| m as f64),
            base_multiplier: raw.base_multiplier.filter(|&m| (m - 1.0).abs() > 1e-9),
        });
    }
    // 每个效果的等级数组按 level 升序（diff 友好 + 查表确定）。
    for rows in levels.values_mut() {
        rows.sort_by_key(|r| r.level);
    }

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
        levels,
        zh_skill_names,
        gems_total,
        effects_total,
        level_rows_total,
    })
}

// ---- 分等级伤害 stat 集（GrantedEffectStatSets* 域）----

#[derive(Deserialize)]
struct RawStatId {
    #[serde(rename = "_index")]
    index: usize,
    #[serde(rename = "Id")]
    id: String,
}

#[derive(Deserialize)]
struct RawGrantedEffectStatSetLink {
    #[serde(rename = "Id")]
    id: String,
    /// `GrantedEffectStatSets` 行索引（负数/缺失 → 无 stat set）。
    #[serde(rename = "StatSet")]
    stat_set: Option<i64>,
}

#[derive(Deserialize)]
struct RawStatSet {
    #[serde(rename = "BaseEffectiveness")]
    base_effectiveness: Option<f64>,
    /// 等级无关常量 stat（`Stats` 行索引）与其值（位置配对；如 support `damage_+%_final`）。
    #[serde(rename = "ConstantStats", default)]
    constant_stats: Vec<usize>,
    #[serde(rename = "ConstantStatsValues", default)]
    constant_stats_values: Vec<i64>,
}

#[derive(Deserialize)]
struct RawStatSetPerLevel {
    /// `GrantedEffectStatSets` 行索引。
    #[serde(rename = "StatSet")]
    stat_set: Option<usize>,
    #[serde(rename = "GemLevel")]
    gem_level: Option<i64>,
    /// 每级浮动 stat（`Stats` 行索引）与其解析值（位置配对）。
    #[serde(rename = "FloatStats", default)]
    float_stats: Vec<usize>,
    #[serde(rename = "BaseResolvedValues", default)]
    base_resolved_values: Vec<i64>,
    /// 额外 stat（`Stats` 行索引）与其值（位置配对）。
    #[serde(rename = "AdditionalStats", default)]
    additional_stats: Vec<usize>,
    #[serde(rename = "AdditionalStatsValues", default)]
    additional_stats_values: Vec<i64>,
    /// 技能伤害倍率原始值（permyriad）；倍率 = `1 + BaseMultiplier/10000`。
    #[serde(rename = "BaseMultiplier")]
    base_multiplier: Option<i64>,
}

/// 分等级伤害 stat 集适配产物。
pub struct StatSetsBundle {
    /// 含至少一条伤害 stat 的授予效果，按 effect id 排序。
    pub sets: Vec<SkillStatSetDef>,
    /// `GrantedEffectStatSets` 总行数（用于汇报）。
    pub sets_total: usize,
    /// 入库的伤害分等级行总数。
    pub damage_levels_total: usize,
}

/// 是否为可注入计算的伤害相关 stat：flat 伤害值（min/max base/added、DoT per-minute）
/// **或**伤害缩放百分比（`damage_+%` / `<type>_damage_+%` 及其 `_final` more 变体）。
///
/// 入库判定从宽——由计算侧的 stat→ModName 映射决定能否落地，这样数据通用、升级映射
/// 无需重生成。覆盖 support 宝石的 `damage_+%[_final]` 倍率（解锁 P0-2）。仍排除抗性%、
/// buff 专用%、条件型（如 `..._to_exerted_attacks`，不以 `damage_+%[_final]` 结尾）。
fn is_mappable_stat(stat: &str) -> bool {
    // flat 伤害值
    (stat.contains("minimum") || stat.contains("maximum")) && stat.contains("_damage")
        || stat.ends_with("_damage_to_deal_per_minute")
        // 伤害缩放百分比（increased / more）
        || stat.ends_with("damage_+%")
        || stat.ends_with("damage_+%_final")
        // 技能自带转换 / gain-as-extra（如 grenade 物理→火）
        || stat.contains("_damage_%_to_convert_to_")
        || stat.contains("_damage_%_to_gain_as_")
}

/// 适配 `GrantedEffectStatSets` + `GrantedEffectStatSetsPerLevel`（+ `Stats` / `GrantedEffects`
/// 外键）为「effect id → 分等级伤害 stat」域。
///
/// 解析方式（对照 PoB2 `Export/Scripts/skills.lua` 的 statSets 处理）：
/// - `GrantedEffects.StatSet` 行索引 → `GrantedEffectStatSets` 行（取 `BaseEffectiveness`）；
/// - `GrantedEffectStatSetsPerLevel`（按 `StatSet` 行索引分组）每行的
///   `FloatStats[i]`↔`BaseResolvedValues[i]`、`AdditionalStats[i]`↔`AdditionalStatsValues[i]`
///   位置配对，stat 行索引经 `Stats` 解析为稳定 id；
/// - 过滤出伤害值 stat（[`is_damage_value_stat`]），按 effect id（= player 技能的 stat-set Id）入库。
///
/// 验证基准：FireballPlayer L1 → `spell_minimum/maximum_base_fire_damage` = 8 / 12
/// （与 PoB 自身 `Data/Skills/act_int.lua` 解析后逐字一致）。
pub fn adapt_stat_sets(en: &Path) -> Result<StatSetsBundle, String> {
    // stat 行索引 → 稳定 id（按 `_index` 落位，越界为空串）。
    let raw_stats = read_json::<Vec<RawStatId>>(&en.join("Stats.json"))?;
    let max_stat = raw_stats.iter().map(|r| r.index).max().map_or(0, |m| m + 1);
    let mut stat_id = vec![String::new(); max_stat];
    for r in &raw_stats {
        stat_id[r.index] = r.id.clone();
    }

    let sets = read_json::<Vec<RawStatSet>>(&en.join("GrantedEffectStatSets.json"))?;
    let sets_total = sets.len();
    let links = read_json::<Vec<RawGrantedEffectStatSetLink>>(&en.join("GrantedEffects.json"))?;

    // per-level 行按 StatSet 行索引分组。
    let per_level =
        read_json::<Vec<RawStatSetPerLevel>>(&en.join("GrantedEffectStatSetsPerLevel.json"))?;
    let mut rows_by_set: BTreeMap<usize, Vec<&RawStatSetPerLevel>> = BTreeMap::new();
    for row in &per_level {
        if let Some(si) = row.stat_set {
            rows_by_set.entry(si).or_default().push(row);
        }
    }

    let mut out = Vec::new();
    let mut damage_levels_total = 0usize;
    for link in &links {
        if link.id.is_empty() {
            continue;
        }
        let Some(si) = link.stat_set.filter(|&i| i >= 0).map(|i| i as usize) else {
            continue;
        };
        let Some(set) = sets.get(si) else { continue };

        // 等级无关常量 stat（如 support `damage_+%_final` 倍率）。
        let mut constant_stats = Vec::new();
        for (&stat_idx, &value) in set
            .constant_stats
            .iter()
            .zip(set.constant_stats_values.iter())
        {
            if let Some(sid) = stat_id.get(stat_idx).filter(|s| !s.is_empty())
                && is_mappable_stat(sid)
            {
                constant_stats.push(SkillDamageStat {
                    stat: sid.clone(),
                    value: value as f64,
                });
            }
        }

        let rows = rows_by_set.get(&si).map(Vec::as_slice).unwrap_or(&[]);

        let mut levels = Vec::new();
        for row in rows {
            let Some(gem_level) = row.gem_level.filter(|&l| l > 0).map(|l| l as u32) else {
                continue;
            };
            let pairs = row
                .float_stats
                .iter()
                .zip(row.base_resolved_values.iter())
                .chain(
                    row.additional_stats
                        .iter()
                        .zip(row.additional_stats_values.iter()),
                );
            let mut stats = Vec::new();
            for (&stat_idx, &value) in pairs {
                let Some(sid) = stat_id.get(stat_idx).filter(|s| !s.is_empty()) else {
                    continue;
                };
                if is_mappable_stat(sid) {
                    stats.push(SkillDamageStat {
                        stat: sid.clone(),
                        value: value as f64,
                    });
                }
            }
            // 伤害倍率 = 1 + BaseMultiplier/10000（攻击技能武器伤害倍率，如 grenade 7.57）。
            let damage_multiplier =
                1.0 + f64::from(row.base_multiplier.unwrap_or(0) as i32) / 10000.0;
            // 收录有伤害 stat 或有非平凡倍率的等级行。
            if !stats.is_empty() || (damage_multiplier - 1.0).abs() > f64::EPSILON {
                levels.push(SkillStatSetLevel {
                    gem_level,
                    damage_multiplier,
                    stats,
                });
            }
        }
        if levels.is_empty() && constant_stats.is_empty() {
            continue;
        }
        levels.sort_by_key(|l| l.gem_level);
        damage_levels_total += levels.len();
        out.push(SkillStatSetDef {
            id: link.id.clone(),
            base_effectiveness: set.base_effectiveness.unwrap_or(0.0),
            constant_stats,
            levels,
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(StatSetsBundle {
        sets: out,
        sets_total,
        damage_levels_total,
    })
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

/// 把 base item 行建成 `_index -> (Id, Name)` 查表（`_index` 连续，越界返回 None）。
fn id_lookup_from_base(rows: &[RawBaseItemId]) -> Vec<(String, String)> {
    let max = rows.iter().map(|r| r.index).max().map_or(0, |m| m + 1);
    let mut table = vec![(String::new(), String::new()); max];
    for r in rows {
        table[r.index] = (r.id.clone(), r.name.clone());
    }
    table
}
