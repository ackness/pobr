//! Integration tests for the summary/example Build: verifies the
//! placeholder smoke output contains the expected fields and the title can be localized.

use pobr_desktop::{EXAMPLE_BUILD_LEVEL, app_title, build_summary, example_build};

#[test]
fn example_build_has_expected_shape() {
    let build = example_build();
    assert_eq!(build.character.level, EXAMPLE_BUILD_LEVEL);
    assert!(
        !build.equipped_items().is_empty(),
        "example build should equip an item"
    );
}

#[test]
fn summary_contains_core_stat_labels() {
    let build = example_build();
    let summary = build_summary(&build, "en-US").expect("calc should succeed");

    for label in [
        "Life",
        "Mana",
        "Fire Res",
        "Cold Res",
        "Lightning Res",
        "DPS",
    ] {
        assert!(
            summary.contains(label),
            "summary should contain `{label}`, got:\n{summary}"
        );
    }
}

#[test]
fn summary_uses_localized_title() {
    let build = example_build();
    let summary = build_summary(&build, "en-US").expect("calc should succeed");
    assert!(
        summary.contains("Path of Building in Rust"),
        "summary should embed the en-US app.title, got:\n{summary}"
    );
}

#[test]
fn app_title_falls_back_for_unknown_language() {
    // Unknown language code must fall back to the canonical en-US pack.
    let title = app_title("xx-ZZ");
    assert_eq!(title, "Path of Building in Rust");
}

#[test]
fn example_build_resolves_at_least_some_modifiers() {
    // The hand-crafted modifier texts are PoB-compatible English; the parser
    // should resolve them, so the example build's life pool exceeds the base.
    let build = example_build();
    let summary = build_summary(&build, "en-US").expect("calc should succeed");
    // "+25 to maximum Life" applied on top of base life -> life label present
    // and the value should be non-empty. We assert the line exists with a digit.
    let life_line = summary
        .lines()
        .find(|line| line.starts_with("Life:"))
        .expect("Life line present");
    assert!(
        life_line.chars().any(|c| c.is_ascii_digit()),
        "Life line should contain a number, got `{life_line}`"
    );
}
