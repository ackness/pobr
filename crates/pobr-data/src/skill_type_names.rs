//! Full lookup table from PoB2 SkillType enum name → id (1-based) — the
//! backing table for [`crate::skill::SkillTypes::from_pob2_name`].
//!
//! The actual data lives in the sidecar `skill_type_names.txt` (generated
//! from vendor `Global.lua::SkillType` by `gen-skill-types`, embedded via
//! `include_str!` — the same pattern as pobr-i18n's locale tomls; the data
//! table doesn't go in a `.rs` file, so it's outside the `no_embedded_data`
//! guard's reach). Line format is `name id`, `#` lines are comments;
//! generated output is already sorted by name, and this module sorts it
//! again as a fallback (the binary search shouldn't have to depend on the
//! generator's ordering convention).

use std::sync::LazyLock;

static RAW: &str = include_str!("skill_type_names.txt");

/// (name, id) pairs, sorted by name — for binary search.
pub(crate) static SKILL_TYPE_IDS: LazyLock<Vec<(&'static str, u32)>> = LazyLock::new(|| {
    let mut entries: Vec<(&str, u32)> = RAW
        .lines()
        .filter(|l| !l.trim_start().starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let (name, id) = l
                .split_once(' ')
                .unwrap_or_else(|| panic!("malformed line in skill_type_names.txt: {l}"));
            let id: u32 = id.trim().parse().unwrap_or_else(|e| {
                panic!("failed to parse id in skill_type_names.txt ({l}): {e}")
            });
            (name, id)
        })
        .collect();
    entries.sort_unstable();
    assert!(!entries.is_empty(), "skill_type_names.txt is empty");
    entries
});

/// Looks up id (1-based) by name; an unknown name → None.
pub(crate) fn lookup(name: &str) -> Option<u32> {
    SKILL_TYPE_IDS
        .binary_search_by_key(&name, |(n, _)| n)
        .ok()
        .map(|i| SKILL_TYPE_IDS[i].1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_covers_low_and_high_ids() {
        assert_eq!(lookup("Attack"), Some(1));
        assert_eq!(lookup("Persistent"), Some(140));
        assert_eq!(lookup("SupportedByCreepingChill"), Some(290));
        assert_eq!(lookup("NotAType"), None);
    }

    #[test]
    fn table_is_full_vendor_enum() {
        // A coarse check on the full extraction: vendor 0.5.x has 290 enum entries;
        // vendor upgrades only ever add, never remove.
        assert!(SKILL_TYPE_IDS.len() >= 290);
    }
}
