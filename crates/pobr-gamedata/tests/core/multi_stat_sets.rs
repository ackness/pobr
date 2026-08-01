//! Multi-statSet storage acceptance:
//! - the IceNova effect has ≥2 **additional** sets
//!   (`IceNovaPlayerOnFrostbolt` / `IceNovaColdInfusedPlayer`, the `.dat`'s
//!   `GrantedEffects.AdditionalStatSets` column);
//! - a vendor-exported set carries a non-empty label (merged from
//!   `overlay/stat_set_labels.json`);
//! - `GrantedEffectDef::additional_stat_set_ids`'s foreign key lines up
//!   with the stat-set domain.

use pobr_gamedata::{GameData, repo_data_root};

fn game_data() -> GameData {
    GameData::new(repo_data_root().join(pobr_gamedata::data_version()))
}

#[test]
fn ice_nova_has_additional_sets_with_labels() {
    let sets = game_data().skill_stat_sets().expect("load stat sets");
    let ice = sets
        .iter()
        .find(|s| s.effect_id == "IceNovaPlayer")
        .expect("IceNovaPlayer present");

    // The primary set + ≥2 additional sets.
    assert!(
        ice.sets.len() >= 3,
        "IceNova should have a primary set + ≥2 additional sets, got {}",
        ice.sets.len()
    );
    assert_eq!(
        ice.sets[0].set_id, "IceNovaPlayer",
        "sets[0] is always the primary set"
    );
    let set_ids: Vec<&str> = ice.sets.iter().map(|s| s.set_id.as_str()).collect();
    assert!(set_ids.contains(&"IceNovaPlayerOnFrostbolt"));
    assert!(set_ids.contains(&"IceNovaColdInfusedPlayer"));

    // A non-empty label (a vendor-exported set; OnFrostbolt was curated
    // out by vendor's template → no label, a faithful transcription of
    // PoB2's behavior).
    assert_eq!(ice.sets[0].label.as_deref(), Some("Ice Nova"));
    assert_eq!(ice.sets[0].vendor_set_index, Some(1));
    let cold = ice
        .sets
        .iter()
        .find(|s| s.set_id == "IceNovaColdInfusedPlayer")
        .expect("Cold-Infused set");
    assert_eq!(cold.label.as_deref(), Some("Cold-Infused"));
    assert_eq!(
        cold.vendor_set_index,
        Some(2),
        "vendor template index = PoB2's statSetIndex semantics (OnFrostbolt is curated out)"
    );
    let frostbolt = ice
        .sets
        .iter()
        .find(|s| s.set_id == "IceNovaPlayerOnFrostbolt")
        .expect("OnFrostbolt set");
    assert!(
        frostbolt.vendor_set_index.is_none(),
        "a set not exported by vendor must not be selectable via statSetIndex"
    );

    // An additional set has already gone through vendor's base-merge:
    // constants = the primary set's constants ++ this set's constants.
    assert!(
        cold.constant_stats.len() >= ice.sets[0].constant_stats.len(),
        "an additional set's constants should include the primary set's concatenated in"
    );
    assert!(
        cold.constant_stats
            .iter()
            .any(|c| c.stat == "base_skill_effect_duration"),
        "the primary set's constants are concatenated into the additional set (skills.lua:502-504)"
    );
}

#[test]
fn granted_effect_additional_stat_set_ids_align() {
    let data = game_data();
    let effects = data.granted_effects().expect("load effects");
    let ice = effects
        .iter()
        .find(|e| e.id == "IceNovaPlayer")
        .expect("IceNovaPlayer effect");
    assert_eq!(
        ice.additional_stat_set_ids,
        ["IceNovaPlayerOnFrostbolt", "IceNovaColdInfusedPlayer"],
        "AdditionalStatSets foreign key resolves to stable ids (column order preserved)"
    );
}
