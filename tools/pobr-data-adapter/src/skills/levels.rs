//! 分等级参数表适配（`GrantedEffectsPerLevel` → `granted_effect_levels.json`）。
//!
//! `GrantedEffectsPerLevel.GrantedEffect` 整型 `_index` → `GrantedEffects.Id`
//! （查表由 [`super::effects`] 产出）；已接入分等级 cost / cooldown / attack time
//! 以及cost 倍率 / Spirit 保留族 / 储存次数 / 攻速乘数 / 暴击率表列直读。
//!
//! 列值换算（对照 vendor `Export/Scripts/skills.lua:226-295`；
//! 社区 schema 与 vendor spec 的列名错位见 `pipeline/README.md` 对照表）：
//!
//! | 入库字段 | 社区列名 | 换算 |
//! |---|---|---|
//! | `mana_multiplier` | `CostMultiplier` | `- 100`（== 100 → None） |
//! | `spirit_reservation_flat` | `Reservation`（= vendor `SpiritReservation`） | 原值（== 0 → None） |
//! | `reservation_multiplier` | `EffectOnPlayer`（= vendor `ReservationMultiplier`） | `- 100`（== 100 → None） |
//! | `stored_uses` | `StoredUses` | 原值（== 0 → None） |
//! | `attack_speed_multiplier` | `AttackSpeedMultiplier` | 原值（== 0 → None） |
//! | `crit_chance` | stat-set 表两暴击列（见 [`super::stat_sets::crit_from_statset_levels`]） | `/100`，Offhand 覆盖 |
//! | `level_requirement` | **无 `.dat` 列**（真源 `ItemExperiencePerLevel` 不可下载） | 恒 None，经 extract-lua 落库 |

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
    /// 攻击速度乘数（百分点，可负；vendor `attackSpeedMultiplier`，如 Flicker -50）。
    /// 起表列直读（T4.3 已对 overlay 历史值 3578 条逐值一致验收）。
    #[serde(rename = "AttackSpeedMultiplier")]
    attack_speed_multiplier: Option<f64>,
    /// 伤害基础倍率（PoB `baseMultiplier`，stat-set BaseMultiplier 缺失时的回退源）。
    /// 注：该列不在 `GrantedEffectsPerLevel` 社区表中（恒缺失 → None），分等级值仍由
    /// `overlay/skill_overrides.json` merge 提供（真源 = stat-set 表 `BaseMultiplier`，
    /// 其表列直读归 T5 多 statSet 改造）。
    #[serde(rename = "BaseMultiplier")]
    base_multiplier: Option<f64>,
    /// cost 倍率原始值（vendor `CostMultiplier`，100 = 无倍率）。
    #[serde(rename = "CostMultiplier")]
    cost_multiplier: Option<f64>,
    /// Spirit 扁平保留量（社区列名 `Reservation` = vendor `SpiritReservation`）。
    #[serde(rename = "Reservation")]
    spirit_reservation: Option<f64>,
    /// 保留倍率原始值（社区列名 `EffectOnPlayer` = vendor `ReservationMultiplier`，
    /// 100 = 无倍率）。
    #[serde(rename = "EffectOnPlayer")]
    reservation_multiplier: Option<f64>,
    /// 可储存使用次数（vendor `StoredUses`，0 = 无储存）。
    #[serde(rename = "StoredUses")]
    stored_uses: Option<i64>,
}

/// 适配 `GrantedEffectsPerLevel` 为 `granted_effect_id -> 升序等级数组`
/// （返回 `(查表, 原始总行数)`）。
///
/// `crit_by_effect`：stat-set 表直读的暴击查表（effect id → gem level → 暴击百分点），
/// 由 [`super::stat_sets::crit_from_statset_levels`] 产出——暴击在 `.dat` 中挂在
/// stat-set 维度，与本表按 (effect, level) join。
pub(super) fn adapt_levels(
    en: &Path,
    effect_id_by_index: &[String],
    crit_by_effect: &BTreeMap<String, BTreeMap<u32, f64>>,
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
        // 平凡值归一化对齐 vendor 导出守卫（`Export/Scripts/skills.lua`）：
        // CostMultiplier == 100 / Reservation == 0 / EffectOnPlayer == 100 /
        // StoredUses == 0 / AttackSpeedMultiplier == 0 时 vendor 省略字段 → None。
        let crit_chance = crit_by_effect.get(&id).and_then(|m| m.get(&level)).copied();
        levels.entry(id).or_default().push(SkillLevelDef {
            level,
            cooldown_ms: raw.cooldown.filter(|&c| c > 0).map(|c| c as u32),
            attack_time_ms: raw.attack_time.filter(|&t| t > 0).map(|t| t as u32),
            cost_amounts: raw
                .cost_amounts
                .into_iter()
                .map(|c| c.max(0) as u32)
                .collect(),
            attack_speed_multiplier: raw.attack_speed_multiplier.filter(|&m| m != 0.0),
            base_multiplier: raw.base_multiplier.filter(|&m| (m - 1.0).abs() > 1e-9),
            crit_chance,
            mana_multiplier: raw
                .cost_multiplier
                .filter(|&c| c != 100.0)
                .map(|c| c - 100.0),
            spirit_reservation_flat: raw.spirit_reservation.filter(|&v| v != 0.0),
            reservation_multiplier: raw
                .reservation_multiplier
                .filter(|&v| v != 100.0)
                .map(|v| v - 100.0),
            stored_uses: raw.stored_uses.filter(|&v| v != 0).map(|v| v.max(0) as u32),
            // PoE2 `.dat` 无 PlayerLevelReq 列（W0 核验）；经 extract-lua 落库。
            level_requirement: None,
        });
    }
    // 每个效果的等级数组按 level 升序（diff 友好 + 查表确定）。
    for rows in levels.values_mut() {
        rows.sort_by_key(|r| r.level);
    }
    Ok((levels, level_rows_total))
}
