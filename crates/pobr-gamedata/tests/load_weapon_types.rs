//! `base/weapon_types.json` 加载与逐值校验。
//!
//! pobr 无既有的武器类型 Rust **数据表**（仅散落谓词，见
//! `pobr-build::calc_orchestrator::weapon_type_conditions`），故本表整体取
//! vendor 准源 `data.weaponTypeInfo`
//! （`vendor/PathOfBuilding-PoE2/src/Modules/Data.lua:532-551`）逐条写死断言；
//! 另对 pobr 现有谓词覆盖的近战/远程子集做一致性断言，已知出入只记录不改值。

use pobr_data::catalog::weapon_types::WeaponTypeDef;
use pobr_gamedata::{GameData, repo_data_root};

const VERSION: &str = "4.5.0.3.4";

fn game_data() -> GameData {
    GameData::new(repo_data_root().join(VERSION))
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

/// 全表逐值等于 vendor `data.weaponTypeInfo`（Modules/Data.lua:532-551，19 条）。
/// 元组序：(id, one_hand, melee, flag, label)。
#[test]
fn full_table_matches_vendor_weapon_type_info() {
    // 写死的 vendor 值，逐行对应 Data.lua:533-551。
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

/// JSON 按 id 排序（稳定 diff，对齐 base_items 约定）。
#[test]
fn sorted_by_id_for_stable_diffs() {
    let table = load();
    let mut sorted = table.clone();
    sorted.sort_by(|a, b| a.id.cmp(&b.id));
    assert_eq!(table, sorted, "weapon_types.json 应按 id 排序");
}

/// 与 pobr 现有近战判定一致的子集：
/// `pobr-build::calc_orchestrator::weapon_type_conditions` 的 matches! 近战类
/// （Warstaff / 1H·2H Mace·Sword·Axe / Spear / Dagger / Claw / Flail）在本表
/// `melee = true`；法器/弓/弩（pobr 注释「法器/弓/弩为非近战」）`melee = false`。
#[test]
fn melee_subset_consistent_with_pobr_weapon_type_conditions() {
    let table = load();
    // pobr 近战 matches! 列表（GGG item_class 键空间；长杖类名 Warstaff）。
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
    // pobr 非近战：法器/弓/弩。
    for id in ["Wand", "Bow", "Crossbow"] {
        assert!(!find(&table, id).melee, "{id} 应为远程（与 pobr 判定一致）");
    }
}

/// 已知 pobr↔vendor 出入（本测试钉住 **vendor 值**，出入只记录不改行为）：
/// - TODO(parity): pobr `weapon_type_conditions` 不把 Talisman/FishingRod 计为
///   近战，vendor `Talisman`/`Fishing Rod` 均 `melee = true`（Data.lua:547,551）。
/// - TODO(parity): pobr `two_handed` 谓词视 Bow/Crossbow 为「非双手」，vendor
///   两者均 `oneHand = false`（Data.lua:534-535）。
#[test]
fn divergences_pinned_to_vendor_values() {
    let table = load();
    assert!(find(&table, "Talisman").melee, "vendor Data.lua:551");
    assert!(find(&table, "Fishing Rod").melee, "vendor Data.lua:547");
    assert!(!find(&table, "Bow").one_hand, "vendor Data.lua:534");
    assert!(!find(&table, "Crossbow").one_hand, "vendor Data.lua:535");
}

/// 空手（`None`）与长杖（`Staff`，label=Quarterstaff）的语义抽样：
/// 空手计单手近战、flag=Unarmed（Data.lua:533）；PoE2 长杖基底 `type = "Staff"`
/// （Data/Bases/staff.lua:159-167，GGG item_class 记为 `Warstaff`）。
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
