//! Ordinary source rows preserve source ordering and append the explicit legacy
//! compatibility entry; they need not equal the old rounded vendor table.

use pobr_gamedata::GameData;

/// The ordinary source projection keeps all twelve Characters rows before the
/// retained Scion entry. Its duplicate legacy class_id does not reorder source
/// rows or replace target Marauder; the consumer resolves by class name.
#[test]
fn target_source_order_precedes_explicit_legacy_compatibility() {
    let entries = GameData::new(pobr_gamedata::current_data_dir())
        .unarmed_data()
        .expect("current source unarmed data");
    let (legacy, source) = entries.split_last().expect("nonempty source projection");
    assert_eq!(source.len(), 12);
    assert_eq!(
        source
            .iter()
            .map(|entry| entry.class_id)
            .collect::<Vec<_>>(),
        (0..12).collect::<Vec<_>>()
    );
    assert_eq!(source[0].class_name, "Marauder");
    assert_eq!(legacy.class_name, "Scion");
    assert_eq!(
        legacy,
        pobr_data::catalog::unarmed_data::UnarmedDataTable::default()
            .for_class("Scion")
            .unwrap()
    );
    assert!(
        source
            .iter()
            .all(|entry| entry.attack_rate == 1000.0 / 606.0)
    );
}
