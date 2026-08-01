//! 解析派发上下文 [`ParseCtx`]——把可选的数据驱动引擎规则打包，沿 ingest 链
//! （item / passive / gem）传递，决定每行词条是否可解析。
//!
//! 收尾后引擎（[`parse_mod_engine`]）是唯一解析器：`engine = Some` 走数据驱动
//! 引擎；`engine = None`（未注入规则，如旧数据包 / 纯文本回退）**不再有 legacy
//! 回退**——每行按整行 [`ParseStatus::Unsupported`](super::ParseStatus::Unsupported)
//! 返回（词条不生效但被收集进 unsupported 报表，绝不静默丢失或误算）。
//!
//! [`parse_mod_engine`]: crate::mod_parser::parse_mod_engine

use super::outcome::{ParseError, ParseOutcome, ParseStatus};

/// 解析派发上下文：数据驱动引擎规则的可选引用，沿 ingest 链（item / passive /
/// gem / flask）传递。
///
/// 默认（[`ParseCtx::none`]）= 未注入规则：所有文本按 Unsupported 处理（见模块
/// 文档）。生产路径（orchestrator / wasm / CLI）恒经 `mod_parser_rules.json`
/// 编译 [`CompiledParserRules`] 注入。
///
/// [`CompiledParserRules`]: crate::mod_parser::CompiledParserRules
#[derive(Debug, Clone, Copy, Default)]
pub struct ParseCtx<'a> {
    /// 数据驱动 parser 引擎规则。`Some` 时 [`ParseCtx::parse`] 走
    /// [`parse_mod_engine`]；`None` 时每行返回 Unsupported。
    ///
    /// [`parse_mod_engine`]: crate::mod_parser::parse_mod_engine
    pub engine: Option<&'a crate::mod_parser::CompiledParserRules>,
}

impl<'a> ParseCtx<'a> {
    /// 空上下文（无引擎规则）：任何文本都解析为整行 Unsupported。
    pub fn none() -> Self {
        Self::default()
    }

    /// 携带数据驱动 parser 引擎规则：之后 [`parse`](Self::parse) 走
    /// [`parse_mod_engine`]（special 通道已编译进
    /// [`CompiledParserRules::special`]）。
    ///
    /// [`parse_mod_engine`]: crate::mod_parser::parse_mod_engine
    /// [`CompiledParserRules::special`]: crate::mod_parser::CompiledParserRules
    pub fn with_engine(engine: &'a crate::mod_parser::CompiledParserRules) -> Self {
        Self {
            engine: Some(engine),
        }
    }

    /// 按本上下文解析一行词条。
    ///
    /// - `engine = Some`（生产路径）→ [`parse_mod_engine`]（数据驱动；引擎对
    ///   无法识别的输入返回 Unsupported，永不报错）。
    /// - `engine = None` → 整行 [`ParseStatus::Unsupported`]（mods 空、
    ///   `unparsed = 原文`）。词条不生效但进入 unsupported 收集面（session /
    ///   报表可见），不会被当作已解析静默吞掉。
    ///
    /// [`parse_mod_engine`]: crate::mod_parser::parse_mod_engine
    pub fn parse(&self, text: &str) -> Result<ParseOutcome, ParseError> {
        match self.engine {
            Some(engine) => Ok(crate::mod_parser::parse_mod_engine(text, engine)),
            None => Ok(ParseOutcome {
                mods: Vec::new(),
                status: ParseStatus::Unsupported,
                unparsed: Some(text.trim().to_string()),
                special_meta: None,
            }),
        }
    }
}
