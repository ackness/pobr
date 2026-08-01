//! `base/weapon_types.json` loading and value-by-value verification.
//!
//! pobr has no existing weapon-type Rust **data table** (only scattered
//! predicates, see
//! `pobr-build::calc_orchestrator::weapon_type_conditions`), so this table
//! is asserted as a whole against vendor's source of truth
//! `data.weaponTypeInfo`
//! (`vendor/PathOfBuilding-PoE2/src/Modules/Data.lua:532-551`) with
//! hardcoded values line by line; it also checks consistency for the
//! melee/ranged subset pobr's existing predicates already cover — known
//! discrepancies are recorded only, not fixed here.

use pobr_data::catalog::weapon_types::WeaponTypeDef;
use pobr_gamedata::{GameData, repo_data_root};

fn version() -> String {
    pobr_gamedata::data_version()
}

fn game_data() -> GameData {
    GameData::new(repo_data_root().join(version()))
}

fn load() -> Vec<WeaponTypeDef> {
    game_data().weapon_types().expect("weapon_types 可加载")
}

fn find<'a>(table: &'a [WeaponTypeDef], id: &str) -> &'a WeaponTypeDef {
    table
        .iter()
        .find(|w| w.id == id)
        .unwrap_or_else(|| panic!("存在武器类型 {id}"))
}

/// The full table is value-equal to vendor's `data.weaponTypeInfo`
/// (Modules/Data.lua:532-551, 19 entries).
/// Tuple order: (id, one_hand, melee, flag, label).
#[test]
fn full_table_matches_vendor_weapon_type_info() {
    // Hardcoded vendor values, corresponding line by line to Data.lua:533-551.
    #[rustfmt::skip]
    let expected: &[(&str, bool, bool, &str, Option<&str>)] = &[
        ("None",                     true,  true,  "Unarmed",  None),                    // Data.lua:533
        ("Bow",                      false, false, "Bow",      None),                    // Data.lua:534
        ("Crossbow",                 false, false, "Crossbow", None),                    // Data.lua:535
        ("Claw",                     true,  true,  "Claw",     None),                    // Data.lua:536
        ("Dagger",                   true,  true,  "Dagger",   None),                    // Data.lua:537
        ("Spear",                    true,  true,  "Spear",    None),                    // Data.lua:538
        ("Flail",                    true,  true,  "Flail",    None),                    // Data.lua:539
        ("Staff",                    false, true,  "Staff",    Some("Quarterstaff")),    // Data.lua:540
        ("Warstaff",                 false, true,  "Warstaff", None),                    // Data.lua:541
        ("Wand",                     true,  false, "Wand",     None),                    // Data.lua:542
        ("One Hand Axe",             true,  true,  "Axe",      None),                    // Data.lua:543
        ("One Hand Mace",            true,  true,  "Mace",     None),                    // Data.lua:544
        ("One Hand Sword",           true,  true,  "Sword",    None),                    // Data.lua:545
        ("Thrusting One Hand Sword", true,  true,  "Sword",    Some("One Hand Sword")),  // Data.lua:546
        ("Fishing Rod",              false, true,  "Fishing",  None),                    // Data.lua:547
        ("Two Hand Axe",             false, true,  "Axe",      None),                    // Data.lua:548
        ("Two Hand Mace",            false, true,  "Mace",     None),                    // Data.lua:549
        ("Two Hand Sword",           false, true,  "Sword",    None),                    // Data.lua:550
        ("Talisman",                 false, true,  "Talisman", None),                    // Data.lua:551
    ];

    let table = load();
    assert_eq!(
        table.len(),
        expected.len(),
        "vendor weaponTypeInfo 共 19 条"
    );
    for (id, one_hand, melee, flag, label) in expected {
        let w = find(&table, id);
        assert_eq!(w.one_hand, *one_hand, "{id}.one_hand");
        assert_eq!(w.melee, *melee, "{id}.melee");
        assert_eq!(w.flag, *flag, "{id}.flag");
        assert_eq!(w.label.as_deref(), *label, "{id}.label");
    }
}

/// The JSON is sorted by id (a stable diff, matching the base_items convention).
#[test]
fn sorted_by_id_for_stable_diffs() {
    let table = load();
    let mut sorted = table.clone();
    sorted.sort_by(|a, b| a.id.cmp(&b.id));
    assert_eq!(table, sorted, "weapon_types.json 应按 id 排序");
}

/// The subset consistent with pobr's existing melee check: the melee
/// classes matched by `matches!` in
/// `pobr-build::calc_orchestrator::weapon_type_conditions` (Warstaff /
/// 1H·2H Mace·Sword·Axe / Spear / Dagger / Claw / Flail) have
/// `melee = true` in this table; casting implements/bows/crossbows (pobr's
/// comment says "casting implements/bows/crossbows are non-melee") have
/// `melee = false`.
#[test]
fn melee_subset_consistent_with_pobr_weapon_type_conditions() {
    let table = load();
    // pobr's melee `matches!` list (GGG item_class key space; the
    // quarterstaff class is named Warstaff).
    for id in [
        "Warstaff",
        "One Hand Mace",
        "Two Hand Mace",
        "One Hand Sword",
        "Two Hand Sword",
        "One Hand Axe",
        "Two Hand Axe",
        "Spear",
        "Dagger",
        "Claw",
        "Flail",
    ] {
        assert!(find(&table, id).melee, "{id} 应为近战（与 pobr 判定一致）");
    }
    // pobr's non-melee: casting implements/bows/crossbows.
    for id in ["Wand", "Bow", "Crossbow"] {
        assert!(!find(&table, id).melee, "{id} 应为远程（与 pobr 判定一致）");
    }
}

/// Known pobr↔vendor discrepancies (this test pins **vendor's values**;
/// the discrepancy is recorded only, behavior isn't changed here):
/// - TODO(parity): pobr's `weapon_type_conditions` doesn't count
///   Talisman/FishingRod as melee, while vendor has both
///   `Talisman`/`Fishing Rod` as `melee = true` (Data.lua:547,551).
/// - TODO(parity): pobr's `two_handed` predicate treats Bow/Crossbow as
///   "not two-handed", while vendor has both as `oneHand = false`
///   (Data.lua:534-535).
#[test]
fn divergences_pinned_to_vendor_values() {
    let table = load();
    assert!(find(&table, "Talisman").melee, "vendor Data.lua:551");
    assert!(find(&table, "Fishing Rod").melee, "vendor Data.lua:547");
    assert!(!find(&table, "Bow").one_hand, "vendor Data.lua:534");
    assert!(!find(&table, "Crossbow").one_hand, "vendor Data.lua:535");
}

/// A semantic spot check on unarmed (`None`) and the quarterstaff
/// (`Staff`, label=Quarterstaff): unarmed counts as one-handed melee,
/// flag=Unarmed (Data.lua:533); PoE2's quarterstaff base has
/// `type = "Staff"` (Data/Bases/staff.lua:159-167, recorded by GGG's
/// item_class as `Warstaff`).
#[test]
fn unarmed_and_quarterstaff_semantics() {
    let table = load();
    let none = find(&table, "None");
    assert!(none.one_hand && none.melee);
    assert_eq!(none.flag, "Unarmed");
    let staff = find(&table, "Staff");
    assert_eq!(staff.label.as_deref(), Some("Quarterstaff"));
    assert!(!staff.one_hand && staff.melee);
}
