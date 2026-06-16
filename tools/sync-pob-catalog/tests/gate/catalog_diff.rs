//! Integration tests for catalog diffing and self-contained fixtures.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use pobr_data::prelude::*;
use sync_pob_catalog::{
    CatalogDiff, DiffKind, check_against_fixture, diff_catalogs, write_self_contained_fixture,
};

fn output_entry(key: &str, source: &str, parity: ParityStatus) -> PobOutputCatalogEntry {
    PobOutputCatalogEntry {
        key: PobOutputKey::from(key),
        source_files: vec![source.to_string()],
        parity_status: parity,
    }
}

fn catalog_with(entries: Vec<PobOutputCatalogEntry>) -> PobCatalog {
    PobCatalog {
        display_stats: Vec::new(),
        output_keys: entries,
        breakdowns: Vec::new(),
    }
}

#[test]
fn empty_catalogs_have_no_diff() {
    // Arrange
    let expected = catalog_with(Vec::new());
    let actual = catalog_with(Vec::new());

    // Act
    let diffs = diff_catalogs(&expected, &actual);

    // Assert
    assert!(
        diffs.is_empty(),
        "two empty catalogs must not diff: {diffs:?}"
    );
}

#[test]
fn identical_catalogs_have_no_diff() {
    // Arrange
    let cat = catalog_with(vec![
        output_entry("TotalDPS", "CalcOffence.lua", ParityStatus::Computed),
        output_entry("FireResist", "CalcDefence.lua", ParityStatus::Planned),
    ]);

    // Act
    let diffs = diff_catalogs(&cat, &cat);

    // Assert
    assert!(
        diffs.is_empty(),
        "identical catalogs must not diff: {diffs:?}"
    );
}

#[test]
fn detects_added_output_key() {
    // Arrange: actual has a key the expected catalog lacks.
    let expected = catalog_with(vec![output_entry(
        "TotalDPS",
        "CalcOffence.lua",
        ParityStatus::Computed,
    )]);
    let actual = catalog_with(vec![
        output_entry("TotalDPS", "CalcOffence.lua", ParityStatus::Computed),
        output_entry("NewStat", "CalcOffence.lua", ParityStatus::Planned),
    ]);

    // Act
    let diffs = diff_catalogs(&expected, &actual);

    // Assert
    assert_eq!(diffs.len(), 1);
    assert_eq!(diffs[0].kind, DiffKind::Added);
    assert_eq!(diffs[0].key, "NewStat");
}

#[test]
fn detects_removed_output_key() {
    // Arrange: expected has a key that actual dropped.
    let expected = catalog_with(vec![
        output_entry("TotalDPS", "CalcOffence.lua", ParityStatus::Computed),
        output_entry("GoneStat", "CalcOffence.lua", ParityStatus::Planned),
    ]);
    let actual = catalog_with(vec![output_entry(
        "TotalDPS",
        "CalcOffence.lua",
        ParityStatus::Computed,
    )]);

    // Act
    let diffs = diff_catalogs(&expected, &actual);

    // Assert
    assert_eq!(diffs.len(), 1);
    assert_eq!(diffs[0].kind, DiffKind::Removed);
    assert_eq!(diffs[0].key, "GoneStat");
}

#[test]
fn detects_status_changed() {
    // Arrange: same key, different parity status.
    let expected = catalog_with(vec![output_entry(
        "TotalDPS",
        "CalcOffence.lua",
        ParityStatus::Planned,
    )]);
    let actual = catalog_with(vec![output_entry(
        "TotalDPS",
        "CalcOffence.lua",
        ParityStatus::Computed,
    )]);

    // Act
    let diffs = diff_catalogs(&expected, &actual);

    // Assert
    assert_eq!(diffs.len(), 1);
    assert_eq!(diffs[0].kind, DiffKind::StatusChanged);
    assert_eq!(diffs[0].key, "TotalDPS");
    assert!(diffs[0].detail.contains("Planned"));
    assert!(diffs[0].detail.contains("Computed"));
}

#[test]
fn detects_source_changed() {
    // Arrange: same key + status, different source files.
    let expected = catalog_with(vec![output_entry(
        "TotalDPS",
        "CalcOffence.lua",
        ParityStatus::Computed,
    )]);
    let actual = catalog_with(vec![output_entry(
        "TotalDPS",
        "Calcs.lua",
        ParityStatus::Computed,
    )]);

    // Act
    let diffs = diff_catalogs(&expected, &actual);

    // Assert
    assert_eq!(diffs.len(), 1);
    assert_eq!(diffs[0].kind, DiffKind::SourceChanged);
    assert_eq!(diffs[0].key, "TotalDPS");
}

#[test]
fn diffs_are_sorted_by_key() {
    // Arrange
    let expected = catalog_with(vec![output_entry(
        "Keep",
        "CalcOffence.lua",
        ParityStatus::Computed,
    )]);
    let actual = catalog_with(vec![
        output_entry("Keep", "CalcOffence.lua", ParityStatus::Computed),
        output_entry("Zebra", "CalcOffence.lua", ParityStatus::Planned),
        output_entry("Alpha", "CalcOffence.lua", ParityStatus::Planned),
    ]);

    // Act
    let diffs = diff_catalogs(&expected, &actual);

    // Assert
    let keys: Vec<&str> = diffs.iter().map(|d| d.key.as_str()).collect();
    assert_eq!(keys, vec!["Alpha", "Zebra"]);
}

#[test]
fn fixture_roundtrip_has_no_diff() {
    // Arrange
    let path = temp_path("roundtrip");
    let cat = catalog_with(vec![
        output_entry("TotalDPS", "CalcOffence.lua", ParityStatus::Computed),
        output_entry("FireResist", "CalcDefence.lua", ParityStatus::Planned),
    ]);

    // Act
    write_self_contained_fixture(&cat, &path).unwrap();
    let diffs = check_against_fixture(&path, &cat).unwrap();

    // Assert
    assert!(
        diffs.is_empty(),
        "write+check roundtrip must not diff: {diffs:?}"
    );

    let _ = fs::remove_file(&path);
}

#[test]
fn fixture_check_detects_drift() {
    // Arrange: write one catalog, then check a drifted one against the fixture.
    let path = temp_path("drift");
    let baseline = catalog_with(vec![output_entry(
        "TotalDPS",
        "CalcOffence.lua",
        ParityStatus::Planned,
    )]);
    write_self_contained_fixture(&baseline, &path).unwrap();

    let drifted = catalog_with(vec![
        output_entry("TotalDPS", "CalcOffence.lua", ParityStatus::Computed),
        output_entry("BrandNew", "CalcOffence.lua", ParityStatus::Planned),
    ]);

    // Act
    let diffs = check_against_fixture(&path, &drifted).unwrap();

    // Assert
    assert!(
        diffs
            .iter()
            .any(|d| d.kind == DiffKind::StatusChanged && d.key == "TotalDPS"),
        "drift in status must be detected: {diffs:?}"
    );
    assert!(
        diffs
            .iter()
            .any(|d| d.kind == DiffKind::Added && d.key == "BrandNew"),
        "added key must be detected: {diffs:?}"
    );

    let _ = fs::remove_file(&path);
}

#[test]
fn catalog_diff_is_constructible_and_comparable() {
    // Arrange / Act
    let diff = CatalogDiff {
        kind: DiffKind::Added,
        key: "Foo".to_string(),
        detail: "added".to_string(),
    };

    // Assert
    assert_eq!(diff.kind, DiffKind::Added);
    assert_eq!(diff.key, "Foo");
}

fn temp_path(tag: &str) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("sync-pob-catalog-{tag}-{now}.json"))
}
