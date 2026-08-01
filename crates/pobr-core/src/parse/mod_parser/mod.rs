//! Modifier 文本解析。
//!
//! - 数据驱动 scan 引擎（[`scan`] / [`compiled`] / [`forms`] / [`template`] /
//!   [`engine`]）：消费 Track A 预交付的 `overlay/mod_parser_rules.json`
//!   （schema `pobr_data::catalog::parser_rules`），照搬 vendor `ModParser.lua`
//!   的 `scan()` + `parseMod()` 语义。引擎入口
//!   [`engine::parse_mod_engine`] 是**唯一**解析器（收尾已删 legacy 手写
//!   解析器）——orchestrator 经 pobr-gamedata 恒 load `mod_parser_rules.json`
//!   编译 [`CompiledParserRules`] 注入 session；未注入规则时 [`ParseCtx`] 对
//!   每行返回整行 Unsupported（见 [`dispatch`] 模块文档）。
//! - [`outcome`]：解析输出共享类型（[`ParseOutcome`] 等）。
//! - [`dispatch`]：解析派发上下文 [`ParseCtx`]。
//! - [`canonical`]：[`ParseOutcome`] 的规范序列化（precompile 的比较单位）。

/// 解析输出共享类型。
pub mod outcome;

/// 解析派发上下文 [`ParseCtx`]。
pub mod dispatch;

pub mod canonical;
pub mod compiled;
pub mod engine;
pub mod forms;
pub mod scan;
pub mod template;

// 解析输出共享类型从 `outcome` 再导出（调用方路径 `pobr_core::mod_parser::*` 不变）。
pub use dispatch::ParseCtx;
pub use outcome::{ParseError, ParseOutcome, ParseStatus, SpecialMatchMeta};

pub use canonical::{canonical_outcome, canonical_tags};
pub use compiled::{CompileError, CompiledParserRules};
pub use engine::{EngineDiag, parse_mod_engine, parse_mod_engine_diag};

// `test-rules` feature 供下游 crate 集成测试；`cfg(test)` 使 pobr-core 自身
// 单测也能取真实规则（serde_json 走 dev-dependency，零 I/O 不变式只约束生产构建）。
#[cfg(any(test, feature = "test-rules"))]
mod test_rules;
#[cfg(any(test, feature = "test-rules"))]
pub use test_rules::test_compiled_rules;
