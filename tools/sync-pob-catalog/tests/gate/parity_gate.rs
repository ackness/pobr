//! CI gate: parity check between the PoBR display catalog and a PoB fixture.
//!
//! This test suite uses a committed PoB catalog fixture
//! (`devs/fixtures/pob/parity/pob-catalog.json`) to validate parity for the
//! **Computed** fields declared in `pobr-core::display_catalog`:
//!
//! 1. **`pob_fixture_is_non_empty`**: makes sure the fixture loads correctly
//!    and has enough entries, guarding against an empty or truncated file silently passing.
//! 2. **`all_computed_display_stats_have_known_pob_key`**: every display
//!    stat marked `Computed` in PoBR must find a matching `pob_key` in the
//!    PoB fixture's `output_keys` or `display_stats`. This test fails when a
//!    key is added or renamed without updating the fixture.
//! 3. **`parity_check_rejects_unknown_key`**: unit-verifies that
//!    `check_pobr_parity`'s detection logic itself is correct (a bad key must be reported).

use std::path::PathBuf;

use pobr_core::display_catalog;
use pobr_data::prelude::*;
use sync_pob_catalog::{MissingPobKey, check_pobr_parity, read_catalog};

/// Locate the fixture file path under the repo root.
///
/// The test binary's cwd is the workspace root (cargo test's default
/// behavior); if the fixture is missing, the caller skips (rather than
/// panicking outright, to avoid a false alarm in a CI environment with no fixture).
fn fixture_path() -> Option<PathBuf> {
    let p = PathBuf::from("devs/fixtures/pob/parity/pob-catalog.json");
    if p.exists() { Some(p) } else { None }
}

/// The fixture file must exist and contain >= 100 output_keys (guards against being empty/truncated).
#[test]
fn pob_fixture_is_non_empty() {
    let Some(path) = fixture_path() else {
        eprintln!("SKIP: pob-catalog.json fixture not found (run sync-pob-catalog scan first)");
        return;
    };

    let catalog = read_catalog(&path).expect("fixture must be valid JSON");

    assert!(
        catalog.output_keys.len() >= 100,
        "pob-catalog.json has only {} output_keys — looks truncated or empty (expected ≥100)",
        catalog.output_keys.len()
    );
    assert!(
        catalog.display_stats.len() >= 50,
        "pob-catalog.json has only {} display_stats (expected ≥50)",
        catalog.display_stats.len()
    );
}

/// Every display stat PoBR marks `Computed` must find a matching pob_key in the PoB fixture.
///
/// This is the core of the CI gate: it blocks a new `Computed` mapping from using a key PoB doesn't recognize.
#[test]
fn all_computed_display_stats_have_known_pob_key() {
    let Some(path) = fixture_path() else {
        eprintln!("SKIP: pob-catalog.json fixture not found");
        return;
    };

    let pob_catalog = read_catalog(&path).expect("fixture must be valid JSON");
    let pobr_defs = display_catalog();

    let missing: Vec<MissingPobKey> = check_pobr_parity(&pob_catalog, &pobr_defs);

    assert!(
        missing.is_empty(),
        "PoBR 声明的 {} 个 Computed 字段在 PoB fixture 中找不到对应 pob_key:\n{}",
        missing.len(),
        missing
            .iter()
            .map(|m| format!("  pobr_id={} → pob_key={}", m.pobr_id, m.pob_key))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Validates `check_pobr_parity`'s own correctness: constructs a fake PoB fixture and a def with a bad pob_key.
#[test]
fn parity_check_rejects_unknown_key() {
    // Arrange: a minimal PoB fixture that only knows "TotalDPS"
    let pob_catalog = PobCatalog {
        display_stats: vec![],
        output_keys: vec![PobOutputCatalogEntry {
            key: PobOutputKey::from("TotalDPS"),
            source_files: vec!["CalcOffence.lua".to_string()],
            parity_status: ParityStatus::Planned,
        }],
        breakdowns: vec![],
    };

    // A Computed def with a key that exists in fixture → should pass
    let good_def = DisplayStatDefinition::computed(
        "GoodStat",
        DisplayStatCategory::Offence,
        StatValueType::Number,
    )
    .with_pob_key("TotalDPS");

    // A Computed def with a key the fixture doesn't know → should fail
    let bad_def = DisplayStatDefinition::computed(
        "BadStat",
        DisplayStatCategory::Offence,
        StatValueType::Number,
    )
    .with_pob_key("NoSuchKey");

    // A Planned def with a bad key → should be ignored (only Computed are checked)
    let planned_def = DisplayStatDefinition {
        id: DisplayStatId::from("PlannedStat"),
        pob_key: Some("NoSuchKey".to_string()),
        label: None,
        category: DisplayStatCategory::Misc,
        value_type: StatValueType::Number,
        format: None,
        default_visible: true,
        comparison_visible: false,
        higher_is_better: None,
        breakdown_policy: BreakdownPolicy::Optional,
        parity_status: ParityStatus::Planned,
    };

    let defs = vec![good_def, bad_def, planned_def];

    // Act
    let missing = check_pobr_parity(&pob_catalog, &defs);

    // Assert: only BadStat should be missing
    assert_eq!(
        missing.len(),
        1,
        "only the unknown Computed key must be reported: {missing:?}"
    );
    assert_eq!(missing[0].pobr_id, "BadStat");
    assert_eq!(missing[0].pob_key, "NoSuchKey");
}

/// A `Planned`-status display stat never participates in the parity check (even if pob_key is missing).
#[test]
fn parity_check_ignores_planned_stats() {
    // Arrange: completely empty PoB fixture
    let pob_catalog = PobCatalog {
        display_stats: vec![],
        output_keys: vec![],
        breakdowns: vec![],
    };

    let planned_defs: Vec<DisplayStatDefinition> = vec![
        DisplayStatDefinition {
            id: DisplayStatId::from("A"),
            pob_key: Some("KeyA".to_string()),
            label: None,
            category: DisplayStatCategory::Misc,
            value_type: StatValueType::Number,
            format: None,
            default_visible: true,
            comparison_visible: false,
            higher_is_better: None,
            breakdown_policy: BreakdownPolicy::Optional,
            parity_status: ParityStatus::Planned,
        },
        DisplayStatDefinition {
            id: DisplayStatId::from("B"),
            pob_key: None,
            label: None,
            category: DisplayStatCategory::Misc,
            value_type: StatValueType::Number,
            format: None,
            default_visible: true,
            comparison_visible: false,
            higher_is_better: None,
            breakdown_policy: BreakdownPolicy::Optional,
            parity_status: ParityStatus::Planned,
        },
    ];

    // Act
    let missing = check_pobr_parity(&pob_catalog, &planned_defs);

    // Assert
    assert!(
        missing.is_empty(),
        "Planned stats must not appear in parity failures: {missing:?}"
    );
}
