//! Error type for the i18n layer.

use thiserror::Error;

/// Failures that can arise while constructing or querying a [`crate::Translator`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum I18nError {
    /// The requested language has no embedded bundle.
    #[error("unknown language: {0}")]
    UnknownLanguage(String),

    /// A key was requested that does not exist in the active language or its
    /// fallback. (The translator usually returns the key verbatim instead of
    /// raising this; it exists for stricter callers and diagnostics.)
    #[error("missing translation key `{key}` for language `{lang}`")]
    MissingKey { key: String, lang: String },

    /// An embedded TOML bundle failed to parse.
    #[error("failed to parse language bundle: {0}")]
    Parse(String),
}
