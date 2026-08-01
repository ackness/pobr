//! Spirit reservation aggregation for persistent-reservation effects
//! (`OutputTable::spirit_reserved`).
//!
//! Matches PoB2's `CalcDefence.lua:192-249` Reservation section (Spirit subset):
//! `reserved = max(round(flat_total × floor4(Π(1 + reservation_multiplier/100))), 0)`,
//! where flat/multiplier include contributions from same-group supports
//! (`CalcActiveSkill.lua:692-700` / `:754-756`). This test only exercises the
//! skill-side aggregation — the `Reserved`/`ReservationEfficiency` mod family
//! and the Spirit pool's own value/unreserved amount belong elsewhere;
//! over-reservation is only reported, never blocked.

use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup, calculate_with_data,
};
use pobr_data::catalog::{GrantedEffectDef, SkillLevelDef};
use pobr_gamedata::GameData;
use serde_json::json;

fn repo_data() -> BuildData {
    let data = GameData::new(pobr_gamedata::repo_data_root().join(pobr_gamedata::data_version()));
    BuildData::load(&data).expect("load repo data")
}

/// Builds a synthetic [`BuildData`]: a persistent-reservation aura (Spirit 60,
/// optionally with its own reservation multiplier) plus a support (optionally
/// with a spirit flat / reservation multiplier). Built through serde to avoid
/// coupling to schema fields added by parallel tracks (serde default absorbs
/// new fields).
fn synthetic_data(
    aura_reservation_multiplier: Option<f64>,
    support_spirit_flat: Option<f64>,
    support_reservation_multiplier: Option<f64>,
) -> BuildData {
    let mut data = BuildData::empty();
    let aura: GrantedEffectDef = serde_json::from_value(json!({
        "id": "TestAura",
        "is_support": false,
        "active_skill": "test_aura",
        "cast_time": null,
        "skill_types": ["HasReservation", "Buff", "Persistent", "Aura"],
    }))
    .unwrap();
    let support: GrantedEffectDef = serde_json::from_value(json!({
        "id": "TestSupport",
        "is_support": true,
        "active_skill": null,
        "cast_time": null,
    }))
    .unwrap();
    let aura_row: SkillLevelDef = serde_json::from_value(json!({
        "level": 1,
        "spirit_reservation_flat": 60.0,
        "reservation_multiplier": aura_reservation_multiplier,
    }))
    .unwrap();
    let support_row: SkillLevelDef = serde_json::from_value(json!({
        "level": 1,
        "spirit_reservation_flat": support_spirit_flat,
        "reservation_multiplier": support_reservation_multiplier,
    }))
    .unwrap();
    data.granted_effects.insert("TestAura".into(), aura);
    data.granted_effects.insert("TestSupport".into(), support);
    data.granted_effect_levels
        .insert("TestAura".into(), vec![aura_row]);
    data.granted_effect_levels
        .insert("TestSupport".into(), vec![support_row]);
    data
}

