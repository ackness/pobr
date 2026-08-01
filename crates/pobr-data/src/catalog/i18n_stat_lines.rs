//! Mod-line input translation templates (`i18n/<lang>/stat_lines.json`, TODO Phase 7.1).
//!
//! Source = localized template pairs from GGG's stat descriptions
//! (transcribed from the CN client's dictionary via `pipeline/gen-zh-cn.mjs`).
//! The consumer (pobr-wasm's contract layer) reverse-looks-up a localized
//! mod line against its template, substitutes the numbers back in, and gets
//! an English canonical line to feed into the existing parser — the engine
//! itself stays English-only.

use serde::{Deserialize, Serialize};

/// A pair of mod-line templates: `src` = the localized template (with
/// `{0}` / `{0:+d}` placeholders), `en` = the corresponding English
/// canonical template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatLineTemplate {
    /// The localized template (e.g. `{0:+d} 生命上限`).
    pub src: String,
    /// The English canonical template (e.g. `{0:+d} to maximum Life`).
    pub en: String,
}
