//! 聚合层——「把同名词条合成一个数」。
//!
//! 词条分层叙事的聚合环节（见 crate 根 `lib.rs` 的总览）：[`mod_db`] 的
//! [`ModDb`](mod_db::ModDb) 按 [`ModName`](pobr_data::modifier::ModName) 索引词条，
//! 提供查询原语 `sum`(Base/Inc 相加) / `more`(连乘 `Π(1+v/100)`) / `flag` /
//! `override_` / `list`，以及 traced 变体（产出 [`TraceGraph`](crate::TraceGraph)
//! 供归因）。标准属性管线 `(base + Σbase) × (1 + Σinc/100) × Π(1 + more/100)`。
//! 忠实移植 PoB2 `ModDB.lua` / `ModList.lua`。

pub mod mod_db;
