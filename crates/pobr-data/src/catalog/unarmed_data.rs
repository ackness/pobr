//! 空手基底域 schema（`base/unarmed_data.json`，per-class 空手 phys/攻速/暴击基底）。
//!
//! 对应 PoB2 `data.unarmedWeaponData`：
//! `vendor/PathOfBuilding-PoE2/src/Modules/Data.lua:553-563`（按 PoE2 classId 索引，
//! 9 个职业条目）；其中暴击常量源 `src/Data/Misc.lua:155`
//! （`characterConstants["unarmed_base_critical_strike_chance"] = 500`，
//! vendor 侧 `/ 100` 得百分数 `5`）。
//!
//! 搬迁不变式：数值以 pobr 现有 Rust 准源
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

/// 空手基底全表（[`crate::catalog::RuntimeConstants`] 的注入域）。
///
/// `#[serde(transparent)]`：JSON 形状与 `base/unarmed_data.json`（数组）一致。
/// `Default` = fallback 全表（与 JSON 逐值相等，搬迁不变式）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UnarmedDataTable(pub Vec<UnarmedWeaponDef>);

impl Default for UnarmedDataTable {
    fn default() -> Self {
        Self(UnarmedWeaponDef::default_table())
    }
}

impl UnarmedDataTable {
    /// 按职业英文名查空手基底条目；未知职业返回 `None`（消费方回退通用值）。
    pub fn for_class(&self, class_name: &str) -> Option<&UnarmedWeaponDef> {
        self.0.iter().find(|e| e.class_name == class_name)
    }
}

impl UnarmedWeaponDef {
    /// 全表 fallback（注入管道）：无 GameData / 数据目录缺
    /// `base/unarmed_data.json` 时 [`crate::catalog::RuntimeConstants`] 的默认值。
    ///
    /// 搬迁不变式：与 JSON 逐值相等（`pobr-gamedata` 测试锁定），数值出处 =
    /// pobr 旧 Rust 准源 `pobr-build::calc_orchestrator::unarmed_contribution`
    /// （该准源位于上层 crate，依赖方向不可达，故此处为带出处 doc 的字面量）+
    /// vendor `Data.lua:554-562`（class_id / weapon_type 等 vendor-only 字段）。
    pub fn default_table() -> Vec<Self> {
        /// 单条目构造：weapon_type 全表 `"None"`、攻速 1.65、暴击 0.05、物理下限 2
        /// 为全职业同值（vendor 同源），仅 class_id/职业名/物理上限逐职业不同。
        fn entry(class_id: u32, class_name: &str, physical_max: f64) -> UnarmedWeaponDef {
            UnarmedWeaponDef {
                class_id,
                class_name: class_name.to_string(),
                weapon_type: "None".to_string(),
                attack_rate: 1.65,
                crit_chance: 0.05,
                physical_min: 2.0,
                physical_max,
            }
        }
        // 按 class_id 升序（与 JSON 排序口径一致，保证 Vec 逐值相等）。
        vec![
            entry(0, "Scion", 6.0),
            entry(1, "Witch", 5.0),
            entry(2, "Ranger", 5.0),
            entry(6, "Warrior", 8.0),
            entry(7, "Sorceress", 5.0),
            entry(8, "Huntress", 5.0),
            entry(9, "Mercenary", 6.0),
            entry(10, "Monk", 5.0),
            entry(11, "Druid", 6.0),
        ]
    }
}
