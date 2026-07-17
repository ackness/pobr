//! 怪物百级缩放表 schema（`base/monster_scaling.json`）。
//!
//! 对应 PoB2 `src/Data/Misc.lua` 的九张 per-level 怪物表（每张 100 项，
//! 索引 = 怪物等级 - 1，源头是 GGG `DefaultMonsterStats.dat`，adapter 未来
//! 可直接从 `.dat` 再生同形 JSON）：
//!
//! | JSON 字段 | PoB2 Lua 表（Misc.lua 行号） | pobr 准源（迁出前） |
//! |---|---|---|
//! | `accuracy` | `data.monsterAccuracyTable`（L6） | `monster.rs::MONSTER_ACCURACY_TABLE` |
//! | `evasion` | `data.monsterEvasionTable`（L5） | `monster.rs::MONSTER_EVASION_TABLE` |
//! | `armour` | `data.monsterArmourTable`（L11） | `monster.rs::MONSTER_ARMOUR_TABLE` |
//! | `life` | `data.monsterLifeTable`（L7） | `monster.rs::MONSTER_LIFE_TABLE` |
//! | `ally_life` | `data.monsterAllyLifeTable`（L8） | 无（vendor-only） |
//! | `damage` | `data.monsterDamageTable`（L9） | `monster.rs::MONSTER_DAMAGE_TABLE` |
//! | `ally_damage` | `data.monsterAllyDamageTable`（L10） | 无（vendor-only） |
//! | `ailment_threshold` | `data.monsterAilmentThresholdTable`（L12） | `monster.rs::MONSTER_AILMENT_THRESHOLD_TABLE` |
//! | `poise_threshold` | `data.monsterPoiseThresholdTable`（L13） | `monster.rs::MONSTER_POISE_THRESHOLD_TABLE` |
//!
//! 数值口径：
//! - 有 pobr Rust 准源的七张表，JSON 与 Rust 表逐值相等（搬迁不变式）；
//!   其中 `damage` 沿用 pobr 既有口径——vendor f32 噪声值（如 `9.1599998474121`）
//!   round 到 2 位小数（`9.16`），与 vendor 在 2 位精度下逐值一致。
//! - `ally_life` / `ally_damage` 为 vendor-only 字段（pobr 此前未迁），自
//!   `vendor/PathOfBuilding-PoE2/src/Data/Misc.lua` L8 / L10 抽取；
//!   `ally_damage` 同样 round 到 2 位小数（与 `damage` 口径一致，也与 PoB2
//!   `CalcActiveSkill.lua:907` hiddenDamageFixup 派生中 `round(..., 2)` 的
//!   精度处理同构）。
//!
//! 消费方（PoB2 侧用法，pobr 对应 `setup_env` / EHP / minion 装配）：
//! - `accuracy`/`evasion`/`armour`/`life`：`CalcSetup.lua` 注入 enemy ModDB 的
//!   BASE 值；`damage`：EHP 计算 `monsterDamageTable[lv] * 1.5 * DPSMult`。
//! - `ailment_threshold`：几率派生型异常（点燃/感电/冰缓最小阈值），
//!   `CalcOffence.lua` `enemyThreshold = 表值 × mod(EnemyAilmentThreshold)`。
//! - `poise_threshold`：积累型 debuff（冰冻/电击/重眩晕/钉刺），Boss 档位的
//!   `PoiseThreshold MORE 500` 在 mod_db 层另行注入，表值为裸值。
//! - `ally_life`/`ally_damage`：召唤物（非敌对 minion）的生命/伤害基线
//!   （`CalcActiveSkill.lua:899-908`），并作为 hiddenDamageFixup 的派生输入：
//!   `hiddenDamageFixup = round(allyDamage[lv] / damageTable[lv] × SpectreBeastDamageFixup, 2) - 1`
//!   （`SpectreBeastDamageFixup = 1.25` 属 misc 常量，归 `game_constants` 域）。

use serde::{Deserialize, Serialize};

use crate::monster::{
    MONSTER_ACCURACY_TABLE, MONSTER_AILMENT_THRESHOLD_TABLE, MONSTER_ARMOUR_TABLE,
    MONSTER_DAMAGE_TABLE, MONSTER_EVASION_TABLE, MONSTER_LIFE_TABLE, MONSTER_POISE_THRESHOLD_TABLE,
};

/// 怪物百级表长度（等级 1..=100，各数组恒为 100 项）。
pub const MONSTER_SCALING_TABLE_LEN: usize = 100;

