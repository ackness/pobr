//! Embedded language-bundle loading.
//!
//! The six locale files (`ui`, `stats`, `errors` for `en-US` and `zh-TW`) are
//! compiled into the binary via [`include_str!`], so the translator has no
//! runtime path dependency. Each file is parsed into a flat
//! `"section.key" -> value` map and the three files for a language are merged
//! into a single [`Bundle`].

use std::collections::BTreeMap;

use toml::Value;

use crate::errors::I18nError;
use crate::language::LanguageId;

/// A fully flattened key/value map for one language.
///
/// Keys use dotted notation (`"build.copy_code"`, `"stat.life"`). Backed by a
/// `BTreeMap` so iteration (and thus the completeness lint) is deterministic.
pub type Bundle = BTreeMap<String, String>;

// --- Embedded sources (canonical en-US + zh-TW). -------------------------

const EN_UI: &str = include_str!("../locales/en-US/ui.toml");
const EN_STATS: &str = include_str!("../locales/en-US/stats.toml");
const EN_ERRORS: &str = include_str!("../locales/en-US/errors.toml");

const ZH_UI: &str = include_str!("../locales/zh-TW/ui.toml");
const ZH_STATS: &str = include_str!("../locales/zh-TW/stats.toml");
const ZH_ERRORS: &str = include_str!("../locales/zh-TW/errors.toml");

/// The languages that ship with a bundle, in canonical-first order.
pub const SUPPORTED: [&str; 2] = ["en-US", "zh-TW"];

/// The three embedded `.toml` sources for `language`, or `None` if unsupported.
fn sources_for(language: &LanguageId) -> Option<[&'static str; 3]> {
    match language.as_str() {
        "en-US" => Some([EN_UI, EN_STATS, EN_ERRORS]),
        "zh-TW" => Some([ZH_UI, ZH_STATS, ZH_ERRORS]),
        _ => None,
    }
}

/// Load and flatten the merged bundle for `language`.
///
/// Returns [`I18nError::UnknownLanguage`] for an unsupported language and
/// [`I18nError::Parse`] if any embedded file is malformed (which would be a
/// build-time bug, since the files are vendored).
pub fn load_bundle(language: &LanguageId) -> Result<Bundle, I18nError> {
    let sources =
        sources_for(language).ok_or_else(|| I18nError::UnknownLanguage(language.to_string()))?;

    let mut bundle = Bundle::new();
    for src in sources {
        flatten_into(src, &mut bundle)?;
    }
    Ok(bundle)
}

/// Parse one TOML document and flatten its `[section] key = value` structure
/// into `section.key` entries, accumulating into `out`.
fn flatten_into(toml_src: &str, out: &mut Bundle) -> Result<(), I18nError> {
    // toml 1.x: `Value::from_str` only parses a single value; whole documents
    // parse as `Table`.
    let table: toml::Table = toml_src
        .parse()
        .map_err(|e: toml::de::Error| I18nError::Parse(e.to_string()))?;
    flatten_value(None, &Value::Table(table), out);
    Ok(())
}

/// Recursively flatten a TOML value into dotted keys. Only string leaves are
/// kept; nested tables compose their key with `.`.
fn flatten_value(prefix: Option<&str>, value: &Value, out: &mut Bundle) {
    match value {
        Value::Table(table) => {
            for (key, child) in table {
                let composed = match prefix {
                    Some(p) => format!("{p}.{key}"),
                    None => key.clone(),
                };
                flatten_value(Some(&composed), child, out);
            }
        }
        Value::String(s) => {
            if let Some(key) = prefix {
                out.insert(key.to_string(), s.clone());
            }
        }
        // Non-string leaves (numbers/bools/arrays/dates) are not valid display
        // text and are intentionally ignored.
        _ => {}
    }
}