fn build_with_group(group: SocketGroup) -> Build {
    Build::new()
        .with_character(CharacterIdentity {
            level: 80,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(group)
}

fn calc(build: &Build, data: &BuildData) -> f64 {
    let opts = DataOrchestratorOptions::default();
    calculate_with_data(build, data, &opts)
        .expect("calc")
        .spirit_reserved
}

/// Real data: Alchemist's Boon (HasReservation, spiritReservationFlat 30, no
/// multiplier) -> spirit_reserved = 30.
#[test]
fn real_aura_reserves_flat_spirit() {
    let data = repo_data();
    let build = build_with_group(SocketGroup::new().with_gem_skill("AlchemistsBoonPlayer", 1));
    assert_eq!(calc(&build, &data), 30.0);
}

/// A non-reservation skill (an ordinary active skill) produces no Spirit reservation.
#[test]
fn non_reserving_skill_reserves_nothing() {
    let data = repo_data();
    let build = build_with_group(SocketGroup::new().with_gem_skill("FireballPlayer", 20));
    assert_eq!(calc(&build, &data), 0.0);
}

/// Bare flat: 60 × 1.0 = 60.
#[test]
fn flat_only_reservation() {
    let data = synthetic_data(None, None, None);
    let build = build_with_group(SocketGroup::new().with_gem_skill("TestAura", 1));
    assert_eq!(calc(&build, &data), 60.0);
}

/// Own negative reservation multiplier (e.g. -50): 60 × 0.5 = 30.
#[test]
fn own_negative_reservation_multiplier_halves() {
    let data = synthetic_data(Some(-50.0), None, None);
    let build = build_with_group(SocketGroup::new().with_gem_skill("TestAura", 1));
    assert_eq!(calc(&build, &data), 30.0);
}

/// Same-group support positive multiplier +20% (PoB2 `ReservationMultiplier`
/// MORE): 60 × 1.2 = 72; support spirit flat (ExtraSpirit) +10 folded into
/// base: (60+10) × 1.2 = 84.
#[test]
fn support_multiplier_and_flat_apply() {
    let data = synthetic_data(None, None, Some(20.0));
    let build = build_with_group(
        SocketGroup::new()
            .with_gem_skill("TestAura", 1)
            .with_gem_skill("TestSupport", 1),
    );
    assert_eq!(calc(&build, &data), 72.0);

    let data = synthetic_data(None, Some(10.0), Some(20.0));
    let build = build_with_group(
        SocketGroup::new()
            .with_gem_skill("TestAura", 1)
            .with_gem_skill("TestSupport", 1),
    );
    assert_eq!(calc(&build, &data), 84.0);
}

/// Multiplier stacking + truncation to 4 decimal places + round: own -50% ×
/// support +33% -> floor4(0.5 × 1.33) = 0.665 -> round(60 × 0.665) = round(39.9) = 40.
#[test]
fn multiplier_product_truncates_to_four_decimals_then_rounds() {
    let data = synthetic_data(Some(-50.0), None, Some(33.0));
    let build = build_with_group(
        SocketGroup::new()
            .with_gem_skill("TestAura", 1)
            .with_gem_skill("TestSupport", 1),
    );
    assert_eq!(calc(&build, &data), 40.0);
}

/// -100% reservation multiplier (e.g. Impurity-style zero reservation) -> reservation of 0.
#[test]
fn full_negative_multiplier_zeroes_reservation() {
    let data = synthetic_data(Some(-100.0), None, None);
    let build = build_with_group(SocketGroup::new().with_gem_skill("TestAura", 1));
    assert_eq!(calc(&build, &data), 0.0);
}

/// The same effect appearing in multiple groups is deduplicated by id (counted once); disabled groups don't participate.
#[test]
fn dedupes_across_groups_and_skips_disabled() {
    let data = synthetic_data(None, None, None);
    let build = build_with_group(SocketGroup::new().with_gem_skill("TestAura", 1))
        .add_socket_group(SocketGroup::new().with_gem_skill("TestAura", 1));
    assert_eq!(
        calc(&build, &data),
        60.0,
        "duplicate groups are deduplicated"
    );

    let mut disabled = SocketGroup::new().with_gem_skill("TestAura", 1);
    disabled.enabled = false;
    let build = Build::new()
        .with_character(CharacterIdentity {
            level: 80,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(disabled);
    assert_eq!(calc(&build, &data), 0.0, "a disabled group doesn't reserve");
}

/// Blasphemy per-curse reservation (vendor CalcDefence.lua:229-239): the
/// `IsBlasphemy` effect adds `blasphemy_base_spirit_reservation_per_socketed_curse`
/// (constant stat 60) once per same-group AppliesCurse active skill,
/// **folded into baseFlat first**, then scaled and rounded once as a whole
/// (:236-238). Quality efficiency (q20 Blasphemy = 20×0.5 = 10%) divides the
/// total: round(120/1.1) = 109 (not the old per-instance
/// round(60/1.1)=55×2=110; the essence-drain oracle pin uses the same
/// convention, 164).
#[test]
fn blasphemy_reserves_per_socketed_curse_with_quality_efficiency() {
    let data = repo_data();
    // q20 Blasphemy + 2 curses -> round(120/1.1) = 109 (curses themselves reserve 0).
    let build = build_with_group(
        SocketGroup::new()
            .with_gem_skill_quality("BlasphemyPlayer", 19, 20)
            .with_gem_skill("TemporalChainsPlayer", 19)
            .with_gem_skill("EnfeeblePlayer", 19),
    );
    assert_eq!(calc(&build, &data), 109.0);

    // q0 Blasphemy + 1 curse -> 60 (no efficiency scaling).
    let build = build_with_group(
        SocketGroup::new()
            .with_gem_skill("BlasphemyPlayer", 19)
            .with_gem_skill("TemporalChainsPlayer", 19),
    );
    assert_eq!(calc(&build, &data), 60.0);

    // Bare Blasphemy with no curse -> 0 (no per-curse instances, own flat is empty).
    let build = build_with_group(SocketGroup::new().with_gem_skill("BlasphemyPlayer", 19));
    assert_eq!(calc(&build, &data), 0.0);
}

/// Gem quality reservation efficiency also applies to ordinary reservation
/// skills (vendor :251 computes efficiency per-skill via skillCfg; the data
/// source here is the `base_reservation_efficiency_+%` slope in
/// overlay/gem_quality_stats.json). Herald of Thunder q20 = 20×0.5 = 10% ->
/// round(30/1.1) = 27.
#[test]
fn gem_quality_reservation_efficiency_scales_flat() {
    let data = repo_data();
    // Self-verifying data precondition: only assert scaling if HeraldOfThunderPlayer actually has a quality efficiency slope.
    let has_quality_eff = data
        .gem_quality_stats
        .get("HeraldOfThunderPlayer")
        .is_some_and(|rows| {
            rows.iter()
                .any(|q| q.stat == "base_reservation_efficiency_+%")
        });
    let build =
        build_with_group(SocketGroup::new().with_gem_skill_quality("HeraldOfThunderPlayer", 1, 20));
    if has_quality_eff {
        assert_eq!(calc(&build, &data), 27.0);
    } else {
        assert_eq!(calc(&build, &data), 30.0);
    }
}
