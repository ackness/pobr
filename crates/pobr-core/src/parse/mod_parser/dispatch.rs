//! 解析派发上下文 [`ParseCtx`]——把可选的 special 规则 / 数据驱动引擎规则打包，沿
//! ingest 链（item / passive / gem）传递，决定每行词条走哪条解析路径。
//!
//! M6 D-T8「删 legacy 前置 2/3」：`ParseCtx` 从 `legacy.rs` 迁出至本独立模块（与 1/3
//! 把解析输出类型迁到 `outcome.rs` 同范式），使调用方依赖引擎侧的派发类型而非 legacy
//! 模块——为删 legacy 解耦。当前 `parse` 的 engine 分支走 [`parse_mod_engine`]，fallback
//! 仍调 legacy [`parse_mod_with_rules`]（删 legacy 前的回退路径）。
//!
//! [`parse_mod_with_rules`]: super::legacy::parse_mod_with_rules
//! [`parse_mod_engine`]: crate::mod_parser::parse_mod_engine

use super::legacy::parse_mod_with_rules;
use super::outcome::{ParseError, ParseOutcome};

/// special 规则解析上下文——把 [`SpecialModRules`] 与 [`HandlerRegistry`] 引用
/// 打包，沿 ingest 链（item / passive / gem）传递（M5b B-4 消费激活）。
///
/// 默认（[`ParseCtx::none`]）= 三者皆 `None`：等价历史 `parse_mod`，逐值不变。
/// `engine = Some` 时走数据驱动引擎；否则 `rules = Some` 时 special 条目整行命中优先。
///
/// [`SpecialModRules`]: crate::rules::SpecialModRules
/// [`HandlerRegistry`]: crate::rules::HandlerRegistry
#[derive(Debug, Clone, Copy, Default)]
pub struct ParseCtx<'a> {
    /// special 规则集（`None` = 不查表）。
    pub rules: Option<&'a crate::rules::SpecialModRules>,
    /// handler 注册表（handler_id 条目路由用）。
    pub registry: Option<&'a crate::rules::HandlerRegistry>,
    /// 数据驱动 parser 引擎规则（M6 D-T8 A2 全量穿线）。`Some` 时
    /// [`ParseCtx::parse`] 改走 [`parse_mod_engine`]（数据驱动终局路径）；`None`
    /// 时走 legacy `parse_mod_with_rules`（删 legacy 前的回退路径）。引擎对 18-build
    /// 语料 + fixture 与 legacy 逐字节一致（C1 DIFF=0 gate），故注入与否 parity 零变动。
    ///
    /// [`parse_mod_engine`]: crate::mod_parser::parse_mod_engine
    pub engine: Option<&'a crate::mod_parser::CompiledParserRules>,
}

impl<'a> ParseCtx<'a> {
    /// 空上下文（special/engine 皆 `None`）——历史 `parse_mod` 行为。
    pub fn none() -> Self {
        Self::default()
    }

    /// 携带 special 规则集（+ 可选 handler 注册表）。engine 字段保持 `None`
    /// （走 legacy special 路径，逐值不变）。
    pub fn with_rules(
        rules: &'a crate::rules::SpecialModRules,
        registry: Option<&'a crate::rules::HandlerRegistry>,
    ) -> Self {
        Self {
            rules: Some(rules),
            registry,
            engine: None,
        }
    }

    /// 携带数据驱动 parser 引擎规则（M6 D-T8 A2）：之后 [`parse`](Self::parse)
    /// 改走 [`parse_mod_engine`]。engine 路径自带 special 通道（编译进
    /// [`CompiledParserRules::special`]），故 legacy `rules`/`registry` 字段在此
    /// 路径**不消费**——保持 `None`。
    ///
    /// [`parse_mod_engine`]: crate::mod_parser::parse_mod_engine
    /// [`CompiledParserRules::special`]: crate::mod_parser::CompiledParserRules
    pub fn with_engine(engine: &'a crate::mod_parser::CompiledParserRules) -> Self {
        Self {
            rules: None,
            registry: None,
            engine: Some(engine),
        }
    }

    /// 按本上下文解析一行词条。
    ///
    /// - `engine = Some`（A2 生产路径）→ [`parse_mod_engine`]（数据驱动；引擎对
    ///   空输入返回 Unsupported 空表，永不报错）。
    /// - 否则 → legacy [`parse_mod_with_rules`]（`rules = None` 时等价
    ///   [`parse_mod`](super::legacy::parse_mod)，逐值不变）。
    ///
    /// [`parse_mod_engine`]: crate::mod_parser::parse_mod_engine
    /// [`parse_mod_with_rules`]: super::legacy::parse_mod_with_rules
    pub fn parse(&self, text: &str) -> Result<ParseOutcome, ParseError> {
        if let Some(engine) = self.engine {
            return Ok(crate::mod_parser::parse_mod_engine(text, engine));
        }
        parse_mod_with_rules(text, self.rules, self.registry)
    }
}
