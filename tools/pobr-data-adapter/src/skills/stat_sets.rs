//! 分等级伤害 stat 集适配（`GrantedEffectStatSets` + `GrantedEffectStatSetsPerLevel`
//! → `granted_effect_stat_sets.json`）。

use std::collections::BTreeMap;
use std::path::Path;

use pobr_data::catalog::{SkillDamageStat, SkillStatSetDef, SkillStatSetLevel};
use serde::Deserialize;

use crate::read_json;

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

/// （M1-T4，蓝图 §1.2）从 `GrantedEffectStatSetsPerLevel` 直读技能基础暴击率：
/// `effect id → gem level → 暴击率（百分点）`，供 [`super::levels::adapt_levels`] 落
/// `SkillLevelDef::crit_chance`（替代 `overlay/skill_overrides.json` 的 crit merge 来源）。
///
/// 列名错位说明（社区 schema vs vendor `Export/spec.lua`，对照表见 `pipeline/README.md`）：
/// - 社区 `SpellCritChance` = vendor `AttackCritChance`（主列，`/100`）；
/// - 社区 `AttackCritChance` = vendor `OffhandCritChance`（≠0 时**覆盖**主列，
///   vendor `Export/Scripts/skills.lua:281-286`）。
///
/// statSet 归属（对齐 vendor 按 `GrantedEffect` join 的行为）：主 `StatSet` 优先；主 set
/// 全程无暴击而某**附加** set（`AdditionalStatSets`，FK → `GrantedEffectStatSets`）有 →
/// 取第一个有暴击的附加 set（如 GalvanicFieldBuffPlayer 主 set 164 无暴击、附加 set 900
/// 有 9.0——W0 对拍 201/201 与 vendor 一致的规则）。
///
/// 独立函数边界（蓝图 §3.2）：T4 在本文件（T5 owner）内仅新增此函数，不动既有逻辑；
/// T5 多 statSet 改造时对齐调用点。
pub(super) fn crit_from_statset_levels(
    en: &Path,
) -> Result<BTreeMap<String, BTreeMap<u32, f64>>, String> {
    /// 暴击两列的最小行读取（与 [`RawStatSetPerLevel`] 分开：本函数只关心暴击列）。
    #[derive(Deserialize)]
    struct RawCritRow {
        #[serde(rename = "StatSet")]
        stat_set: Option<usize>,
        #[serde(rename = "GemLevel")]
        gem_level: Option<i64>,
        /// 社区列名（= vendor `AttackCritChance` 主列），1/100 百分点（如 900 = 9%）。
        #[serde(rename = "SpellCritChance")]
        spell_crit_chance: Option<f64>,
        /// 社区列名（= vendor `OffhandCritChance` 覆盖列）。
        #[serde(rename = "AttackCritChance")]
        attack_crit_chance: Option<f64>,
    }
    /// `GrantedEffects` 行的 statSet 归属三列。
    #[derive(Deserialize)]
    struct RawEffectSetLink {
        #[serde(rename = "Id")]
        id: String,
        #[serde(rename = "StatSet")]
        stat_set: Option<i64>,
        #[serde(rename = "AdditionalStatSets", default)]
        additional_stat_sets: Vec<i64>,
    }

    let rows = read_json::<Vec<RawCritRow>>(&en.join("GrantedEffectStatSetsPerLevel.json"))?;
    // set 行索引 → {gem level → 暴击率}（文件序内同 level 后写覆盖，行内 Offhand 覆盖主列）。
    let mut crit_by_set: BTreeMap<usize, BTreeMap<u32, f64>> = BTreeMap::new();
    for row in &rows {
        let (Some(si), Some(level)) = (row.stat_set, row.gem_level.filter(|&l| l > 0)) else {
            continue;
        };
        let spell = row.spell_crit_chance.unwrap_or(0.0);
        let offhand = row.attack_crit_chance.unwrap_or(0.0);
        let value = if offhand != 0.0 {
            Some(offhand / 100.0)
        } else if spell != 0.0 {
            Some(spell / 100.0)
        } else {
            None
        };
        if let Some(v) = value {
            crit_by_set.entry(si).or_default().insert(level as u32, v);
        }
    }

    let links = read_json::<Vec<RawEffectSetLink>>(&en.join("GrantedEffects.json"))?;
    let mut out: BTreeMap<String, BTreeMap<u32, f64>> = BTreeMap::new();
    for link in links {
        if link.id.is_empty() {
            continue;
        }
        // 候选 set：主 StatSet 在前、AdditionalStatSets 按列序在后；取第一个有暴击值的。
        let candidates = link
            .stat_set
            .into_iter()
            .chain(link.additional_stat_sets)
            .filter(|&i| i >= 0)
            .map(|i| i as usize);
        for si in candidates {
            if let Some(map) = crit_by_set.get(&si).filter(|m| !m.is_empty()) {
                out.insert(link.id, map.clone());
                break;
            }
        }
    }
    Ok(out)
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
/// - 过滤出伤害值 stat（[`is_mappable_stat`]），按 effect id（= player 技能的 stat-set Id）入库。
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
