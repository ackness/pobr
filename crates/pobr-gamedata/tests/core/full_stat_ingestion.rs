//! Full stat-ingestion acceptance: `granted_effect_stat_sets.json` is no
//! longer filtered by the adapter's whitelist (a former suffix predicate,
//! already deleted alongside its consumer-side backstop at T2.4) —
//! non-damage stats that used to be filtered out (range/duration/
//! projectile count, etc.) must now appear in the stored JSON.
//!
//! The other half of the migration invariant (the consumer-side legacy
//! filter guaranteeing ninja stays value-for-value unchanged) is locked in
//! by `pobr-build::legacy_stat_filter`'s unit tests plus the ninja_parity
//! regression guardrail.

use pobr_gamedata::{GameData, repo_data_root};

#[test]
fn previously_filtered_stats_are_now_ingested() {
    let data = GameData::new(repo_data_root().join(pobr_gamedata::data_version()));
    let sets = data.skill_stat_sets().expect("load stat sets");

    // IceNova's primary set's constantStats contains
    // `base_skill_effect_duration` (8000) and
    // `active_skill_base_area_of_effect_radius` (32) — both non-damage
    // stats the old whitelist excluded (vendor
    // Data/Skills/act_int.lua's IceNovaPlayer statSets[1]).
    let ice = sets
        .iter()
        .find(|s| s.effect_id == "IceNovaPlayer")
        .and_then(|s| s.sets.first())
        .expect("IceNovaPlayer primary stat set present");
    let has_const = |stat: &str| ice.constant_stats.iter().any(|c| c.stat == stat);
    assert!(
        has_const("base_skill_effect_duration"),
        "the previously filtered duration stat should be ingested"
    );
    assert!(
        has_const("active_skill_base_area_of_effect_radius"),
        "the previously filtered radius stat should be ingested"
    );

    // A per-level row's non-damage stats are likewise stored (each of
    // IceNova's levels has a freeze/chill multiplier, and
    // `active_skill_chill_effect_+%_final` didn't match the old whitelist).
    let level1 = ice
        .levels
        .iter()
        .find(|l| l.gem_level == 1)
        .expect("IceNova L1 present");
    assert!(
        level1
            .stats
            .iter()
            .any(|s| s.stat == "active_skill_chill_effect_+%_final"),
        "the previously filtered per-level stat should be ingested"
    );
    // The existing damage stat is unaffected (a regression anchor).
    assert!(
        level1
            .stats
            .iter()
            .any(|s| s.stat == "spell_minimum_base_cold_damage" && s.value == 6.0),
        "the existing damage stat should be unchanged"
    );
}
