//! Error type for pobr-tree.

use thiserror::Error;

/// Errors from passive-tree operations.
#[derive(Debug, Error)]
pub enum TreeError {
    /// JSON parsing failed (from serde_json; the message is converted to a
    /// `String` to avoid leaking internal types).
    #[error("failed to parse passive tree JSON: {0}")]
    Json(String),

    /// Referenced a node skill id that doesn't exist (e.g. a radius jewel's socket).
    #[error("passive tree node not found: {0}")]
    NodeNotFound(u32),

    /// The node has no coordinates, so distance-based radius calculations can't run.
    #[error("passive tree node has no position: {0}")]
    NodePositionMissing(u32),

    /// An invalid radius (negative, NaN, etc).
    #[error("invalid jewel radius: {0}")]
    InvalidRadius(f64),
}
