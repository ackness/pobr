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
