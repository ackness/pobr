//! pobr-tree 错误类型。

use thiserror::Error;

/// 天赋树相关操作的错误。
#[derive(Debug, Error)]
pub enum TreeError {
    /// JSON 解析失败（来自 serde_json，错误信息已转为 String 以避免泄漏内部类型）。
    #[error("failed to parse passive tree JSON: {0}")]
    Json(String),

    /// 引用了不存在的节点 skill id（如 radius jewel 的 socket）。
    #[error("passive tree node not found: {0}")]
    NodeNotFound(u32),

    /// 该节点没有可用坐标，无法做基于距离的范围计算。
    #[error("passive tree node has no position: {0}")]
    NodePositionMissing(u32),

    /// 非法半径（负数或 NaN 等）。
    #[error("invalid jewel radius: {0}")]
    InvalidRadius(f64),
}
