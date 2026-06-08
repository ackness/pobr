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
    /// 技能基础暴击率（PoB `critChance`，百分点；如 Comet 13）。法系/攻击技能固有暴击来源。
    #[serde(rename = "CritChance")]
    crit_chance: Option<f64>,
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
            // 暴击率原样保留（含 0=无暴击，与「缺失」区分）；分等级值由 PoB 抽取合并。
            crit_chance: raw.crit_chance,
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

/// 是否为有计算意义、值得忠实入库的机制 stat——即所有「会影响伤害 / 暴击 / 穿透 /
/// 命中 / 速度 / added flat 伤害」的 stat。
///
/// **入库从宽，落地由计算侧决定**：本函数只决定数据层保留哪些 stat；能否变成 modifier
/// 由 `pobr-build::skill_stat_map::map_skill_stat` 决定（映射不到的 stat 静默跳过，无害）。
/// 因此宁可多保留：旧版只留伤害 stat，导致只含暴击 stat 的 support stat-set（如 Pinpoint
/// Critical 的 `support_pinpoint_critical_strike_*_+%_final`）被 `adapt_stat_sets` 末尾的
/// 「无 stat → 丢整个 set」整段丢弃，进攻 parity 大面积塌陷。
///
/// 保留（按语义后缀，不按具体技能 id）：
/// - flat 伤害值：min/max base/added 伤害、DoT per-minute；
/// - 伤害缩放：`damage_+%` / `..._final`、转换 / gain-as-extra；
/// - 暴击：`critical_strike_chance_+%[_final]` / `critical_strike_multiplier_+%[_final]` /
///   `critical_*damage_+%[_final]`；
/// - 穿透 / 降敌抗：`penetrat*` / `resistance_%`（如 exposure/negate 类 support）；
/// - added flat 伤害 buff：`*added_*_damage`（含 `buff_grant_%_added_<type>_attack_damage`）。
///
/// 仍排除：纯显示 / 持续时间 / 范围显示等与伤害无关的 stat。
fn is_mappable_stat(stat: &str) -> bool {
    // flat 伤害值（min/max base/added）+ DoT per-minute
    (stat.contains("minimum") || stat.contains("maximum")) && stat.contains("_damage")
        || stat.ends_with("_damage_to_deal_per_minute")
        // 伤害缩放百分比（increased / more）
        || stat.ends_with("damage_+%")
        || stat.ends_with("damage_+%_final")
        // 技能自带转换 / gain-as-extra（如 grenade 物理→火）
        || stat.contains("_damage_%_to_convert_to_")
        || stat.contains("_damage_%_to_gain_as_")
        // 暴击率 / 爆伤缩放（含 _final more 变体）——解锁 Pinpoint Critical 等 support set。
        || stat.contains("critical_strike_chance_+%")
        || stat.contains("critical_strike_multiplier_+%")
        || stat.contains("critical") && stat.contains("damage_+%")
        // 穿透 / 降敌抗（exposure / penetration / negate 类 support）。
        || stat.contains("penetrat")
        || stat.ends_with("resistance_%")
        // added flat 伤害 buff（如 Ice Bite 的 buff_grant_%_added_cold_attack_damage）。
        || stat.contains("added") && stat.contains("_damage")
        // 光环 / buff 授予的**防御** stat（Discipline ES、Purity 抗性、Defiance 护甲/闪避…）。
        // 这些以 `base_skill_buff_*_to_apply` / `_to_grant` 命名，由 [`crate::skill_stat_map`]
        // 的 aura buff 映射消费。入库从宽：能否落地由计算侧的映射决定（映射不到静默跳过）。
        || stat.starts_with("base_skill_buff_")
        // 附加施放/攻击时间常量（`total_cast_time_+_ms` / `total_attack_time_+_ms`，毫秒）：
        // 作为加法项计入出手时间分母（如 Comet +1000ms = +1.0s），由 SkillStatMap 映射为
        // `TotalCastTime`/`TotalAttackTime` BASE。这类常量来自 statSet constantStats。
        || stat == "total_cast_time_+_ms"
        || stat == "total_attack_time_+_ms"
        // 出手速度族（攻速 / 施法速度 / 技能速度，含 `_final` more 变体）——解锁 Rapid Attacks
        // （`attack_speed_+%`）、Rapid Casting（`base_cast_speed_+%`）等整组缺失的 support
        // stat-set（旧版只留伤害 stat，整个 set 被「无 stat → 丢」整段丢弃，攻击 build 攻速塌陷）。
        // 这三族进入 PoB 的「Speed」加法/连乘乘区（AttackSpeed/CastSpeed/SkillSpeed），由
        // `pobr-build::skill_stat_map` 按后缀语义落地（INC / `_final`→MORE）。movement/projectile/
        // reload/knockback/cooldown 等**非出手速率**的 speed stat 不在此匹配（与面板 DPS 无关）。
        || is_skill_speed_stat(stat)
        // 距离 ramp more 伤害（`*_damage_+%_final_from_distance`，如 Close Combat / Far Combat）：
        // PoB2 `mod("Damage","MORE", DistanceRamp)`，面板按配置距离取 ramp 系数。入库保留常量
        // ramp 上限值，由 calc 侧按 ramp 应用（见 `skill_stat_map::map_distance_ramp`）。
        || stat.ends_with("_damage_+%_final_from_distance")
}

