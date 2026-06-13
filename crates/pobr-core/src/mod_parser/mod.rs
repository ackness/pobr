//! Modifier 文本解析——双实现并存期（M6-B）。
//!
//! - [`legacy`]：M0–M5 的手写顺序剥离解析器（既有调用方的唯一行为来源，
//!   切换前**保留不删**）。公有 API（`parse_mod` / `parse_mod_with_rules` /
//!   `parse_minion_modifier` / `ParseOutcome` 等）从此处再导出，调用方路径
//!   `pobr_core::mod_parser::*` 逐字不变。
//! - 数据驱动 scan 引擎（[`scan`] / [`compiled`] / [`forms`] / [`template`] /
//!   [`engine`]）：消费 Track A 预交付的 `overlay/mod_parser_rules.json`
//!   （schema `pobr_data::catalog::parser_rules`），照搬 vendor `ModParser.lua`
//!   的 `scan()` + `parseMod()` 语义（蓝图 m6-parser-rules.md §2–§4）。引擎入口
//!   [`engine::parse_mod_engine`] 与 legacy 并存，由 `feature = "parser-engine"`
//!   门控；**本 track 只建引擎 + 双跑达成 diff=0，绝不切换调用方**。切换（删
//!   legacy + 调用方注入 `&CompiledParserRules`）是 D-T8 的独立 commit。
//! - [`canonical`]：[`ParseOutcome`] 的规范序列化（双跑 diff 与 precompile 的
//!   共用比较单位，C/D 衔接契约——禁两套）。

pub mod legacy;

#[cfg(feature = "parser-engine")]
pub mod canonical;
#[cfg(feature = "parser-engine")]
pub mod compiled;
#[cfg(feature = "parser-engine")]
pub mod engine;
#[cfg(feature = "parser-engine")]
pub mod forms;
#[cfg(feature = "parser-engine")]
pub mod scan;
#[cfg(feature = "parser-engine")]
pub mod template;

// legacy 公有 API 再导出——既有调用方路径不变（搬迁不变式）。
pub use legacy::{
    ParseCtx, ParseError, ParseOutcome, ParseStatus, SpecialMatchMeta, parse_minion_modifier,
    parse_mod, parse_mod_with_rules,
};

#[cfg(feature = "parser-engine")]
pub use canonical::canonical_outcome;
#[cfg(feature = "parser-engine")]
pub use compiled::{CompileError, CompiledParserRules};
#[cfg(feature = "parser-engine")]
pub use engine::parse_mod_engine;
