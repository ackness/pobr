//! 防御 keystone 开关注册表（13-G6 / 13-G16）。
//!
//! 把「数据 flag → 有限稳定分支」的开关集中为一个一次性快照结构
//! [`DefenceKeystones`]：calc 各处只读本结构、不再各自散读 keystone flag
//!
//! 数据 vs 逻辑切分（13-defence §5 结论）：开关本身是数据（树词条 →
//! `Modifier::flag`，由 mod_parser / passive ingest 落入 ModDb）；行为是逻辑
//! （本注册表枚举的有限字段 + 各消费点的分支）。**不做** per-unique 硬编码。
//!
//! vendor 对照（`vendor/PathOfBuilding-PoE2/src/Modules/CalcDefence.lua`，
//! 行号 2026-06-11 核实）：
//! - `ChaosInoculation`：:85（flag 读出）/ :120-123（Life=1 + FullLife）/
//!   :2537-2539（眩晕阈值用 CI 前 Life）。
//! - `EnergyShieldProtectsMana`（EB）：:597-603（ES 嵌套保护 Mana 的 MoMEBPool）/
//!   :2726-2820（MoM/EB 池整备）。
//! - `EternalLife`：:588-594（ES 分支互斥）。
//! - `IronReflexes`/`Unbreakable`：:806-808 与 :1235-1237（两 flag 同时成立时
//!   Body Armour 闪避基底 ×2）；`Unbreakable` 单独成立时 Body Armour 护甲基底 ×2
//!   （:790-795 / :1216-1221）。
//! - `DoubleBodyArmourDefence`：:1150-1290（Body Armour 的 ward/ES/armour/evasion 皆 ×2）。
//! - `EnergyShieldToWard`：:1160-1192（ES 的 inc 借给 Ward、ES 本体不再聚合）。
//! - `WardNotBreak`：:560-575（ward 扣减后返还）/ :3030（EHP ∞ 分支）。
//! - `BloodMagic`：:172-350（预留改走生命，接 reservation，本阶段仅预留字段）。

use crate::{CalcConfig, ModDb};
use pobr_data::prelude::*;

/// ES→Mana 全转换的阈值（`EnergyShieldConvertToMana` BASE 累计 ≥ 100 视为
/// Eldritch Battery 型「全部 ES 转 Mana」，对齐 PoB2 resourceList 的 cap 100 语义）。
const ES_TO_MANA_FULL_CONVERSION_PCT: f64 = 100.0;

/// 防御 keystone 开关快照（一次性从 ModDb 读出，calc 各处只读本结构，不再散读 flag）。
///
/// 构造：[`DefenceKeystones::from_db`]。字段全 `bool`、`Copy`，按值传递；
/// `Default` = 全关（无任何 keystone 的 build，行为与未引入本结构前一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DefenceKeystones {
    /// Chaos Inoculation：Life=1、ES 作生命池、混沌免疫
    /// （mod_parser 已解析 `Maximum Life is 1` → Override + flag，~L537）。
    pub chaos_inoculation: bool,
    /// Eldritch Battery 型 ES→Mana 全转换：`EnergyShieldConvertToMana` BASE ≥ 100
    /// （既有 `es_to_mana_rate` 的「全转」语义并入；部分转换走转换矩阵的数值通道）。
    pub eldritch_battery_es_to_mana: bool,
    /// EB flag（`EnergyShieldProtectsMana`，W0.1 词条）：ES 保护 Mana 而非 Life。
    pub energy_shield_protects_mana: bool,
    /// Eternal Life：ES 扣减分支互斥（CalcDefence.lua:588-594）。
    pub eternal_life: bool,
    /// Iron Reflexes：`EvasionConvertToArmour` 100 的数据展开仍走转换矩阵；
    /// 本 flag 仅供 Unbreakable 联动（Body Armour 闪避基底 ×2）。
    pub iron_reflexes: bool,
    /// Unbreakable：Body Armour 护甲基底 ×2；与 IronReflexes 同时成立时闪避基底也 ×2。
    pub unbreakable: bool,
    /// Body Armour 的 ward/ES/armour/evasion 基底皆 ×2（CalcDefence.lua:1150-1290）。
    pub double_body_armour_defence: bool,
    /// ES 的 inc 借给 Ward、ES 本体不再聚合（CalcDefence.lua:1160-1192，Track D 消费）。
    pub energy_shield_to_ward: bool,
    /// Ward 扣减后返还（不破盾，CalcDefence.lua:560-575，Track A/F 消费）。
    pub ward_not_break: bool,
    /// Blood Magic：预留改走生命（预留字段，接 reservation）。
    pub blood_magic: bool,
}

