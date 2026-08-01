//! End-to-end verification of the support-gem cost multiplier (SupportManaMultiplier).
//!
//! PoB2 primary source: injection at `CalcActiveSkill.lua:689-691` (a
//! compatible support's `level.manaMultiplier` -> `SupportManaMultiplier`
//! MORE); consumed by `CalcOffence.lua:2052`
//! `mult = floor(More("SupportManaMultiplier"), 4)` and `:2076-2077`
//! `finalBaseCost = floor(baseCost × mult)` (applied before the inc/more chain).
//! Only injects for the T3.6 compatible list — a rejected support's
//! multiplier doesn't count (matches PoB2's rejection behaviour).

use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup, calculate_with_data,
    parse_build_from_code,
};
use pobr_data::catalog::{GrantedEffectDef, SkillLevelDef};
use pobr_gamedata::{GameData, repo_data_root};
use serde_json::json;
use std::path::Path;

fn repo_data() -> BuildData {
    let data = GameData::new(repo_data_root().join(pobr_gamedata::data_version()));
    BuildData::load(&data).expect("load repo data")
}

/// Oracle comparison: druid-oracle-comet — Comet + a compatible cost-multiplier
/// support (current compatible set = Magnified Area II (+30%)).
///
/// **PoB2 golden**: the headless oracle (`tools/pob2-oracle/run.sh`, matching
/// meta.json `player_stats.ManaCost = 577`) reports
/// `ManaCostRaw = 577.72 = 404 × 1.43`, where 404 is Comet **L29**'s base cost
/// and 1.43 is `floor4(More("SupportManaMultiplier"))`.
///
/// **Current state (re-verified and re-pinned 2026-07)**: PoBR's output of 577
/// matches the PoB2 golden **exactly** — level L29 (tree GemProperty +1
/// counted), multiplier chain 1.43 = ER(1.1) × MagArea(1.3) (Boundless Energy
/// II still excluded — matching PoB2's own asymmetry). The old anchor of 525,
/// which recorded an "ER rejected" residual state, no longer reproduces; this
/// test was silently red under that old anchor for a while (the skills suite
/// isn't in the targeted gate set, so it got re-pinned incidentally while
/// working on the support-gem-count slice).
/// - Historical note: an even older anchor of 301 was once read as
///   `floor(211 × 1.43)` (L22 with ER counted), but it was actually a numeric
///   coincidence with `floor(232 × 1.3)` (L23 with ER excluded) — both
///   expressions happen to equal 301. Once the level bug was fixed the two
///   readings diverged (527 vs 479), and the anchor was set to 479 based on
///   the actual chain.
#[test]
fn oracle_druid_comet_mana_cost_with_support_multipliers() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/demo-bd-test/builds/druid-oracle-comet");
    let code = std::fs::read_to_string(dir.join("code.txt")).expect("read code.txt");
    let build = parse_build_from_code(code.trim()).expect("decode build");
    let data = repo_data();
    let opts = DataOrchestratorOptions {
        inject_character_base: true,
        ..Default::default()
    };
    let out = calculate_with_data(&build, &data, &opts).expect("calc");

    // Current multiplier chain: floor4(1.1 × 1.3) = 1.43 (ER + MagArea II; Boundless excluded --
    // matches PoB2, see the function doc comment).
    let support_mult = 1.43;
    // This assertion pins the **multiplier chain**, not the level resolution: the
    // output must equal floor(base × 1.43) for some Comet level row's cost
    // (still constrains the chain's shape even if level resolution changes).
    let comet_rows = &data.granted_effect_levels["CometPlayer"];
    let matched = comet_rows.iter().find(|r| {
        r.cost_amounts
            .first()
            .is_some_and(|&c| (f64::from(c) * support_mult).floor() == out.mana_cost)
    });
    assert!(
        matched.is_some(),
        "mana cost {} must = floor(base × 1.43) (the current compatible-set multiplier chain), and base must be \
         some Comet level row's cost -- the multiplier chain or compatibility ruling has drifted",
        out.mana_cost
    );
    // The concrete value is pinned to the PoB2 golden (meta.json
    // player_stats.ManaCost = 577 = floor(L29 base 404 × 1.43); update
    // explicitly and re-verify against the oracle if level/chain logic changes).
    assert_eq!(
        out.mana_cost, 577.0,
        "Comet L29 base 404 × 1.43 → floor = 577 (= PoB2 golden)"
    );
}

