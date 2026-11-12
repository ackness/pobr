//! Ordinary source rows preserve source ordering and append the explicit legacy
//! compatibility entry; they need not equal the old rounded vendor table.

use pobr_gamedata::GameData;

/// The ordinary source projection keeps Characters rows before the
/// retained Scion entry. Its duplicate legacy class_id does not reorder source
/// rows or replace target Marauder; the consumer resolves by class name.
#[test]
fn target_source_order_precedes_explicit_legacy_compatibility() {
    let data = GameData::new(pobr_gamedata::current_data_dir());
    let entries = data.unarmed_data().expect("current source unarmed data");
    let (legacy, source) = entries.split_last().expect("nonempty source projection");
    assert!(!source.is_empty());
    assert_eq!(
        source
            .iter()
            .map(|entry| entry.class_id)
            .collect::<Vec<_>>(),
        (0..source.len() as u32).collect::<Vec<_>>()
    );
    assert_eq!(source[0].class_name, "Marauder");
    assert_eq!(legacy.class_name, "Scion");
    let source_classes: std::collections::BTreeSet<_> = source
        .iter()
        .map(|entry| entry.class_name.as_str())
        .collect();
    let tree = data.passive_tree_meta().unwrap();
    let tree_classes: std::collections::BTreeSet<_> = tree
        .classes
        .iter()
        .map(|class| class.name.as_str())
        .collect();
    assert_eq!(
        source_classes, tree_classes,
        "every source class has an unarmed row"
    );
    let mut names = std::collections::BTreeSet::new();
    for entry in &entries {
        assert!(names.insert(&entry.class_name), "duplicate class name");
        assert!(!entry.class_name.is_empty());
        assert!(!entry.weapon_type.is_empty());
        assert!(entry.attack_rate.is_finite() && entry.attack_rate > 0.0);
        assert!(entry.physical_min.is_finite() && entry.physical_min >= 0.0);
        assert!(entry.physical_max.is_finite() && entry.physical_max >= entry.physical_min);
        assert!(
            (0.0..=1.0).contains(&entry.crit_chance),
            "stored crit fraction"
        );
    }
}
