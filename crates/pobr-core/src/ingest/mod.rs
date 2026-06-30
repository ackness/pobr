//! 来源接入层——「来源对象 → 词条」。
//!
//! 词条分层叙事的第 2 层之一（见 crate 根 `lib.rs` 的总览）：把游戏里的各类
//! 来源对象转成带 [`SourceId`](pobr_data::source::SourceId) 归因的 [`Modifier`](crate::Modifier)，
//! 注入 [`ModDb`](crate::ModDb)。SourceId 让聚合结果可经 [`attribute`](crate::attribute)
//! 层回溯到「是哪件装备 / 哪个天赋 / 哪颗宝石贡献的」。
//! - [`item`] / [`item_text`]：装备（含 flask/charm 词条分支）；raw 文本 → calc 视图。
//! - [`passive`]：天赋树已分配节点的 mod 收集。
//! - [`skill_source`]：主动 / 辅助宝石 → 技能词条与伤害基值。
//! - [`character`]：职业 / 等级 / 属性派生的固有基础值。
//! - [`campaign`]：战役进度惩罚与永久奖励。
//!
//! 对照 PoB2 `Modules/CalcSetup.lua` 的来源装配。

pub mod campaign;
pub mod character;
pub mod item;
pub mod item_text;
pub mod passive;
pub mod skill_source;
