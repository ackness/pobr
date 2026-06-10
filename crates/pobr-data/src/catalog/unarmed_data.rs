//! 空手基底域 schema（`base/unarmed_data.json`，per-class 空手 phys/攻速/暴击基底）。
//!
//! 对应 PoB2 `data.unarmedWeaponData`：
//! `vendor/PathOfBuilding-PoE2/src/Modules/Data.lua:553-563`（按 PoE2 classId 索引，
//! 9 个职业条目）；其中暴击常量源 `src/Data/Misc.lua:155`
//! （`characterConstants["unarmed_base_critical_strike_chance"] = 500`，
//! vendor 侧 `/ 100` 得百分数 `5`）。
//!
//! 搬迁不变式（架构文档 20 §1.1 / P8）：数值以 pobr 现有 Rust 准源
//! `pobr-build::calc_orchestrator::unarmed_contribution` 为准逐值搬迁
//! （`phys_min` / `phys_max` / `attack_rate` / `crit_chance`）；
//! `class_id` / `weapon_type` 为 vendor-only 字段（pobr 现按 `class_name` 匹配）。

use serde::{Deserialize, Serialize};

/// 单职业空手武器基底——无主手武器时攻击技能的 weaponData 来源
/// （对照 PoB2 `CalcSetup.lua:1578` `copyTable(env.data.unarmedWeaponData[env.classId])`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnarmedWeaponDef {
    /// PoE2 classId（vendor `Data.lua:554-562` 的表键）：0=Scion（PoE2 遗留占位）、
    /// 1=Witch、2=Ranger、6=Warrior、7=Sorceress、8=Huntress、9=Mercenary、
    /// 10=Monk、11=Druid。vendor-only（pobr 现无 classId 通道）。
    pub class_id: u32,
    /// 职业英文名（vendor 同行尾注释；pobr `Build.character.class_name` 的匹配键）。
    pub class_name: String,
    /// 武器类型（vendor `type = "None"`，对应 `data.weaponTypeInfo["None"]` →
    /// `Unarmed` flag，见 `Data.lua:533`）。vendor-only。
    pub weapon_type: String,
    /// 基底攻击速率（次/秒；vendor `AttackRate`，全职业 1.65）。
    pub attack_rate: f64,
    /// 基底暴击几率。pobr 现值 `0.05`（`unarmed_contribution` 注释口径「暴击 5%」的
    /// 小数表示），按搬迁不变式逐值照搬。
    ///
    /// TODO(parity): vendor 同字段为百分数 `5`（`Data.lua:554-562` =
    /// `Misc.lua:155` 的 500 / 100），且 pobr 自身武器路径
    /// （`weapon_contribution` 的 `raw crit / 100`）产出 `5.0`——空手与持武器两路
    /// 单位口径不一致。本任务只搬迁不改值，行为对齐留待后续独立 commit。
    pub crit_chance: f64,
    /// 基底物理伤害下限（vendor `PhysicalMin`，全职业 2）。
    pub physical_min: f64,
    /// 基底物理伤害上限（vendor `PhysicalMax`，按职业：Warrior 8、
    /// Scion/Mercenary/Druid 6、其余 5；与 pobr `unarmed_contribution` 的
    /// `class_name` match 一致）。
    pub physical_max: f64,
}
