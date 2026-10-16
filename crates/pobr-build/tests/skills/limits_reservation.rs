//! Cancellable reservation, duration modifier, Full Life threshold and Strike targets
//! on complete builds.
use std::sync::OnceLock;

use pobr_build::{
    BuildData, DataOrchestratorOptions, calculate_with_data, parse_build, parse_build_from_code,
};
use pobr_core::calc::OutputTable;
use pobr_core::display_value;
use pobr_gamedata::{GameData, data_version, repo_data_root};

fn data() -> &'static BuildData {
    static DATA: OnceLock<BuildData> = OnceLock::new();
    DATA.get_or_init(|| {
        BuildData::load(&GameData::new(repo_data_root().join(data_version()))).unwrap()
    })
}

fn calculate(skills: &str, modifiers: &[&str]) -> OutputTable {
    let xml = format!(
        r#"<PathOfBuilding2><Build level="80" className="Witch" mainSocketGroup="1"/><Skills>{skills}</Skills></PathOfBuilding2>"#
    );
    let build = parse_build(&xml).unwrap();
    calculate_with_data(
        &build,
        data(),
        &DataOrchestratorOptions {
            extra_modifier_texts: modifiers.iter().map(|text| text.to_string()).collect(),
            mode_effective: true,
            ..Default::default()
        },
    )
    .unwrap()
}

const FIREBALL: &str =
    r#"<Skill mainActiveSkill="1"><Gem skillId="FireballPlayer" level="20"/></Skill>"#;

#[test]
fn a_permanent_life_reservation_is_reserved_but_not_cancellable() {
    let plain = calculate(FIREBALL, &[]);
    assert_eq!(display_value(&plain, "LifeCancellableReservation"), 100.0);
    assert_eq!(display_value(&plain, "ManaCancellableReservation"), 100.0);
    // "Reserves 25% of Life" is a permanent quarter of maximum Life, and
    // Reservation Efficiency does not change it.
    let reserved = calculate(
        FIREBALL,
        &[
            "Reserves 25% of Life",
            "50% increased Reservation Efficiency of Skills",
        ],
    );
    assert_eq!(display_value(&reserved, "LifeCancellableReservation"), 75.0);
    assert_eq!(
        display_value(&reserved, "ManaCancellableReservation"),
        100.0
    );
    let maximum = reserved.life_reserved + reserved.life_unreserved;
    assert!(
        (reserved.life_reserved - 0.25 * maximum).abs() < 1.0,
        "{} of {maximum}",
        reserved.life_reserved
    );
    assert!(plain.life_reserved < reserved.life_reserved);
}

#[test]
fn duration_mod_is_the_duration_calculations_modifier() {
    let plain = calculate(FIREBALL, &[]);
    assert_eq!(display_value(&plain, "DurationMod"), 1.0);
    let longer = calculate(FIREBALL, &["40% increased Skill Effect Duration"]);
    assert!((display_value(&longer, "DurationMod") - 1.4).abs() < 1e-9);
}

#[test]
fn strike_targets_are_zero_for_a_spell_and_the_ancestral_call_boost_adds_two() {
    // A spell is not a Strike.
    assert_eq!(
        display_value(&calculate(FIREBALL, &[]), "StrikeTargets"),
        0.0
    );
    // Invoker Flicker Strike (tuned build): the Strike hits one enemy and its
    // Ancestral Call support's boosted Strike targets 2 additional Enemies.
    let build = parse_build_from_code(include_str!(
        "../../../../examples/reference-builds/tuned/builds/ninja-2026-10-13-monk-invoker-flicker-strike-e3b8bfa3a005/code.txt"
    ))
    .unwrap();
    let out = calculate_with_data(&build, data(), &DataOrchestratorOptions::default()).unwrap();
    assert_eq!(display_value(&out, "StrikeTargets"), 3.0);
}
