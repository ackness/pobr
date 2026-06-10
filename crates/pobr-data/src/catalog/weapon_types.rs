//! 武器类型域 schema（`base/weapon_types.json`）。
//!
//! 源表：PoB2 `data.weaponTypeInfo`
//! （`vendor/PathOfBuilding-PoE2/src/Modules/Data.lua:532-551`，共 19 条）。
//! 本表为**逐条搬迁**（搬迁不变式，架构文档 20 §1.1 / P8）：字段值与 vendor
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
//! `flag` → `ModFlags` 位派生留代码侧（L4，架构文档 20 §2.2，feature-gated 切换）。

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
