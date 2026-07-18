//! 物品基底域 schema（`base/base_items.json`，来自 `BaseItemTypes.dat` 等）。

use serde::{Deserialize, Serialize};

/// 物品基底定义（来自 `BaseItemTypes.dat` + 外键解析）。
///
/// `name` 为英文 canonical；其它语言的名称走 `i18n/<lang>/base_items.json` 边车
/// （`id -> 本地化名称`）。武器/护甲数值（如 PhysicalMin/Max）来自独立的
/// `WeaponTypes` / `ArmourTypes` 表，后续切片接入。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// 基底固有 Spirit（如权杖 `spirit = 100`；来源 `ItemSpirit.dat` 的 `SpiritGranted`）。
    ///
    /// 该表对应 bundle 已被 CDN 对钉定 patch 剪除（.dat 路线不可得），当前由
    /// `overlay/base_item_overrides.json`（vendor `Data/Bases/*.lua` 确定性抽取，
    /// `sync-pob-catalog extract-bases`）经 gamedata merge 填充——蓝图 m2-defence
    /// §6 开放问题 2 的双路线兜底。无 Spirit 的基底为 `None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spirit: Option<u32>,
    /// charm 基底固有 buff 词条（vendor `Data/Bases/flask.lua` 的 `charm.buff`，
    /// 如 Ruby Charm `"+25% to Fire Resistance"`、Sapphire/Topaz 冰/电抗、Amethyst
    /// 混抗、其余免疫类）。该 buff **不在物品文本里**，是基底固有属性——charm
    /// active 时由 `pobr_core::ingest::item::ingest_flask_charm` 并入 `CharmBuff`
    /// 载荷（vendor `Item.lua:838-844` 把 `base.charm.buff` 逐行 parseMod 进
    /// `buffModList`）。GGG `.dat` 无此列，由 `overlay/base_item_overrides.json`
    /// （vendor `Data/Bases/flask.lua` 抽取，`sync-pob-catalog extract-bases`）经
    /// gamedata merge 填充——与 [`BaseItemDef::spirit`] 同款双路线兜底。
    /// 非 charm / 无 buff 为空。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub charm_buff: Vec<String>,
    /// 基底属性需求（vendor `Data/Bases/*.lua` 的 `req = { str/dex/int }`；GGG
    /// `.dat` 对应表 bundle 不可得，走 `overlay/base_item_overrides.json` 抽取
    /// merge——与 [`Self::spirit`] 同款兜底）。消费方 = 装备需求快照
    /// `<Attr>RequirementsOn<slot>`（vendor CalcPerform.lua:1848-1857，
    /// Smith『Gain Armour equal to 150% of total Strength Requirements …』）。
    /// 无需求为 0。
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub req_str: u32,
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub req_dex: u32,
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub req_int: u32,
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
    /// 弩装填时间（毫秒；M4-T4 W-D2）。真源 = `WeaponTypes.dat` 的 `ReloadTime`
    /// 列（vendor `Export/spec.lua:62483`、`Export/Scripts/bases.lua:268-269`
    /// `ReloadTimeBase = ReloadTime/1000`，仅 >0 时导出）。本地 pipeline/tables
    /// 快照缺失（drill F3/F8）期间由 `overlay/base_item_overrides.json`
    /// （vendor `Data/Bases/crossbow.lua` 的 `ReloadTimeBase` 秒值 ×1000，
    /// `sync-pob-catalog extract-bases`）经 gamedata merge 填充——与
    /// [`BaseItemDef::spirit`] 同款双路线兜底。非弩为 `None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reload_time_ms: Option<u32>,
}

/// 护甲基底数值（`ArmourTypes.dat` 外键解析；armour/evasion/ES/ward 局部基底）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArmourBaseStats {
    pub armour: u32,
    pub evasion: u32,
    pub energy_shield: u32,
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub ward: u32,
    /// 盾牌基底格挡几率（%；来源 `ShieldTypes.dat` 的 `Block` 列，对照 PoB2
    /// `Export/Scripts/bases.lua:277-279`）。该表 bundle 已被 CDN 对钉定 patch
    /// 剪除，当前由 `overlay/base_item_overrides.json` 经 gamedata merge 填充
    /// （双路线兜底，见 [`BaseItemDef::spirit`] 说明）。非盾为 `None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_chance: Option<f64>,
    /// 移动速度惩罚（小数，如 `0.03` = 3% 减速；`ArmourTypes.dat` 的
    /// `IncreasedMovementSpeed` 原始值按 PoB2 口径换算 `-raw/10000`，
    /// 对照 `Export/Scripts/bases.lua:298-300`）。无惩罚为 `None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub movement_penalty: Option<f64>,
}

/// serde 跳过零值 u32（diff 友好）。
fn is_zero_u32(v: &u32) -> bool {
    *v == 0
}

#[cfg(test)]
mod m4_t4_reload_tests {
    use super::WeaponBaseStats;

    /// reload_time_ms serde 往返无损；None 不落盘（既有 base JSON 零 diff）、
    /// 旧 JSON（无键）反序列化为 None（schema 向后兼容）。
    #[test]
    fn reload_time_round_trip_and_backward_compatible() {
        let weapon = WeaponBaseStats {
            physical_min: 7,
            physical_max: 12,
            speed_ms: 625,
            crit_chance: 500,
            range: 120,
            reload_time_ms: Some(800),
        };
        let json = serde_json::to_string(&weapon).unwrap();
        assert!(json.contains(r#""reload_time_ms":800"#));
        let parsed: WeaponBaseStats = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, weapon, "serde 往返必须无损");

        let none = WeaponBaseStats {
            reload_time_ms: None,
            ..weapon
        };
        let json = serde_json::to_string(&none).unwrap();
        assert!(!json.contains("reload_time_ms"), "None 不落盘：{json}");
        let legacy: WeaponBaseStats = serde_json::from_str(&json).unwrap();
        assert_eq!(legacy.reload_time_ms, None, "旧 JSON 缺键回退 None");
    }
}