/// Synthetic end-to-end: a compatible support's multiplier reaches cost;
/// an incompatible support's (require-type mismatch, rejected by the T3.6
/// gating) multiplier does **not**.
#[test]
fn incompatible_support_multiplier_is_not_applied() {
    let mut data = BuildData::empty();
    let active: GrantedEffectDef = serde_json::from_value(json!({
        "id": "TestSpell",
        "is_support": false,
        "active_skill": "test_spell",
        "cast_time": 1000,
        "cost_types": [0],
        "skill_types": ["Spell", "Damage"],
    }))
    .unwrap();
    // Compatible: requires Spell; cost multiplier +30%.
    let sup_ok: GrantedEffectDef = serde_json::from_value(json!({
        "id": "TestSupportOk",
        "is_support": true,
        "active_skill": null,
        "cast_time": null,
        "require_skill_types": ["Spell"],
    }))
    .unwrap();
    // Incompatible: requires Attack (TestSpell doesn't have that type, rejected at stage ④ of the four-stage gating); multiplier +100%.
    let sup_rejected: GrantedEffectDef = serde_json::from_value(json!({
        "id": "TestSupportRejected",
        "is_support": true,
        "active_skill": null,
        "cast_time": null,
        "require_skill_types": ["Attack"],
    }))
    .unwrap();
    let active_row: SkillLevelDef = serde_json::from_value(json!({
        "level": 1,
        "cost_amounts": [100],
    }))
    .unwrap();
    let ok_row: SkillLevelDef = serde_json::from_value(json!({
        "level": 1,
        "mana_multiplier": 30.0,
    }))
    .unwrap();
    let rejected_row: SkillLevelDef = serde_json::from_value(json!({
        "level": 1,
        "mana_multiplier": 100.0,
    }))
    .unwrap();
    data.granted_effects.insert("TestSpell".into(), active);
    data.granted_effects.insert("TestSupportOk".into(), sup_ok);
    data.granted_effects
        .insert("TestSupportRejected".into(), sup_rejected);
    data.granted_effect_levels
        .insert("TestSpell".into(), vec![active_row]);
    data.granted_effect_levels
        .insert("TestSupportOk".into(), vec![ok_row]);
    data.granted_effect_levels
        .insert("TestSupportRejected".into(), vec![rejected_row]);

    let build = Build::new()
        .with_character(CharacterIdentity {
            level: 80,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            SocketGroup::new()
                .with_gem_skill("TestSpell", 1)
                .with_gem_skill("TestSupportOk", 1)
                .with_gem_skill("TestSupportRejected", 1),
        )
        .with_main_socket_group(1);

    let opts = DataOrchestratorOptions::default();
    let out = calculate_with_data(&build, &data, &opts).expect("calc");
    // Only the compatible +30% applies: floor(100 × 1.3) = 130. Wrongly including the rejected +100% would give 260.
    assert_eq!(
        out.mana_cost, 130.0,
        "only the compatible support's cost multiplier applies"
    );
}

/// Synthetic end-to-end: a negative multiplier (Impurity-style -100% doesn't
/// go through the support path — using -50% here) correctly reduces cost:
/// floor(100 × 0.5) = 50.
#[test]
fn negative_support_multiplier_reduces_cost() {
    let mut data = BuildData::empty();
    let active: GrantedEffectDef = serde_json::from_value(json!({
        "id": "TestSpell",
        "is_support": false,
        "active_skill": "test_spell",
        "cast_time": 1000,
        "cost_types": [0],
        "skill_types": ["Spell", "Damage"],
    }))
    .unwrap();
    let sup: GrantedEffectDef = serde_json::from_value(json!({
        "id": "TestSupportCheap",
        "is_support": true,
        "active_skill": null,
        "cast_time": null,
        "require_skill_types": ["Spell"],
    }))
    .unwrap();
    let active_row: SkillLevelDef = serde_json::from_value(json!({
        "level": 1,
        "cost_amounts": [100],
    }))
    .unwrap();
    let sup_row: SkillLevelDef = serde_json::from_value(json!({
        "level": 1,
        "mana_multiplier": -50.0,
    }))
    .unwrap();
    data.granted_effects.insert("TestSpell".into(), active);
    data.granted_effects.insert("TestSupportCheap".into(), sup);
    data.granted_effect_levels
        .insert("TestSpell".into(), vec![active_row]);
    data.granted_effect_levels
        .insert("TestSupportCheap".into(), vec![sup_row]);

    let build = Build::new()
        .with_character(CharacterIdentity {
            level: 80,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            SocketGroup::new()
                .with_gem_skill("TestSpell", 1)
                .with_gem_skill("TestSupportCheap", 1),
        )
        .with_main_socket_group(1);

    let opts = DataOrchestratorOptions::default();
    let out = calculate_with_data(&build, &data, &opts).expect("calc");
    assert_eq!(
        out.mana_cost, 50.0,
        "a negative multiplier reduces cost: floor(100 × 0.5) = 50"
    );
}
