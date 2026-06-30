//! Modifier 文本解析。
//!
//! - 数据驱动 scan 引擎（[`scan`] / [`compiled`] / [`forms`] / [`template`] /
//!   [`engine`]）：消费 Track A 预交付的 `overlay/mod_parser_rules.json`
//!   （schema `pobr_data::catalog::parser_rules`），照搬 vendor `ModParser.lua`
//!   的 `scan()` + `parseMod()` 语义（蓝图 m6-parser-rules.md §2–§4）。引擎入口
//!   [`engine::parse_mod_engine`] 是 calc 主路径的解析器——orchestrator 经
//!   pobr-gamedata 恒 load `mod_parser_rules.json` 编译 [`CompiledParserRules`]
//!   注入 session。
//! - [`legacy`]：M0–M5 的手写顺序剥离解析器——M6 删 legacy 收尾中的回退路径
//!   （[`dispatch::ParseCtx`] 未注入引擎规则时回退至此）；删 legacy 后收掉。
//! - [`outcome`]：解析输出共享类型（[`ParseOutcome`] 等，legacy 与引擎共用）。
//! - [`dispatch`]：解析派发上下文 [`ParseCtx`]——按是否注入引擎规则择路。
//! - [`canonical`]：[`ParseOutcome`] 的规范序列化（双跑 diff 与 precompile 的
//!   共用比较单位，C/D 衔接契约——禁两套）。

pub mod legacy;

/// 解析输出共享类型（legacy 与引擎共用；删 legacy 后随引擎留存）。
pub mod outcome;

/// 解析派发上下文 [`ParseCtx`]（legacy 与引擎共用；删 legacy 后随引擎留存）。
pub mod dispatch;

pub mod canonical;
pub mod compiled;
pub mod engine;
pub mod forms;
pub mod scan;
pub mod template;

// 解析输出共享类型从 `outcome` 再导出（调用方路径 `pobr_core::mod_parser::*` 不变）。
pub use outcome::{ParseError, ParseOutcome, ParseStatus, SpecialMatchMeta};
// 解析派发上下文从 `dispatch` 再导出（M6 删 legacy 前置 2/3：迁出 legacy.rs；调用方
// 路径 `pobr_core::mod_parser::ParseCtx` 不变）。
pub use dispatch::ParseCtx;
// legacy 解析入口再导出——既有调用方路径不变（删 legacy 时收掉）。
pub use legacy::{parse_minion_modifier, parse_mod, parse_mod_with_rules};

pub use canonical::{canonical_outcome, canonical_tags};
pub use compiled::{CompileError, CompiledParserRules};
pub use engine::parse_mod_engine;

#[cfg(feature = "test-rules")]
mod test_rules;
#[cfg(feature = "test-rules")]
pub use test_rules::test_compiled_rules;
