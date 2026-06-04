//! pobr-i18n: localization for PoBR's display layer.
//!
//! The calc engine works purely in stable IDs (`StatId`, `SkillId`,
//! `ItemBaseId`) and stable UI/error keys. This crate maps those keys to
//! user-facing text for the supported languages.
//!
//! - `en-US` is the canonical source of truth and the universal fallback.
//! - `zh-TW` is a partial translation; any key it omits falls back to `en-US`,
//!   and a key absent from both resolves to the key (or stat id) itself.
//!
//! All six locale files are embedded via `include_str!`, so there is no runtime
//! path dependency.
//!
//! ```
//! use pobr_i18n::{LanguageId, Translator, I18nValue};
//!
//! let t = Translator::new(LanguageId::new("zh-TW")).unwrap();
//! assert_eq!(t.text("build.copy_code"), "複製 BD");      // translated
//! assert_eq!(t.text("build.new_build"), "New build");    // falls back to en-US
//! let msg = t.format(
//!     "item.unknown_base",
//!     &[("base", I18nValue::Text("Iron Ring".into()))],
//! );
//! assert_eq!(msg, "未知的物品基底：Iron Ring");
//! ```

mod errors;
mod language;
mod loader;
mod stat_text;
mod translator;

pub use errors::I18nError;
pub use language::{CANONICAL_LANGUAGE, LanguageId, LanguagePack};
pub use stat_text::{STAT_PREFIX, stat_text_key};
pub use translator::{I18nValue, Translator};
