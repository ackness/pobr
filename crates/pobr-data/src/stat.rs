use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StatId(String);

impl StatId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for StatId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for StatId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl fmt::Display for StatId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatDescription {
    pub id: StatId,
    pub text: String,
}

/// 描述某个 stat 的上下界规格，供 `pobr-core` 的 `stat_boundary` 消费。纯数据、无计算。
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct BoundarySpec {
    /// 下界（如抗性的 floor）；`None` 表示无下界。
    pub floor: Option<f64>,
    /// 默认最大值（如抗性默认 75）；`None` 表示无最大值约束。
    pub default_max: Option<f64>,
    /// 最大值硬上限（如抗性 90）；`None` 表示最大值不再被截断。
    pub hard_cap: Option<f64>,
}

impl BoundarySpec {
    pub fn new(floor: Option<f64>, default_max: Option<f64>, hard_cap: Option<f64>) -> Self {
        Self {
            floor,
            default_max,
            hard_cap,
        }
    }

    /// 元素 / 混沌抗性的边界规格：默认最大 75、硬上限 90、无下界。
    pub fn resistance() -> Self {
        Self {
            floor: None,
            default_max: Some(crate::constants::DEFAULT_MAX_RESISTANCE),
            hard_cap: Some(crate::constants::HARD_MAX_RESISTANCE),
        }
    }

    /// `resistance` 的别名（贴合机制蓝图命名）。
    pub fn resist_element() -> Self {
        Self::resistance()
    }
}

/// `StatId` 到 i18n 翻译 key 的占位 newtype；映射表在 `pobr-i18n` 层维护。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StatTextKey(pub String);

impl StatTextKey {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for StatTextKey {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for StatTextKey {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}
