//! Trade 层错误类型。

use thiserror::Error;

/// 官方 trade API 交互过程中可能出现的错误。
///
/// 真实 HTTP 实现（async / reqwest，本阶段延后）会把网络层错误映射到
/// [`TradeError::Http`]，把 JSON 解析失败映射到 [`TradeError::Parse`]，
/// 把 429 映射到 [`TradeError::RateLimit`]，把 404 / 空结果映射到
/// [`TradeError::NotFound`]。离线 mock 直接构造这些变体以覆盖错误路径。
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum TradeError {
    /// 传输层 / HTTP 状态错误，`String` 携带可诊断的上下文（状态码、消息）。
    #[error("trade http error: {0}")]
    Http(String),

    /// 响应体解析失败（JSON 结构不符合预期等）。
    #[error("trade parse error: {0}")]
    Parse(String),

    /// 触发官方速率限制（HTTP 429）。
    #[error("trade rate limit exceeded")]
    RateLimit,

    /// 查询无结果或目标资源不存在。
    #[error("trade resource not found")]
    NotFound,
}
