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
