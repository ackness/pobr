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

/// 适配 `GrantedEffectStatSets` + `GrantedEffectStatSetsPerLevel`（+ `Stats` / `GrantedEffects`
/// 外键）为「effect id → 分等级 stat」域。
///
/// 解析方式（对照 PoB2 `Export/Scripts/skills.lua` 的 statSets 处理）：
/// - `GrantedEffects.StatSet` 行索引 → `GrantedEffectStatSets` 行（取 `BaseEffectiveness`）；
/// - `GrantedEffectStatSetsPerLevel`（按 `StatSet` 行索引分组）每行的
///   `FloatStats[i]`↔`BaseResolvedValues[i]`、`AdditionalStats[i]`↔`AdditionalStatsValues[i]`
///   位置配对，stat 行索引经 `Stats` 解析为稳定 id。
///
/// **全量 stat 入库（M1-T5.3，蓝图 15-G2 修复方向）**：数据层不再施加任何 stat 白名单
/// （原 `is_mappable_stat` 后缀启发式已删除）——statmap 数据引擎（`pobr-core::rules::
/// stat_map_engine`，T2）需要看到全部 stat 才能穷举对照。**搬迁不变式保障**：同一谓词
/// 已平移到消费侧 legacy 路径（`pobr-build::legacy_stat_filter`，在 `mapped_stat_modifiers`
/// 的 Legacy 通道入口过滤），保证 ninja parity 逐值不变；该消费侧过滤随 T2.4 删 legacy
/// 一起删除。
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

        // 等级无关常量 stat（如 support `damage_+%_final` 倍率）。全量入库，不过滤。
        let mut constant_stats = Vec::new();
        for (&stat_idx, &value) in set
            .constant_stats
            .iter()
            .zip(set.constant_stats_values.iter())
        {
            if let Some(sid) = stat_id.get(stat_idx).filter(|s| !s.is_empty()) {
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
                stats.push(SkillDamageStat {
                    stat: sid.clone(),
                    value: value as f64,
                });
            }
            // 伤害倍率 = 1 + BaseMultiplier/10000（攻击技能武器伤害倍率，如 grenade 7.57）。
            let damage_multiplier =
                1.0 + f64::from(row.base_multiplier.unwrap_or(0) as i32) / 10000.0;
            // 收录有 stat 或有非平凡倍率的等级行。
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
