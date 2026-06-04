use pobr_core::calc::OutputTable;
use pobr_core::{display_catalog, extract_display_values};
use pobr_data::prelude::*;

#[test]
fn catalog_defines_core_display_stats() {
    let catalog = display_catalog();
    let ids: Vec<&str> = catalog.iter().map(|def| def.id.as_str()).collect();

    assert!(ids.contains(&"TotalDPS"));
    assert!(ids.contains(&"TotalEHP"));
    assert!(ids.contains(&"BleedDPS"));
    assert!(ids.contains(&"BlockChance"));
}

#[test]
fn catalog_entries_are_computed_with_pob_keys() {
    let catalog = display_catalog();
    for def in &catalog {
        assert_eq!(def.parity_status, ParityStatus::Computed);
        assert!(def.pob_key.is_some(), "{} missing pob_key", def.id.as_str());
    }
}

#[test]
fn extract_display_values_reads_output_table_fields() {
    let output = OutputTable {
        dps: 12345.0,
        total_ehp: 9000.0,
        bleed_dps: 250.0,
        block_chance: 35.0,
        ..OutputTable::default()
    };

    let values = extract_display_values(&output);
    let lookup = |id: &str| {
        values
            .iter()
            .find(|value| value.id == DisplayStatId::from(id))
            .map(|value| value.value)
    };

    assert_eq!(lookup("TotalDPS"), Some(12345.0));
    assert_eq!(lookup("TotalEHP"), Some(9000.0));
    assert_eq!(lookup("BleedDPS"), Some(250.0));
    assert_eq!(lookup("BlockChance"), Some(35.0));
}

#[test]
fn extract_covers_every_computed_catalog_entry() {
    let computed = display_catalog()
        .iter()
        .filter(|def| def.parity_status == ParityStatus::Computed)
        .count();
    let values = extract_display_values(&OutputTable::default());
    assert_eq!(values.len(), computed);
}
