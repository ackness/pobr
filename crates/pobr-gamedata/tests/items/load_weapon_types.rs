//! Loaded weapon rows supply usable conditions and complete classification.

use pobr_data::catalog::WeaponTypeTable;
use pobr_data::modifier::ModFlags;
use pobr_gamedata::{GameData, current_data_dir};

#[test]
fn every_weapon_row_has_conditions_and_generic_classification() {
    let table = WeaponTypeTable(
        GameData::new(current_data_dir())
            .weapon_types()
            .expect("weapon_types should load"),
    );
    assert!(!table.0.is_empty());
    let mut ids = std::collections::HashSet::new();
    for row in &table.0 {
        assert!(ids.insert(&row.id), "duplicate weapon id {}", row.id);
        assert!(!row.flag.trim().is_empty(), "{} needs a flag", row.id);
        assert!(row.conditions().contains(&format!("Using{}", row.flag)));
        assert!(row.damage_keyword().ends_with("Damage"));
        let flags = ModFlags::weapon_flags(&row.id, &row.flag, row.one_hand, row.melee);
        if row.id == "None" {
            assert!(!flags.intersects(ModFlags::WEAPON));
            continue;
        }
        assert!(flags.intersects(ModFlags::WEAPON), "{}", row.id);
        let (grip, other_grip) = if row.one_hand {
            (ModFlags::WEAPON_1H, ModFlags::WEAPON_2H)
        } else {
            (ModFlags::WEAPON_2H, ModFlags::WEAPON_1H)
        };
        let (range, other_range) = if row.melee {
            (ModFlags::WEAPON_MELEE, ModFlags::WEAPON_RANGED)
        } else {
            (ModFlags::WEAPON_RANGED, ModFlags::WEAPON_MELEE)
        };
        assert!(
            flags.intersects(grip) && !flags.intersects(other_grip),
            "{}",
            row.id
        );
        assert!(
            flags.intersects(range) && !flags.intersects(other_range),
            "{}",
            row.id
        );
    }
}

#[test]
fn sorted_by_id_for_stable_diffs() {
    let table = GameData::new(current_data_dir()).weapon_types().unwrap();
    assert!(table.windows(2).all(|pair| pair[0].id < pair[1].id));
}
