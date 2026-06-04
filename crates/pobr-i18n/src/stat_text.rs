//! Stat display-name resolution.
//!
//! The calc engine works in stable [`StatId`]s; this module maps them to
//! localized display text. Lookup keys live in `stats.toml` under the `stat.`
//! prefix (e.g. the id `life` resolves the bundle key `stat.life`).
//!
//! [`stat_text_key`] derives the i18n bundle key for a given id, exposed as a
//! [`StatTextKey`] newtype so callers can pass the stable key around without
//! re-deriving the `stat.<id>` convention.

use std::borrow::Cow;

use pobr_data::prelude::{StatId, StatTextKey};

use crate::loader::Bundle;

/// Prefix under which stat display names live in `stats.toml`.
pub const STAT_PREFIX: &str = "stat";

/// The bundle key (`stat.<id>`) used to look up a stat's display name.
pub fn stat_text_key(stat: &StatId) -> StatTextKey {
    StatTextKey::new(format!("{STAT_PREFIX}.{}", stat.as_str()))
}

/// Resolve a stat's localized display name against `active` then `fallback`.
///
/// On a complete miss the stable id string is returned (never the dotted
/// lookup key), so a misspelled or unmapped stat surfaces its id verbatim.
pub fn resolve<'a>(active: &'a Bundle, fallback: &'a Bundle, stat: &StatId) -> Cow<'a, str> {
    let key = stat_text_key(stat);
    match lookup(active, fallback, key.as_str()) {
        Some(value) => value,
        None => Cow::Owned(stat.as_str().to_string()),
    }
}

/// Resolve a pre-derived [`StatTextKey`] against `active` then `fallback`.
///
/// Used by callers that already hold the stable bundle key. On a complete miss
/// the key string is returned verbatim.
pub fn resolve_key<'a>(
    active: &'a Bundle,
    fallback: &'a Bundle,
    key: &StatTextKey,
) -> Cow<'a, str> {
    match lookup(active, fallback, key.as_str()) {
        Some(value) => value,
        None => Cow::Owned(key.as_str().to_string()),
    }
}

/// Internal: active-then-fallback bundle lookup, borrowing from whichever holds
/// the value, returning `None` on a complete miss.
fn lookup<'a>(active: &'a Bundle, fallback: &'a Bundle, key: &str) -> Option<Cow<'a, str>> {
    if let Some(v) = active.get(key) {
        return Some(Cow::Borrowed(v.as_str()));
    }
    if let Some(v) = fallback.get(key) {
        return Some(Cow::Borrowed(v.as_str()));
    }
    None
}
