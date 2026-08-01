//! 武器类型域 schema（`base/weapon_types.json`）。
//!
//! 源表：PoB2 `data.weaponTypeInfo`
//! （`vendor/PathOfBuilding-PoE2/src/Modules/Data.lua:532-551`，共 19 条）。
//! 本表为**逐条搬迁**（搬迁不变式）：字段值与 vendor
//! 完全一致；pobr 现有 Rust 侧的散落判定（见下）与 vendor 的出入只记录、不改值。
//!
//! 键空间说明：`id` 是 PoB base item 的 `type` 名（vendor `Data/Bases/*.lua` 的
//! `type` 字段），**不是** GGG `ItemClasses.Id`（pobr `BaseItemDef::item_class`）。
//! 两者大多同名，已知差异：
//! - PoE2 长杖（quarterstaff）基底 `type = "Staff"`、`subType = "Warstaff"`
//!   （`Data/Bases/staff.lua:159-167`），对应本表 `id = "Staff"`（`label =
//!   "Quarterstaff"`）；而 GGG item_class 把长杖记为 `Warstaff`、把法杖记为
//!   `Staff`。本表 `id = "Warstaff"` 条目在 vendor 基底数据中当前无基底使用
//!   （遗留条目，仅 `subType` 出现 `Warstaff`）。
//! - 钓竿：本表 `id = "Fishing Rod"`（有空格），GGG item_class 为 `FishingRod`。
//!
//! 与 pobr 现有 Rust 判定的已知出入（仅记录，行为对齐属后续独立 commit）：
//! - TODO(parity): `pobr-build::calc_orchestrator::weapon_type_conditions`
//!   的近战类列表（matches! 分支）不含 `Talisman` / `FishingRod`，而 vendor 对
//!   `Talisman` / `Fishing Rod` 均为 `melee = true`。
//! - TODO(parity): 同函数的 `two_handed` 谓词（`starts_with("Two Hand") ||
//!   "Warstaff" || "Staff"`）对 `Bow` / `Crossbow` / `Talisman` / `FishingRod`
//!   求得 false（即视为单手），而 vendor 这些类型均为 `oneHand = false`。
//!
//! 远程（ranged）派生：vendor 无独立 range 字段，远程 = `!melee`；
//! `flag` → `ModFlags` 位派生留代码侧。

use serde::{Deserialize, Serialize};

/// 武器类型定义（对应 vendor `data.weaponTypeInfo` 一条）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeaponTypeDef {
    /// 武器类型名（PoB base item `type`，如 `One Hand Axe`；`None` = 空手）。
    pub id: String,
    /// 是否单手（vendor `oneHand`；双持/持握条件判定用）。
    pub one_hand: bool,
    /// 是否近战（vendor `melee`；远程 = `!melee`，vendor 无独立 range 字段）。
    pub melee: bool,
    /// ModFlag 名（vendor `flag`，如 `Bow`/`Axe`/`Unarmed`）——武器类型位
    /// （`ModFlags` 扩位）由此派生，位枚举本身留代码（L4）。
    pub flag: String,
    /// 显示别名（vendor `label`；缺省时显示 `id`）。已知两条：
    /// `Staff` → `Quarterstaff`、`Thrusting One Hand Sword` → `One Hand Sword`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// 武器类型全表（[`crate::catalog::RuntimeConstants`] 的注入域）。
///
/// `#[serde(transparent)]`：JSON 形状与 `base/weapon_types.json`（数组）一致。
/// `Default` = fallback 全表（与 JSON 逐值相等，搬迁不变式）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WeaponTypeTable(pub Vec<WeaponTypeDef>);

impl Default for WeaponTypeTable {
    fn default() -> Self {
        Self(WeaponTypeDef::default_table())
    }
}

impl WeaponTypeTable {
    /// 按武器类型名（PoB base `type`，如 `One Hand Axe`）查条目；未知类型返回
    /// `None`。键空间陷阱（模块 doc）：GGG `Warstaff`（长杖 item_class）对应本表
    /// `Staff`、GGG `FishingRod` 对应 `Fishing Rod`——item_class → 表键的映射由
    /// 调用方做（消费侧，见 pobr-build `calc_orchestrator`）。
    pub fn get(&self, id: &str) -> Option<&WeaponTypeDef> {
        self.0.iter().find(|w| w.id == id)
    }
}

impl WeaponTypeDef {
    /// 全表 fallback（注入管道）：无 GameData / 数据目录缺
    /// `base/weapon_types.json` 时 [`crate::catalog::RuntimeConstants`] 的默认值。
    ///
    /// 搬迁不变式：与 JSON 逐值相等（`pobr-gamedata` 测试锁定）。数值出处 =
    /// vendor `data.weaponTypeInfo`（`Data.lua:532-551`，19 条逐条照搬）；
    /// pobr 旧 Rust 侧无完整对应表（散落谓词，出入见模块 doc TODO(parity)），
    /// 故此处为带出处 doc 的字面量。
    pub fn default_table() -> Vec<Self> {
        /// 单条目构造（label 见结构体字段 doc，仅两条非空）。
        fn entry(
            id: &str,
            one_hand: bool,
            melee: bool,
            flag: &str,
            label: Option<&str>,
        ) -> WeaponTypeDef {
            WeaponTypeDef {
                id: id.to_string(),
                one_hand,
                melee,
                flag: flag.to_string(),
                label: label.map(str::to_string),
            }
        }
        // 按 id 升序（与 JSON 排序口径一致，保证 Vec 逐值相等）。
        vec![
            entry("Bow", false, false, "Bow", None),
            entry("Claw", true, true, "Claw", None),
            entry("Crossbow", false, false, "Crossbow", None),
            entry("Dagger", true, true, "Dagger", None),
            entry("Fishing Rod", false, true, "Fishing", None),
            entry("Flail", true, true, "Flail", None),
            entry("None", true, true, "Unarmed", None),
            entry("One Hand Axe", true, true, "Axe", None),
            entry("One Hand Mace", true, true, "Mace", None),
            entry("One Hand Sword", true, true, "Sword", None),
            entry("Spear", true, true, "Spear", None),
            entry("Staff", false, true, "Staff", Some("Quarterstaff")),
            entry("Talisman", false, true, "Talisman", None),
            entry(
                "Thrusting One Hand Sword",
                true,
                true,
                "Sword",
                Some("One Hand Sword"),
            ),
            entry("Two Hand Axe", false, true, "Axe", None),
            entry("Two Hand Mace", false, true, "Mace", None),
            entry("Two Hand Sword", false, true, "Sword", None),
            entry("Wand", true, false, "Wand", None),
            entry("Warstaff", false, true, "Warstaff", None),
        ]
    }
}
