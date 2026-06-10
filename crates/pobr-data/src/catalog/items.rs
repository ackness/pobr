//! 物品基底域 schema（`base/base_items.json`，来自 `BaseItemTypes.dat` 等）。

use serde::{Deserialize, Serialize};

/// 物品基底定义（来自 `BaseItemTypes.dat` + 外键解析）。
///
/// `name` 为英文 canonical；其它语言的名称走 `i18n/<lang>/base_items.json` 边车
/// （`id -> 本地化名称`）。武器/护甲数值（如 PhysicalMin/Max）来自独立的
/// `WeaponTypes` / `ArmourTypes` 表，后续切片接入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaseItemDef {
    /// 稳定 ID，即 `.dat` 的 `Id`（如 `Metadata/Items/Weapons/.../FourOneHandAxe1`）。
    pub id: String,
    /// 英文 canonical 名称。
    pub name: String,
    /// 物品类别（解析 `ItemClasses.Id`，如 `One Hand Axe`）。
    pub item_class: String,
    /// 掉落等级。
    pub drop_level: u32,
    /// 物品栏宽 / 高。
    pub width: u8,
    pub height: u8,
    /// 标签（解析 `Tags.Id`，如 `ezomyte_basetype`）。
    pub tags: Vec<String>,
    /// 固有词缀（implicit）的 mod 稳定 ID（解析 `Mods.Id`）。
    pub implicits: Vec<String>,
    /// mod domain（GGG 原始枚举值，用于词缀适用域判定）。
    pub mod_domain: u32,
    /// 武器基底数值（来自 `WeaponTypes.dat`；非武器为 `None`）——攻击伤害的基底。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weapon: Option<WeaponBaseStats>,
    /// 护甲基底数值（来自 `ArmourTypes.dat`；非护甲为 `None`）——armour/evasion/ES 局部基底。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub armour: Option<ArmourBaseStats>,
}

/// 武器基底数值（`WeaponTypes.dat` 外键解析；攻击技能伤害的基底，对照 PoB2
/// `CalcSetup.lua` weaponData 装配）。数值均为原始 `.dat` 整型，计算侧按单位换算。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeaponBaseStats {
    /// 基底物理伤害下/上限（`DamageMin`/`DamageMax`）。
    pub physical_min: u32,
    pub physical_max: u32,
    /// 攻击间隔（`Speed`，毫秒）；攻击速率 = `1000 / speed_ms`。
    pub speed_ms: u32,
    /// 基底暴击率（`CritChance` 原始值；暴击% = `crit_chance / 100`，如 `500` = 5%）。
    pub crit_chance: u32,
    /// 攻击射程（`RangeMax`）。
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub range: u32,
}

/// 护甲基底数值（`ArmourTypes.dat` 外键解析；armour/evasion/ES/ward 局部基底）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArmourBaseStats {
    pub armour: u32,
    pub evasion: u32,
    pub energy_shield: u32,
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub ward: u32,
}

/// serde 跳过零值 u32（diff 友好）。
fn is_zero_u32(v: &u32) -> bool {
    *v == 0
}
