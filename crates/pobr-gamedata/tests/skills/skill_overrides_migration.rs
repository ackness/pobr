//! A **one-off value-by-value check** for the migration invariant
//! (env-triggered, skipped in CI by default).
//!
//! Usage: put the pre-migration hand-patched base files
//! (`granted_effect_levels.json` / `granted_effect_stat_sets.json`) in a
//! directory, then:
//!
//! ```bash
//! POBR_MIGRATION_OLD_BASE=/path/to/old_base \
//!   cargo test -p pobr-gamedata --test skill_overrides_migration
//! ```
//!
//! Asserts "the value in effect after plain base + overlay merge" is
//! **equal row for row, field for field** to the old hand-patched base;
//! the only allowed exception is FireRuneFireDjinn L2's
//! `crit_chance: 7.0` — vendor PoB2 has **no** critChance at that level
//! (the exporter writes each level's value independently, so an omission
//! means GGG's data has no value there, see the
//! `AttackCritChance ~= 0` guard in `vendor src/Export/Scripts/skills.lua`);
//! the old value was a fill-in artifact from when it was hand-patched,
//! corrected by this migration.

//! **Historical scope**: this check is only valid for a check-out made
//! **before** T4.3 — starting at T4.3, crit / attspd switched to reading
//! the `.dat` table columns directly (adding values for monster/non-vendor
//! skills beyond vendor's coverage), and the level rows gained T4.2's
//! field family, so an exact field-for-field match against the old
//! hand-patched base necessarily shows expected differences now. The proof
//! that the T4 channel switchover itself is value-for-value consistent
//! (3911 crit + 3578 attspd entries with zero drift, zero additions for
//! covered skills) is recorded in the T4.3 migration commit.

use std::collections::BTreeMap;

use pobr_data::catalog::{SkillLevelDef, SkillStatSetDef};
use pobr_gamedata::GameData;

/// A known fill-in artifact in the old hand-patched base (corrected during
/// the migration, not counted as drift).
const KNOWN_ARTIFACT_SKILL: &str = "FireRuneFireDjinn";
const KNOWN_ARTIFACT_LEVEL: u32 = 2;

#[test]
fn merged_values_equal_old_hand_patched_base() {
    let Ok(old_dir) = std::env::var("POBR_MIGRATION_OLD_BASE") else {
        eprintln!(
            "skip: POBR_MIGRATION_OLD_BASE not set (a one-off migration check, triggered on demand)"
        );
        return;
    };
    let old_dir = std::path::PathBuf::from(old_dir);

    let data = GameData::new(pobr_gamedata::repo_data_root().join(pobr_gamedata::data_version()));

    // granted_effect_levels: per effect, per row, per field.
    let old_levels: BTreeMap<String, Vec<SkillLevelDef>> = serde_json::from_slice(
        &std::fs::read(old_dir.join("granted_effect_levels.json"))
            .expect("failed to read the old level domain"),
    )
    .expect("failed to parse the old level domain");
    let merged_levels = data
        .granted_effect_levels()
        .expect("load + merge the level domain");

    assert_eq!(
        old_levels.keys().collect::<Vec<_>>(),
        merged_levels.keys().collect::<Vec<_>>(),
        "effect id sets must match"
    );
    let mut artifact_hits = 0usize;
    for (id, old_rows) in &old_levels {
        let merged_rows = &merged_levels[id];
        assert_eq!(
            old_rows.len(),
            merged_rows.len(),
            "{id} row count must match"
        );
        for (old_row, merged_row) in old_rows.iter().zip(merged_rows) {
            if old_row == merged_row {
                continue;
            }
            // The only allowed difference: the known fill-in artifact (old
            // Some(7.0) → new None, every other field equal).
            let is_known_artifact = id == KNOWN_ARTIFACT_SKILL
                && old_row.level == KNOWN_ARTIFACT_LEVEL
                && old_row.crit_chance == Some(7.0)
                && merged_row.crit_chance.is_none()
                && SkillLevelDef {
                    crit_chance: None,
                    ..old_row.clone()
                } == *merged_row;
            assert!(
                is_known_artifact,
                "{id} L{} drift: old {old_row:?} vs new {merged_row:?}",
                old_row.level
            );
            artifact_hits += 1;
        }
    }
    assert_eq!(
        artifact_hits, 1,
        "the known artifact should hit exactly once"
    );

    // granted_effect_stat_sets: value-equal as a whole.
    let old_sets: Vec<SkillStatSetDef> = serde_json::from_slice(
        &std::fs::read(old_dir.join("granted_effect_stat_sets.json"))
            .expect("failed to read the old stat-set domain"),
    )
    .expect("failed to parse the old stat-set domain");
    let merged_sets = data
        .skill_stat_sets()
        .expect("load + merge the stat-set domain");
    assert_eq!(
        old_sets, merged_sets,
        "the stat-set domain must be value-equal"
    );
}
