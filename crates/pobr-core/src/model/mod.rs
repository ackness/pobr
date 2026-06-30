//! 词条本体层——「词条是什么」。
//!
//! 词条分层叙事的第 1 层（见 crate 根 `lib.rs` 的总览）：
//! - [`modifier`]：词条的数据类型 [`Modifier`](modifier::Modifier)（`{name, type,
//!   value, flags, keyword_flags, tags, source, origin}`）+ 标签体系
//!   [`ModTag`](modifier::ModTag)（Condition / Multiplier / PerStat / …）+ 求值
//!   入口 `matches` / `effective_number`。忠实移植 PoB2 `mod.lua` 的 `Mod` 与
//!   `ModStore.lua::EvalMod`。
//! - [`config`]：词条的求值上下文 [`CalcConfig`](config::CalcConfig) /
//!   [`EvalContext`](config::EvalContext)——flags / conditions / multipliers /
//!   damage_type 等，决定一条词条在「当前情境」下是否生效、生效几何。

pub mod config;
pub mod modifier;