/// 是否为**出手速率**（攻速 / 施法速度 / 技能速度）类 stat——即进入 PoB「Speed」乘区、
/// 影响每秒出手次数的速度 stat。匹配 `*attack_speed_+%[_final]` / `*cast_speed_+%[_final]` /
/// `*skill_speed_+%[_final]`。
///
/// **刻意排除**与面板出手速率无关的同形 speed stat：`movement_speed`（位移）、
/// `projectile_speed`（弹道飞行速度）、`reload_speed`（换弹）、`knockback_speed`、
/// `cooldown_speed`（冷却恢复）——这些都含 `speed_+%` 但不属攻/施/技能速度乘区。
fn is_skill_speed_stat(stat: &str) -> bool {
    let base = stat.strip_suffix("_final").unwrap_or(stat);
    let Some(core) = base.strip_suffix("_+%") else {
        return false;
    };
    core.ends_with("attack_speed") || core.ends_with("cast_speed") || core.ends_with("skill_speed")
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
            // statSet baseMods（如 Flicker `Speed MORE 285`）不在 GGG `.dat` 表中，是 PoB2 自带常量，
            // 由 vendor Lua 合并入 JSON（同 crit_chance 先例），适配阶段留空。
            skill_attack_speed_more: None,
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

#[cfg(test)]
mod tests {
    use super::is_mappable_stat;

    #[test]
    fn keeps_flat_and_percent_damage_stats() {
        assert!(is_mappable_stat("spell_minimum_base_fire_damage"));
        assert!(is_mappable_stat("attack_maximum_added_cold_damage"));
        assert!(is_mappable_stat("damage_+%"));
        assert!(is_mappable_stat("fire_damage_+%_final"));
        assert!(is_mappable_stat("base_chaos_damage_to_deal_per_minute"));
        assert!(is_mappable_stat(
            "active_skill_base_physical_damage_%_to_convert_to_fire"
        ));
        assert!(is_mappable_stat(
            "support_added_fire_damage_%_to_gain_as_cold"
        ));
    }

    #[test]
    fn keeps_critical_strike_stats() {
        // Pinpoint Critical 的两条 constantStats——旧版被过滤，整个 set 被丢。
        assert!(is_mappable_stat(
            "support_pinpoint_critical_strike_chance_+%_final"
        ));
        assert!(is_mappable_stat(
            "support_pinpoint_critical_strike_multiplier_+%_final"
        ));
        assert!(is_mappable_stat("critical_strike_chance_+%"));
        assert!(is_mappable_stat("local_critical_strike_multiplier_+%"));
        assert!(is_mappable_stat("critical_strike_damage_+%_final"));
    }

    #[test]
    fn keeps_penetration_and_resistance_stats() {
        assert!(is_mappable_stat(
            "base_fire_damage_resistance_penetration_%"
        ));
        assert!(is_mappable_stat("elemental_damage_penetration_%"));
        // 降敌抗 exposure 类（resistance_% 后缀）。
        assert!(is_mappable_stat("base_fire_damage_resistance_%"));
    }

    #[test]
    fn keeps_added_flat_buff_damage() {
        // Ice Bite 等 added flat buff（数据层忠实保留；条件应用由计算侧决定）。
        assert!(is_mappable_stat(
            "support_ice_bite_buff_grant_%_added_cold_attack_damage"
        ));
    }

    #[test]
    fn keeps_skill_speed_stats() {
        // Rapid Attacks（attack_speed_+%）/ Rapid Casting（base_cast_speed_+%）整组缺失的祸首。
        assert!(is_mappable_stat("attack_speed_+%"));
        assert!(is_mappable_stat("base_cast_speed_+%"));
        // `_final` more 变体（active_skill_attack_speed_+%_final = mod("Speed","MORE",Attack)）。
        assert!(is_mappable_stat("active_skill_attack_speed_+%_final"));
        assert!(is_mappable_stat(
            "support_additional_fissures_skill_speed_+%_final"
        ));
        // 前缀型（具体 support 前缀，按后缀语义保留）。
        assert!(is_mappable_stat("totem_skill_attack_speed_+%"));
        // 条件后缀变体（`..._while_not_at_maximum_rage`）不以 `<族>_speed_+%[_final]` 结尾，
        // 与 calc 侧映射保持一致——不保留（即便保留计算侧也会保守跳过，入库无益）。
        assert!(!is_mappable_stat(
            "support_rage_attack_speed_+%_while_not_at_maximum_rage"
        ));
    }

    #[test]
    fn keeps_distance_ramp_more_damage() {
        // Close Combat / Far Combat（`*_damage_+%_final_from_distance` = mod("Damage","MORE",ramp)）。
        assert!(is_mappable_stat(
            "support_close_combat_attack_damage_+%_final_from_distance"
        ));
        assert!(is_mappable_stat(
            "support_far_combat_attack_damage_+%_final_from_distance"
        ));
    }

    #[test]
    fn rejects_non_combat_stats() {
        assert!(!is_mappable_stat("base_skill_area_of_effect_+%"));
        assert!(!is_mappable_stat("support_ice_bite_base_buff_duration"));
        assert!(!is_mappable_stat("number_of_additional_projectiles"));
        // 非出手速率的同形 speed stat——不属攻/施/技能速度乘区，不入库。
        assert!(!is_mappable_stat(
            "movement_speed_+%_final_while_performing_action"
        ));
        assert!(!is_mappable_stat("active_skill_projectile_speed_+%_final"));
        assert!(!is_mappable_stat("active_skill_reload_speed_+%_final"));
        assert!(!is_mappable_stat("base_knockback_speed_+%"));
        assert!(!is_mappable_stat("base_cooldown_speed_+%_final"));
    }
}