impl DefenceKeystones {
    /// 从 ModDb 一次性读出全部防御 keystone 开关。
    ///
    /// 除 `eldritch_battery_es_to_mana`（`EnergyShieldConvertToMana` BASE 累计 ≥ 100）
    /// 外，其余字段均为同名 `Flag` 词条的直接读出（`ModDb::flag`，受 `cfg` 条件过滤）。
    pub fn from_db(db: &ModDb, cfg: &CalcConfig) -> Self {
        let es_to_mana_pct = db
            .sum(
                ModType::Base,
                cfg,
                &[ModName::from("EnergyShieldConvertToMana")],
            )
            .clamp(0.0, ES_TO_MANA_FULL_CONVERSION_PCT);
        Self {
            chaos_inoculation: db.flag(cfg, ModName::from("ChaosInoculation")),
            eldritch_battery_es_to_mana: es_to_mana_pct >= ES_TO_MANA_FULL_CONVERSION_PCT,
            energy_shield_protects_mana: db.flag(cfg, ModName::from("EnergyShieldProtectsMana")),
            eternal_life: db.flag(cfg, ModName::from("EternalLife")),
            iron_reflexes: db.flag(cfg, ModName::from("IronReflexes")),
            unbreakable: db.flag(cfg, ModName::from("Unbreakable")),
            double_body_armour_defence: db.flag(cfg, ModName::from("DoubleBodyArmourDefence")),
            energy_shield_to_ward: db.flag(cfg, ModName::from("EnergyShieldToWard")),
            ward_not_break: db.flag(cfg, ModName::from("WardNotBreak")),
            blood_magic: db.flag(cfg, ModName::from("BloodMagic")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Modifier;

    /// 空 ModDb → 全部开关关闭（与 `Default` 一致）。
    #[test]
    fn from_db_empty_is_all_off() {
        // Arrange
        let db = ModDb::new();
        let cfg = CalcConfig::new();

        // Act
        let ks = DefenceKeystones::from_db(&db, &cfg);

        // Assert
        assert_eq!(ks, DefenceKeystones::default());
        assert!(!ks.chaos_inoculation);
        assert!(!ks.eldritch_battery_es_to_mana);
    }

    /// 各 flag 词条逐一驱动对应字段（一次性快照，不读其它 flag）。
    #[test]
    fn from_db_reads_each_flag() {
        // Arrange
        let mut db = ModDb::new();
        db.add_list([
            Modifier::flag("ChaosInoculation"),
            Modifier::flag("EnergyShieldProtectsMana"),
            Modifier::flag("EternalLife"),
            Modifier::flag("IronReflexes"),
            Modifier::flag("Unbreakable"),
            Modifier::flag("DoubleBodyArmourDefence"),
            Modifier::flag("EnergyShieldToWard"),
            Modifier::flag("WardNotBreak"),
            Modifier::flag("BloodMagic"),
        ]);
        let cfg = CalcConfig::new();

        // Act
        let ks = DefenceKeystones::from_db(&db, &cfg);

        // Assert
        assert!(ks.chaos_inoculation);
        assert!(ks.energy_shield_protects_mana);
        assert!(ks.eternal_life);
        assert!(ks.iron_reflexes);
        assert!(ks.unbreakable);
        assert!(ks.double_body_armour_defence);
        assert!(ks.energy_shield_to_ward);
        assert!(ks.ward_not_break);
        assert!(ks.blood_magic);
        // 未注入 EnergyShieldConvertToMana → eldritch 仍为 false。
        assert!(!ks.eldritch_battery_es_to_mana);
    }

    /// `EnergyShieldConvertToMana` 累计 ≥100 才视为全转换（50+50 命中，50 不命中）。
    #[test]
    fn eldritch_battery_requires_full_conversion() {
        // Arrange：50% 部分转换 → 不是全转换 keystone。
        let mut partial = ModDb::new();
        partial.add_list([Modifier::number(
            "EnergyShieldConvertToMana",
            ModType::Base,
            50.0,
        )]);
        // 50 + 50 = 100 → 全转换。
        let mut full = ModDb::new();
        full.add_list([
            Modifier::number("EnergyShieldConvertToMana", ModType::Base, 50.0),
            Modifier::number("EnergyShieldConvertToMana", ModType::Base, 50.0),
        ]);
        let cfg = CalcConfig::new();

        // Act / Assert
        assert!(!DefenceKeystones::from_db(&partial, &cfg).eldritch_battery_es_to_mana);
        assert!(DefenceKeystones::from_db(&full, &cfg).eldritch_battery_es_to_mana);
    }
}
