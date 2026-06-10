//! 分等级参数表适配（`GrantedEffectsPerLevel` → `granted_effect_levels.json`）。
//!
//! `GrantedEffectsPerLevel.GrantedEffect` 整型 `_index` → `GrantedEffects.Id`
//! （查表由 [`super::effects`] 产出）；已接入分等级 cost / cooldown / attack time。

use std::collections::BTreeMap;
use std::path::Path;

use pobr_data::catalog::SkillLevelDef;
use serde::Deserialize;

use crate::read_json;

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
    // 注（M1-W0）：`AttackSpeedMultiplier` 列已随扩列下载（4256 行非零），但「表列直读
    // 替代 skill_overrides merge 来源」是 T4.1/T4.3 的行为改动（需 3578 历史值逐值一致
    // 验收 + overlay 边车收窄同步）；W0 刻意**不消费**该列，保持 base 为与既往逐字节
    // 一致的纯 adapter 产物（attack_speed_multiplier 仍由 overlay merge 提供）。
    /// 伤害基础倍率（PoB `baseMultiplier`，stat-set BaseMultiplier 缺失时的回退源）。
    #[serde(rename = "BaseMultiplier")]
    base_multiplier: Option<f64>,
    /// 技能基础暴击率（PoB `critChance`，百分点；如 Comet 13）。法系/攻击技能固有暴击来源。
    #[serde(rename = "CritChance")]
    crit_chance: Option<f64>,
}

/// 适配 `GrantedEffectsPerLevel` 为 `granted_effect_id -> 升序等级数组`
/// （返回 `(查表, 原始总行数)`）。
pub(super) fn adapt_levels(
    en: &Path,
    effect_id_by_index: &[String],
) -> Result<(BTreeMap<String, Vec<SkillLevelDef>>, usize), String> {
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
            // 见上方 RawGrantedEffectPerLevel 注：T4 前不消费表列，值由 overlay merge 提供。
            attack_speed_multiplier: None,
            base_multiplier: raw.base_multiplier.filter(|&m| (m - 1.0).abs() > 1e-9),
            // 暴击率原样保留（含 0=无暴击，与「缺失」区分）；分等级值由 PoB 抽取合并。
            crit_chance: raw.crit_chance,
        });
    }
    // 每个效果的等级数组按 level 升序（diff 友好 + 查表确定）。
    for rows in levels.values_mut() {
        rows.sort_by_key(|r| r.level);
    }
    Ok((levels, level_rows_total))
}