/// 怪物百级缩放表（九张并列 per-level 数组，索引 = 等级 - 1）。
///
/// 并列数组形与 vendor `Misc.lua` / `DefaultMonsterStats.dat` 同构，
/// adapter 再生时逐表照搬即可；各数组长度恒为
/// [`MONSTER_SCALING_TABLE_LEN`]，由 loader 侧测试约束。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MonsterScalingTable {
    /// 怪物基础精准（`data.monsterAccuracyTable`）。
    pub accuracy: Vec<u32>,
    /// 怪物基础闪避（`data.monsterEvasionTable`）。
    pub evasion: Vec<u32>,
    /// 怪物基础护甲（`data.monsterArmourTable`）。
    pub armour: Vec<u32>,
    /// 怪物基础生命（`data.monsterLifeTable`）。
    pub life: Vec<u32>,
    /// 友方（召唤物）基础生命（`data.monsterAllyLifeTable`，vendor-only）。
    pub ally_life: Vec<u32>,
    /// 怪物基础伤害（`data.monsterDamageTable`，2 位小数口径）。
    pub damage: Vec<f64>,
    /// 友方（召唤物）基础伤害（`data.monsterAllyDamageTable`，vendor-only，
    /// 2 位小数口径；hiddenDamageFixup 派生输入）。
    pub ally_damage: Vec<f64>,
    /// 怪物异常阈值（`data.monsterAilmentThresholdTable`，点燃/感电/冰缓）。
    pub ailment_threshold: Vec<u32>,
    /// 怪物姿态阈值（`data.monsterPoiseThresholdTable`，冰冻/电击/重眩晕/钉刺）。
    pub poise_threshold: Vec<u32>,
}

/// 通用等级查表（u32 表）：等级 clamp 到 `[1, 表长]`，索引 = 等级 - 1；
/// 空表返回 0（loader 侧测试约束各表恒 100 项，正常路径不会触发）。
/// 与 `monster.rs::level_to_index` 的 clamp 语义逐值一致（搬迁不变式）。
fn lookup_u32(table: &[u32], level: u32) -> u32 {
    if table.is_empty() {
        return 0;
    }
    table[(level.clamp(1, table.len() as u32) - 1) as usize]
}

/// 通用等级查表（f64 表）：语义同 [`lookup_u32`]。
fn lookup_f64(table: &[f64], level: u32) -> f64 {
    if table.is_empty() {
        return 0.0;
    }
    table[(level.clamp(1, table.len() as u32) - 1) as usize]
}

impl MonsterScalingTable {
    /// 查询怪物基础精准（等级 1..=100，超界 clamp；对应 `monster_accuracy`）。
    pub fn accuracy_at(&self, level: u32) -> u32 {
        lookup_u32(&self.accuracy, level)
    }

    /// 查询怪物基础闪避（对应 `monster_evasion`）。
    pub fn evasion_at(&self, level: u32) -> u32 {
        lookup_u32(&self.evasion, level)
    }

    /// 查询怪物基础护甲（对应 `monster_armour`）。
    pub fn armour_at(&self, level: u32) -> u32 {
        lookup_u32(&self.armour, level)
    }

    /// 查询怪物基础生命（对应 `monster_life`）。
    pub fn life_at(&self, level: u32) -> u32 {
        lookup_u32(&self.life, level)
    }

    /// 查询怪物基础伤害（EHP 用；对应 `monster_damage`）。
    pub fn damage_at(&self, level: u32) -> f64 {
        lookup_f64(&self.damage, level)
    }

    /// 查询怪物异常阈值（裸表值；对应 `enemy_ailment_threshold`）。
    pub fn ailment_threshold_at(&self, level: u32) -> u32 {
        lookup_u32(&self.ailment_threshold, level)
    }

    /// 查询怪物姿态阈值（裸表值；对应 `enemy_poise_threshold`）。
    pub fn poise_threshold_at(&self, level: u32) -> u32 {
        lookup_u32(&self.poise_threshold, level)
    }
}

