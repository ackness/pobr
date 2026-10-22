//! Locale TOML is validated and flattened by build.rs. Runtime lookup only
//! constructs the string map; it needs neither a parser nor filesystem access.

use std::collections::BTreeMap;

use crate::errors::I18nError;
use crate::language::LanguageId;

/// Dotted translation keys in deterministic order for completeness lints.
pub type Bundle = BTreeMap<String, String>;

include!(concat!(env!("OUT_DIR"), "/bundles.rs"));

/// The languages that ship with a bundle, in canonical-first order.
pub const SUPPORTED: [&str; 2] = ["en-US", "zh-TW"];

/// Load the compiled bundle, rejecting unsupported languages.
pub fn load_bundle(language: &LanguageId) -> Result<Bundle, I18nError> {
    let entries = match language.as_str() {
        "en-US" => EN,
        "zh-TW" => ZH,
        _ => return Err(I18nError::UnknownLanguage(language.to_string())),
    };
    Ok(entries
        .iter()
        .map(|&(key, value)| (key.to_owned(), value.to_owned()))
        .collect())
}
