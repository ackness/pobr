//! 基底物品覆盖值 overlay 域 schema（`overlay/base_item_overrides.json`）。
//!
//! 数据来源：vendor PoB2 `Data/Bases/*.lua`（shield / sceptre 等）——GGG `.dat`
//! 对应表（`ShieldTypes` 的 `Block` 列、`ItemSpirit` 的 `SpiritGranted` 列）所在
//! bundle 已被 CDN 对钉定 patch 剪除，`.dat` 路线不可得，按蓝图 m2-defence §6
//! 开放问题 1/2 的双路线裁决走 vendor 抽取兜底。由
//! `sync-pob-catalog extract-bases` 确定性抽取生成（schema 标识
//! `base_item_overrides/v1`，与 M0 的 `skill_overrides` 通道同构）。
//!
//! 消费侧：`pobr-gamedata` 在加载 `base/base_items.json` 时把本表按**英文
//! canonical 名称**merge 到 [`super::BaseItemDef`] 之上（merge 语义与单测见
//! `pobr-gamedata::domains::base_item_overrides`）。本模块只定义 serde 形状，零逻辑。

use serde::{Deserialize, Serialize};

/// 单条基底覆盖值（vendor `itemBases["<name>"]` 的 `armour.BlockChance` / `spirit`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaseItemOverrideEntry {
    /// 基底英文 canonical 名称（= vendor `itemBases` 键 = `BaseItemDef::name`）。
    pub name: String,
    /// 盾牌基底格挡几率（%；vendor `armour.BlockChance`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_chance: Option<f64>,
    /// 基底固有 Spirit（vendor `spirit`，如权杖 100）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spirit: Option<u32>,
    /// 弩装填时间（毫秒；vendor `weapon.ReloadTimeBase` 秒值 ×1000，源头 =
    /// `WeaponTypes.ReloadTime`——M4-T4 W-D2，本地 `.dat` 快照缺失期间的
    /// vendor 抽取兜底，消费侧写入 [`super::WeaponBaseStats::reload_time_ms`]）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reload_time_ms: Option<u32>,
}

/// `overlay/base_item_overrides.json` 顶层（消费侧视角：`_meta` 头部为生成溯源
/// 信息，serde 默认忽略未知字段，消费侧只取 `overrides` 列表）。
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BaseItemOverridesDef {
    /// 覆盖值列表，按 `name` 升序排序。
    pub overrides: Vec<BaseItemOverrideEntry>,
}
