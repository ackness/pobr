//! `overlay/curse_priority.json` schema（`curse_priority/v1`，-C）。
//!
//! vendor 来源：`Modules/Data.lua:274` 起的 `data.cursePriority` 纯数据表，
//! 经 `sync-pob-catalog extract-lua --what curse-priority` 确定性抽取
//! （luajit 求值表字面量，产物 byte-stable、`_meta` 记 vendor commit、
//! 禁手改）。vendor 平铺 `k=v` 表按语义拆为四段，便于消费侧按途查表。
//!
//! 消费侧（`calc/buff_pass.rs`）= PoB2
//! `determineCursePriority`（CalcPerform.lua:454-485）等价：
//! `priority = curse_base + min(socket_index, 8) × socket_priority_base
//!           + slot_weight(槽名去 " (Swap)" 后缀) + source_weight`，
//! source_weight 为 aura 源 → [`CursePriorityDef::curse_from_aura`]、装备隐式
//! 源 → [`CursePriorityDef::curse_from_equipment`]（Ring 2/3 折回 Ring 1 权重）。
//! 本模块只定义 serde 形状，零逻辑、零 I/O。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// 当前 overlay 文档 schema 标识（字段演化时递增）。
pub const CURSE_PRIORITY_SCHEMA: &str = "curse_priority/v1";

/// `overlay/curse_priority.json` 顶层（消费侧忽略 `_meta`；生产侧由工具
/// `CursePriorityDoc` flatten 包装）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CursePriorityDef {
    /// per-curse 基值（如 `"Temporal Chains"→1` … `"Poacher's Mark"→13`；
    /// 同槽冲突时值大者覆盖值小者）。
    pub curse_base: BTreeMap<String, i64>,
    /// socket 序权重单价（`min(socket_index, 8) × 此值`；vendor = 100）。
    pub socket_priority_base: i64,
    /// 装备槽名权重（`"Weapon 1"→1000` … `"Ring 3"→10000`；查表前去
    /// `" (Swap)"` 后缀）。
    pub slot_weights: BTreeMap<String, i64>,
    /// 装备隐式诅咒来源权重（vendor = 11000）。
    pub curse_from_equipment: i64,
    /// aura 来源诅咒权重（vendor = 20000，恒为最高一段）。
    pub curse_from_aura: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> CursePriorityDef {
        CursePriorityDef {
            curse_base: BTreeMap::from([("Temporal Chains".to_string(), 1)]),
            socket_priority_base: 100,
            slot_weights: BTreeMap::from([("Weapon 1".to_string(), 1000)]),
            curse_from_equipment: 11000,
            curse_from_aura: 20000,
        }
    }

    /// serde 往返：序列化 → 反序列化逐字段一致。
    #[test]
    fn serde_round_trip() {
        let def = sample();
        let json = serde_json::to_string_pretty(&def).unwrap();
        let back: CursePriorityDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back, def);
    }

    /// 消费侧加载容忍未知字段（overlay 文档头部的 `_meta` 直接忽略）。
    #[test]
    fn deserialize_ignores_meta() {
        let json = r#"{
            "_meta": {"schema": "curse_priority/v1"},
            "curse_base": {"Enfeeble": 2},
            "socket_priority_base": 100,
            "slot_weights": {"Amulet": 2000},
            "curse_from_equipment": 11000,
            "curse_from_aura": 20000
        }"#;
        let def: CursePriorityDef = serde_json::from_str(json).unwrap();
        assert_eq!(def.curse_base["Enfeeble"], 2);
        assert_eq!(def.slot_weights["Amulet"], 2000);
        assert_eq!(def.curse_from_aura, 20000);
    }
}
