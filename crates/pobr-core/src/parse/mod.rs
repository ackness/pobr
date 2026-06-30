//! 解析层——「自由文本 → 词条」。
//!
//! 词条分层叙事的第 2 层之一（见 crate 根 `lib.rs` 的总览）：把人类可读的
//! modifier 文本（`"25% increased Fire Damage"` 等）翻译成 [`Modifier`](crate::Modifier)。
//! - [`mod_parser`]：数据驱动的 scan 引擎（消费 `overlay/mod_parser_rules.json`，
//!   照搬 PoB2 `ModParser.lua` 的 `scan()` + `parseMod()`）+ 待删的 legacy 手写解析器。
//! - [`apply_range`]：`+(40-50) to maximum Life` 类区间词条按 `range`（0..1）线性
//!   具体化为单值文本后再喂解析器（对照 PoB2 `ItemTools.lua::applyRange`）。
//! - [`mod_cache`]：`文本 → Vec<Modifier>` 解析缓存（热路径零重复解析）。
//!
//! 与 [`rules`](crate::rules) 的分工：本层处理**自由文本**，`rules` 处理**策展规则
//! 数据**（JSON 规则表 + handler）；二者都产出词条，但输入形态不同。

pub mod apply_range;
pub mod mod_cache;
pub mod mod_parser;
