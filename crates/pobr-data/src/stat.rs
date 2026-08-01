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

/// The bounds a stat is clamped to. Consumed by `pobr-core`'s `stat_boundary`;
/// pure data, no logic.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct BoundarySpec {
    /// Lower bound, such as the resistance floor. `None` means unbounded below.
    pub floor: Option<f64>,
    /// The maximum before any mods raise it — 75 for resistances. `None` means
    /// there is no maximum.
    pub default_max: Option<f64>,
    /// Ceiling the maximum itself cannot exceed — 90 for resistances. `None`
    /// means mods can raise the maximum without limit.
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

    /// Bounds for elemental and chaos resistance: 75 by default, 90 at most,
    /// no floor.
    pub fn resistance() -> Self {
        Self {
            floor: None,
            default_max: Some(crate::constants::DEFAULT_MAX_RESISTANCE),
            hard_cap: Some(crate::constants::HARD_MAX_RESISTANCE),
        }
    }

    /// Alias for [`Self::resistance`] that reads better at some call sites.
    pub fn resist_element() -> Self {
        Self::resistance()
    }
}

/// Translation key for a [`StatId`]. The key-to-text mapping lives in `pobr-i18n`.
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
