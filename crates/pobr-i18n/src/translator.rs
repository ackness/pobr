//! The `Translator`: language selection, fallback resolution, formatting and
//! `StatId` display lookup over the embedded bundles.

use std::borrow::Cow;
use std::collections::BTreeSet;

use pobr_data::prelude::{DisplayStatId, StatId, StatTextKey};

use crate::errors::I18nError;
use crate::language::{CANONICAL_LANGUAGE, LanguageId};
use crate::loader::{self, Bundle};
use crate::stat_text;

/// A value substituted into a `{name}` placeholder by [`Translator::format`].
#[derive(Debug, Clone, PartialEq)]
pub enum I18nValue {
    /// A numeric value. Integral numbers render without a decimal point
    /// (`42`), non-integral ones use their default float formatting.
    Number(f64),
    /// An already-formatted text value, inserted verbatim.
    Text(String),
}

impl I18nValue {
    fn render(&self) -> String {
        match self {
            I18nValue::Text(s) => s.clone(),
            I18nValue::Number(n) => {
                if n.fract() == 0.0 && n.is_finite() {
                    format!("{}", *n as i64)
                } else {
                    n.to_string()
                }
            }
        }
    }
}

/// Resolves stable keys / `StatId`s into localized display text.
///
/// Holds the active language's bundle and the canonical fallback bundle. When
/// `active == fallback` (i.e. the active language is canonical) only one bundle
/// is consulted.
#[derive(Debug, Clone)]
pub struct Translator {
    active: LanguageId,
    fallback: LanguageId,
    active_bundle: Bundle,
    fallback_bundle: Bundle,
}

impl Translator {
    /// Build a translator for `active`, using the canonical language as
    /// fallback. Returns [`I18nError::UnknownLanguage`] if `active` has no
    /// embedded bundle.
    pub fn new(active: LanguageId) -> Result<Self, I18nError> {
        let fallback = LanguageId::new(CANONICAL_LANGUAGE);
        let active_bundle = loader::load_bundle(&active)?;
        let fallback_bundle = if active == fallback {
            active_bundle.clone()
        } else {
            loader::load_bundle(&fallback)?
        };
        Ok(Self {
            active,
            fallback,
            active_bundle,
            fallback_bundle,
        })
    }

    /// The active language.
    pub fn active(&self) -> &LanguageId {
        &self.active
    }

    /// The fallback (canonical) language.
    pub fn fallback(&self) -> &LanguageId {
        &self.fallback
    }

    /// Look up `key` in the active language, falling back to canonical, then to
    /// returning the key itself if neither defines it.
    pub fn text(&self, key: &str) -> Cow<'_, str> {
        if let Some(v) = self.active_bundle.get(key) {
            return Cow::Borrowed(v.as_str());
        }
        if let Some(v) = self.fallback_bundle.get(key) {
            return Cow::Borrowed(v.as_str());
        }
        Cow::Owned(key.to_string())
    }

    /// Look up `key` and substitute `{name}` placeholders from `args`.
    ///
    /// Unknown keys return the key verbatim (no interpolation). Placeholders
    /// without a matching arg are left untouched.
    pub fn format(&self, key: &str, args: &[(&str, I18nValue)]) -> String {
        let template = self.text(key);
        interpolate(&template, args)
    }

    /// Localized display name for a stat. Looks up `stat.<id>`; falls back to
    /// canonical, then to the stat id string itself.
    pub fn stat_text(&self, stat: &StatId) -> Cow<'_, str> {
        stat_text::resolve(&self.active_bundle, &self.fallback_bundle, stat)
    }

    /// Localized display name for a pre-derived [`StatTextKey`]. Falls back to
    /// canonical, then to the key string itself on a complete miss.
    pub fn stat_text_by_key(&self, key: &StatTextKey) -> Cow<'_, str> {
        stat_text::resolve_key(&self.active_bundle, &self.fallback_bundle, key)
    }

    /// The stable bundle key (`stat.<id>`) used to look up a stat's display
    /// name. Exposed so callers can carry the [`StatTextKey`] without
    /// re-deriving the convention.
    pub fn stat_text_key(stat: &StatId) -> StatTextKey {
        stat_text::stat_text_key(stat)
    }

    /// Localized label for an output display stat. Looks up `display.<id>`;
    /// falls back to canonical, then to the `DisplayStatId` string itself.
    ///
    /// The `DisplayStatId` values are PascalCase (e.g. `"TotalDPS"`) and map
    /// to the `[display]` section in `stats.toml`.
    pub fn display_stat_text(&self, id: &DisplayStatId) -> Cow<'_, str> {
        stat_text::resolve_display(&self.active_bundle, &self.fallback_bundle, id)
    }

    /// The stable bundle key (`display.<id>`) used to look up a display-stat
    /// label. Exposed for callers that carry the [`StatTextKey`] without
    /// re-deriving the `display.<id>` convention.
    pub fn display_stat_text_key(id: &DisplayStatId) -> StatTextKey {
        stat_text::display_stat_text_key(id)
    }

    /// All shipped languages, canonical first.
    pub fn supported_languages() -> Vec<LanguageId> {
        loader::SUPPORTED
            .iter()
            .map(|s| LanguageId::new(*s))
            .collect()
    }

    /// The full set of keys defined for `language` (post-flattening). Used by
    /// completeness lints. Errors if the language has no embedded bundle.
    pub fn all_keys(language: &LanguageId) -> Result<BTreeSet<String>, I18nError> {
        let bundle = loader::load_bundle(language)?;
        Ok(bundle.into_keys().collect())
    }
}

/// Replace every `{name}` occurrence in `template` whose `name` is present in
/// `args`. Unmatched placeholders are preserved verbatim.
fn interpolate(template: &str, args: &[(&str, I18nValue)]) -> String {
    if args.is_empty() || !template.contains('{') {
        return template.to_string();
    }
    let mut out = template.to_string();
    for (name, value) in args {
        let placeholder = format!("{{{name}}}");
        if out.contains(&placeholder) {
            out = out.replace(&placeholder, &value.render());
        }
    }
    out
}
