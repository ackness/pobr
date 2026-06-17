//! Parser 输出的共享类型（[`ParseOutcome`] / [`ParseStatus`] / [`SpecialMatchMeta`]
//! / [`ParseError`]）——legacy 与数据驱动引擎共用。
//!
//! 从 `legacy.rs` 抽出（D-T8 删 legacy 前置）：引擎（`engine.rs` / `canonical.rs`）
//! 的产物类型不应依赖 legacy 模块，故独立成模块；legacy 删除后这些类型随引擎留存。

use std::fmt;

use crate::Modifier;

/// 单行 modifier 的解析状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseStatus {
    /// 至少识别出结构（可能产 0 个 mod，如纯识别条目）。
    Parsed,
    /// 无任何规则命中——整行未支持。
    Unsupported,
}

/// 单行解析产物：mod 列表 + 状态 + 未解析残文 + special 命中元数据。
#[derive(Debug, Clone, PartialEq)]
pub struct ParseOutcome {
    /// 解析出的 modifier。
    pub mods: Vec<Modifier>,
    /// 解析状态。
    pub status: ParseStatus,
    /// 未消费的剩余文本（诊断 / 覆盖率报表用）。
    pub unparsed: Option<String>,
    /// special 词条规则命中元数据（M5b §2.3）。`None` = 走通用解析路径；
    /// `Some` = 由 [`crate::rules::SpecialModRules`] 整行命中产出（entry_id +
    /// verified 透传归因与 parity 报表）。
    pub special_meta: Option<SpecialMatchMeta>,
}

/// special 命中元数据（[`ParseOutcome::special_meta`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecialMatchMeta {
    /// 命中的 special 条目稳定 id。
    pub entry_id: String,
    /// 该条目是否经 oracle 对拍验证（`verified:false` 在 parity 报表单列）。
    pub verified: bool,
}

/// 解析失败（保留输入与原因，供调用方诊断）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// 原始输入文本。
    pub input: String,
    /// 失败原因。
    pub reason: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "failed to parse modifier {:?}: {}",
            self.input, self.reason
        )
    }
}

impl std::error::Error for ParseError {}
