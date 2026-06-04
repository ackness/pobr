//! Language identifiers and the fallback concept.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A BCP-47-style language tag such as `"en-US"` or `"zh-TW"`.
///
/// Ordered/hashable so it can index maps and be sorted deterministically.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LanguageId(String);

impl LanguageId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for LanguageId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for LanguageId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl fmt::Display for LanguageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The canonical language: the single source of truth and the universal
/// fallback for any key missing from another language.
pub const CANONICAL_LANGUAGE: &str = "en-US";

/// Metadata describing one shipped language pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguagePack {
    pub id: LanguageId,
    pub display_name: String,
    /// `None` for the canonical language; otherwise the language consulted when
    /// a key is missing.
    pub fallback: Option<LanguageId>,
}

impl LanguagePack {
    pub fn new(
        id: LanguageId,
        display_name: impl Into<String>,
        fallback: Option<LanguageId>,
    ) -> Self {
        Self {
            id,
            display_name: display_name.into(),
            fallback,
        }
    }
}