/// fallback（无 GameData 注入时使用）：与 `base/monster_scaling.json` **逐值相等**
/// （搬迁不变式，W2 测试已锁 JSON == Rust 准源）。
///
/// - 有 pobr Rust 准源的八张表（含 #12 升格的 `ally_life` =
///   `MONSTER_ALLY_LIFE_TABLE`，消费方 = 召唤物基础生命派生）直接引用
///   `crate::monster` 既有 const（零字面量复制）；
/// - `ally_damage` 为 vendor-only（calc 暂无消费方），字面量抄录自
///   `vendor/PathOfBuilding-PoE2/src/Data/Misc.lua` L10
///   （与 `base/monster_scaling.json` 同源同值；2 位小数口径）。
impl Default for MonsterScalingTable {
    fn default() -> Self {
        Self {
            accuracy: MONSTER_ACCURACY_TABLE.to_vec(),
            evasion: MONSTER_EVASION_TABLE.to_vec(),
            armour: MONSTER_ARMOUR_TABLE.to_vec(),
            life: MONSTER_LIFE_TABLE.to_vec(),
            ally_life: crate::monster::MONSTER_ALLY_LIFE_TABLE.to_vec(),
            damage: MONSTER_DAMAGE_TABLE.to_vec(),
            ally_damage: vec![
                3.11, 4.42, 5.82, 7.31, 8.92, 10.63, 12.46, 14.42, // lv1-8
                16.51, 18.73, 21.1, 23.62, 26.31, 29.16, 32.19, 35.42, // lv9-16
                38.83, 42.46, 46.31, 50.39, 54.71, 59.29, 64.14, 69.27, // lv17-24
                74.69, 80.43, 86.5, 92.91, 99.69, 106.84, 114.4, 122.37, // lv25-32
                130.79, 139.67, 149.04, 158.91, 169.32, 180.29, 191.86, 204.04, // lv33-40
                216.86, 230.37, 244.6, 259.57, 275.32, 291.9, 309.34, 327.69, // lv41-48
                346.98, 367.27, 388.59, 411.01, 434.57, 459.32, 485.33, 512.66, // lv49-56
                541.35, 571.49, 603.14, 636.37, 671.26, 707.87, 746.3, 786.63, // lv57-64
                828.94, 873.34, 919.91, 968.76, 1019.99, 1073.72, 1130.06, 1189.13, // lv65-72
                1251.06, 1315.98, 1384.03, 1455.34, 1530.08, 1608.4, 1690.46,
                1776.43, // lv73-80
                1866.5, 1960.84, 2059.66, 2163.16, 2271.56, 2385.06, 2503.91,
                2628.36, // lv81-88
                2758.64, 2895.03, 3037.8, 3187.24, 3343.66, 3507.35, 3678.66,
                3857.93, // lv89-96
                4045.51, 4241.77, 4447.11, 4661.93, // lv97-100
            ],
            ailment_threshold: MONSTER_AILMENT_THRESHOLD_TABLE.to_vec(),
            poise_threshold: MONSTER_POISE_THRESHOLD_TABLE.to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monster;

    /// fallback 不变式：Default 七张准源表与 `monster.rs` 查表函数逐级相等
    /// （含 clamp 语义：0 → lv1、>100 → lv100）。
    #[test]
    fn default_lookups_match_legacy_monster_tables() {
        let t = MonsterScalingTable::default();
        for lv in 0..=120u32 {
            assert_eq!(
                t.accuracy_at(lv),
                monster::monster_accuracy(lv),
                "lv{lv} accuracy"
            );
            assert_eq!(
                t.evasion_at(lv),
                monster::monster_evasion(lv),
                "lv{lv} evasion"
            );
            assert_eq!(
                t.armour_at(lv),
                monster::monster_armour(lv),
                "lv{lv} armour"
            );
            assert_eq!(t.life_at(lv), monster::monster_life(lv), "lv{lv} life");
            assert_eq!(
                t.damage_at(lv),
                monster::monster_damage(lv),
                "lv{lv} damage"
            );
            assert_eq!(
                t.ailment_threshold_at(lv),
                monster::enemy_ailment_threshold(lv),
                "lv{lv} ailment_threshold"
            );
            assert_eq!(
                t.poise_threshold_at(lv),
                monster::enemy_poise_threshold(lv),
                "lv{lv} poise_threshold"
            );
        }
    }

    /// vendor-only ally 两表抽样（与 `base/monster_scaling.json` / Misc.lua L8、L10 同值）。
    #[test]
    fn default_ally_tables_spot_checks() {
        let t = MonsterScalingTable::default();
        assert_eq!(t.ally_life.len(), MONSTER_SCALING_TABLE_LEN);
        assert_eq!(t.ally_damage.len(), MONSTER_SCALING_TABLE_LEN);
        assert_eq!(t.ally_life[0], 51, "lv1 ally_life");
        assert_eq!(t.ally_life[84], 11708, "lv85 ally_life");
        assert_eq!(t.ally_life[99], 17980, "lv100 ally_life");
        assert_eq!(t.ally_damage[0], 3.11, "lv1 ally_damage");
        assert_eq!(t.ally_damage[84], 2271.56, "lv85 ally_damage");
        assert_eq!(t.ally_damage[99], 4661.93, "lv100 ally_damage");
    }

    /// 空表防御：查表对空 Vec 返回 0（不 panic）。
    #[test]
    fn empty_table_lookup_is_zero() {
        assert_eq!(lookup_u32(&[], 50), 0);
        assert_eq!(lookup_f64(&[], 50), 0.0);
    }
}
