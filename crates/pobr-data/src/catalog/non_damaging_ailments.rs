//! 非伤害异常域 schema（`base/non_damaging_ailments.json`）。
//!
//! 对应 PoB2 vendor 三张表（`vendor/PathOfBuilding-PoE2/src/Modules/Data.lua`）：
//! - `data.nonDamagingAilment`（Data.lua:347-351）——chill/freeze/shock 的
//!   default/min/max/precision/duration（gameConstants 引用已解析为字面值）；
//! - `data.buildupTypes`（Data.lua:353-376)——积蓄类异常的伤害类型缩放来源；
//! - `data.defaultAilmentDamageTypes`（Data.lua:378-410）——各异常默认的
//!   伤害缩放来源 + 伤害型异常的 DoT 伤害类型。
//!
//! 数值与 pobr 现有 Rust 准源逐值相等（搬迁不变式）：
//! chill/shock 边界对应 `pobr_data::monster` 的
//! `CHILL_MIN_EFFECT`/`CHILL_MAX_EFFECT`/`BASE_SHOCK_MAGNITUDE`/`SHOCK_MAX_EFFECT`
//! 与 `pobr_data::constants::SHOCK_MIN_EFFECT`；伤害型异常的 `damage_type` 对应
//! `pobr_data::constants::AilmentType::damage_type()`。pobr 没有的字段
//! （associated_type/alt/precision/duration、freeze 边界、scales_from）从
//! vendor 抽取，来源行号见各字段 doc。计算公式仍留 `pobr-core::calc::ailment`，
//! 本表只迁数值（不接线，W3 再消费）。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::constants::DamageType;

/// `base/non_damaging_ailments.json` 顶层结构。
///
/// 三段 map 的 key 均为 vendor canonical 异常/积蓄名
/// （如 `Chill`/`Freeze`/`Shock`/`Electrocute`/`HeavyStun`/`Pin`/`Bleed`/`Poison`/`Ignite`）；
/// 用 `BTreeMap` 保证序列化键序稳定（diff 友好、可再生一致）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NonDamagingAilmentsDef {
    /// 非伤害异常强度边界（`data.nonDamagingAilment`，Data.lua:347-351）。
    pub ailments: BTreeMap<String, NonDamagingAilmentDef>,
    /// 积蓄类异常的缩放来源（`data.buildupTypes`，Data.lua:353-376）。
    pub buildup_types: BTreeMap<String, BuildupTypeDef>,
    /// 各异常默认的伤害缩放来源 / DoT 伤害类型
    /// （`data.defaultAilmentDamageTypes`，Data.lua:378-410）。
    pub default_ailment_damage_types: BTreeMap<String, AilmentDamageTypeDef>,
}

/// 单个非伤害异常的强度边界（`data.nonDamagingAilment` 行）。
///
/// 强度单位：Chill/Shock 为百分比（行动速度降低 % / 受伤增加 %），
/// Freeze 为秒（冰冻持续时间档位）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NonDamagingAilmentDef {
    /// 关联的元素伤害类型（Chill/Freeze→Cold，Shock→Lightning；Data.lua:348-350）。
    pub associated_type: DamageType,
    /// 是否为替代异常（alternative ailment；vendor 三项均 `false`，Data.lua:348-350）。
    pub alt: bool,
    /// 默认施加强度。Chill=30（准源 `monster::CHILL_MIN_EFFECT`）、
    /// Shock=20（准源 `monster::BASE_SHOCK_MAGNITUDE`/`constants::SHOCK_MIN_EFFECT`）；
    /// Freeze 无默认值（vendor `default = nil`，Data.lua:349）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<f64>,
    /// 最小有效强度（低于即不施加）。Chill=30 / Shock=20 同上；
    /// Freeze=0.3 秒（vendor-only，Data.lua:349）。
    pub min: f64,
    /// 强度上限。Chill=50（准源 `monster::CHILL_MAX_EFFECT`）、
    /// Shock=100（准源 `monster::SHOCK_MAX_EFFECT`）；Freeze=3 秒（vendor-only，Data.lua:349）。
    pub max: f64,
    /// 显示精度（小数位数；Chill/Shock=0，Freeze=2；vendor-only，Data.lua:348-350）。
    pub precision: u8,
    /// 基础持续时间（秒；vendor-only，gameConstants 已解析）：
    /// Chill=8（`BaseChillDuration`，Data/Misc.lua:91）、
    /// Freeze=4（`FreezeDuration`，Data/Misc.lua:56）、
    /// Shock=8（`BaseShockDuration`，Data/Misc.lua:93）。
    pub duration: f64,
}

/// 积蓄类异常（buildup）的伤害类型缩放来源（`data.buildupTypes` 行，vendor-only）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildupTypeDef {
    /// 参与积蓄的伤害类型（vendor `ScalesFrom` 集合，按 PoE canonical 伤害序
    /// Physical/Fire/Cold/Lightning/Chaos 排列）。空数组 = 不随击中伤害类型积蓄
    /// （Electrocute/Pin 由技能 stat 直接驱动，Data.lua:354-357、372-375）。
    pub scales_from: Vec<DamageType>,
}

/// 异常默认的伤害缩放来源 / DoT 伤害类型（`data.defaultAilmentDamageTypes` 行）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AilmentDamageTypeDef {
    /// 默认参与 magnitude 计算的击中伤害类型（vendor `ScalesFrom`，
    /// 按 canonical 伤害序排列；Data.lua:380-409）。
    pub scales_from: Vec<DamageType>,
    /// 伤害型异常的 DoT 伤害类型（Bleed→Physical / Poison→Chaos / Ignite→Fire；
    /// 准源 `constants::AilmentType::damage_type()`）；
    /// 非伤害异常（Shock/Chill）为 `None`（vendor 行无 `DamageType` 字段）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub damage_type: Option<DamageType>,
}
