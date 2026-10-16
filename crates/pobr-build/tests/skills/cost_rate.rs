//! Costs follow the game's payment event, including explicit sustained drains.
use std::sync::OnceLock;

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build_from_code};
use pobr_gamedata::{GameData, data_version, repo_data_root};

fn data() -> &'static BuildData {
    static DATA: OnceLock<BuildData> = OnceLock::new();
    DATA.get_or_init(|| {
        BuildData::load(&GameData::new(repo_data_root().join(data_version()))).unwrap()
    })
}

#[test]
fn oil_barrage_pays_its_explicit_drain_without_repeating_the_start_cost() {
    let build = parse_build_from_code(include_str!(
        "../../../../examples/reference-builds/tuned/builds/ninja-2026-10-14-mercenary-tactician-oil-barrage-02b94461578f/code.txt"
    )).unwrap();
    let options = DataOrchestratorOptions::default();
    let out = calculate_with_data(&build, data(), &options).unwrap();
    // Level 27: 69 Mana upfront, 4049 Mana/minute while channelling.
    // The build's 110% support multiplier gives floor(69 * 1.1) = 75
    // upfront and floor(4049 / 60 * 1.1) = 74 each second.
    assert_eq!(out.mana_cost, 75.0);
    assert_eq!(out.mana_per_second_cost, (4049.0_f64 / 60.0 * 1.1).floor());
    let fast = calculate_with_data(
        &build,
        data(),
        &DataOrchestratorOptions {
            extra_modifier_texts: vec!["100% increased Attack Speed".into()],
            ..options
        },
    )
    .unwrap();
    assert!(fast.action_rate > out.action_rate);
    assert_eq!(fast.mana_per_second_cost, 74.0);
}

#[test]
fn dark_consequences_pays_archmages_added_cost_for_each_explosion() {
    let build = parse_build_from_code(include_str!(
        "../../../../examples/reference-builds/tuned/builds/ninja-2026-10-14-witch-blood-mage-impending-doom-c0dcbc68022c/code.txt"
    )).unwrap();
    let options = DataOrchestratorOptions::default();
    let out = calculate_with_data(&build, data(), &options).unwrap();
    // Dark Consequences has no native cost. Archmage adds 5.7% of maximum
    // Mana, Atalui converts it to Life, and Efficiency II gives 40% less
    // cost: ceil(ceil(5800.12 * .057) * .6) = 199 Life per explosion.
    assert_eq!(out.life_cost, (331.0_f64 * 0.6).ceil());
    // Two Cursed Ground areas each spawn once per 250 ms, regardless of
    // cast speed: 199 Life * (2 * 1000 / 250) = 1592 Life per second.
    assert_eq!(out.action_rate, 8.0);
    assert_eq!(out.life_per_second_cost, 199.0 * 8.0);
    let fast = calculate_with_data(
        &build,
        data(),
        &DataOrchestratorOptions {
            extra_modifier_texts: vec!["100% increased Cast Speed".into()],
            ..options
        },
    )
    .unwrap();
    assert_eq!(fast.life_cost, out.life_cost);
    assert_eq!(fast.life_per_second_cost, out.life_per_second_cost);
    assert_eq!(fast.dps, out.dps);
    assert_eq!(fast.combined_dps, out.combined_dps);
}
