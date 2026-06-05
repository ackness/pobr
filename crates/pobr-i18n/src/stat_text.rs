//! Stat display-name resolution.
//!
//! The calc engine works in stable [`StatId`]s; this module maps them to
//! localized display text. Lookup keys live in `stats.toml` under the `stat.`
//! prefix (e.g. the id `life` resolves the bundle key `stat.life`).
//!
//! [`stat_text_key`] derives the i18n bundle key for a given id, exposed as a
//! [`StatTextKey`] newtype so callers can pass the stable key around without
//! re-deriving the `stat.<id>` convention.
//!
//! For output-display labels (the `DisplayStatId` PascalCase IDs from
//! `display_catalog`), use [`display_stat_text_key`] / [`resolve_display`]:
//! keys live under the `display.` prefix (e.g. `"TotalDPS"` → `display.TotalDPS`).

use std::borrow::Cow;

use pobr_data::prelude::{DisplayStatId, StatId, StatTextKey};

use crate::loader::Bundle;

/// Prefix under which stat display names live in `stats.toml`.
pub const STAT_PREFIX: &str = "stat";

/// Prefix under which display-stat labels live in `stats.toml`.
pub const DISPLAY_STAT_PREFIX: &str = "display";

/// The bundle key (`stat.<id>`) used to look up a stat's display name.
pub fn stat_text_key(stat: &StatId) -> StatTextKey {
    StatTextKey::new(format!("{STAT_PREFIX}.{}", stat.as_str()))
}

/// The bundle key (`display.<id>`) used to look up a display-stat label.
///
/// Unlike `stat_text_key`, the `DisplayStatId` is kept verbatim (PascalCase)
/// because that matches the TOML key convention in the `[display]` section.
pub fn display_stat_text_key(id: &DisplayStatId) -> StatTextKey {
    StatTextKey::new(format!("{DISPLAY_STAT_PREFIX}.{}", id.as_str()))
}

/// Resolve a `DisplayStatId` label against `active` then `fallback`.
///
/// On a complete miss the `DisplayStatId` string is returned (never the dotted
/// lookup key), so an unmapped ID surfaces its own PascalCase name verbatim.
pub fn resolve_display<'a>(
    active: &'a Bundle,
    fallback: &'a Bundle,
    id: &DisplayStatId,
) -> Cow<'a, str> {
    let key = display_stat_text_key(id);
    match lookup(active, fallback, key.as_str()) {
        Some(value) => value,
        None => Cow::Owned(id.as_str().to_string()),
    }
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
