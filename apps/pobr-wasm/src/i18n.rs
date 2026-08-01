//! A thin wrapper around the i18n entry point: exposes [`pobr_i18n::Translator`]'s
//! key lookup as a pure `(lang, key) -> String` function, easy to call
//! across the wasm boundary with plain string in/out.

use pobr_i18n::{LanguageId, Translator};

/// Looks up the display text for `key` in the `lang` language.
///
/// Follows [`Translator`]'s fallback chain: active -> the canonical language
/// (en-US) -> the key itself. If `lang` has no embedded language pack
/// (unknown language), falls back to the canonical language before looking
/// up `key`; this way the frontend never gets an error for passing an
/// arbitrary tag — worst case it gets en-US text or the raw key.
pub fn translate(lang: &str, key: &str) -> String {
    let translator = Translator::new(LanguageId::new(lang))
        .or_else(|_| Translator::new(LanguageId::new(pobr_i18n::CANONICAL_LANGUAGE)));

    match translator {
        Ok(translator) => translator.text(key).into_owned(),
        // The canonical language is always embedded and available; reaching
        // here means pobr-i18n itself is broken, so fall back to the raw key.
        Err(_) => key.to_string(),
    }
}
