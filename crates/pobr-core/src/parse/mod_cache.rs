use std::collections::HashMap;

use crate::mod_parser::{ParseCtx, ParseError, ParseOutcome};

#[derive(Debug, Clone, Default)]
pub struct ModCache {
    entries: HashMap<String, ParseOutcome>,
}

impl ModCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, text: &str) -> Option<&ParseOutcome> {
        self.entries.get(&normalize_cache_key(text))
    }

    /// 缓存命中即返回；未命中时经 `ctx` 解析（注入引擎规则时走数据驱动引擎，
    /// `ctx` 空时整行 Unsupported——见 [`ParseCtx::parse`]）。
    ///
    /// **缓存键不含 ctx**：调用方须保证同一 `ModCache` 实例的 `ctx` 在其生命周期内
    /// 一致（编排层每次 build 的 session 持单一 parser_rules，满足此约束）；混用不同
    /// `ctx` 会让先到的解析结果被缓存复用。
    pub fn parse_or_insert_with_ctx(
        &mut self,
        text: &str,
        ctx: ParseCtx<'_>,
    ) -> Result<ParseOutcome, ParseError> {
        let key = normalize_cache_key(text);
        if let Some(outcome) = self.entries.get(&key) {
            return Ok(outcome.clone());
        }

        let outcome = ctx.parse(text)?;
        self.entries.insert(key, outcome.clone());
        Ok(outcome)
    }
}

fn normalize_cache_key(text: &str) -> String {
    text.trim().to_ascii_lowercase()
}
