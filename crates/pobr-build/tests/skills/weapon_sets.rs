//! The weapon set in use. A build uses one of its two weapon sets, and
//! modifiers "while using Weapon Set 1" or "2" apply to that set only. An
//! imported build uses the set its active item set names
//! (`useSecondWeaponSet`).

use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup, WeaponSet,
    calculate_with_data_session, import_build,
};
use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};
use pobr_gamedata::{GameData, repo_data_root};

fn data() -> BuildData {
    let data = GameData::new(repo_data_root().join(pobr_gamedata::data_version()));
    BuildData::load(&data).expect("load BuildData from repo data")
}

/// A Ranger shooting Ice Shot, supported by Rigwald's Ferocity, from a bow
/// in the weapon set `set`.
fn ice_shot(set: WeaponSet) -> Build {
    let mut build = Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Ranger".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            SocketGroup::new()
                .with_slot("weapon1")
                .with_gem_skill("IceShotPlayer", 20)
                .with_gem_skill("SupportRigwaldsFerocityPlayer", 1),
        )
        .set_item(
            EquipmentSlot::Weapon1,
            Item {
                base: ItemBaseId::from("Crude Bow"),
                rarity: ItemRarity::Normal,
                quality: 0,
                corrupted: false,
                implicit_texts: vec![],
                modifier_texts: vec![],
                enchant_texts: vec![],
                rolled_defence: RolledDefence::default(),
                socketed: Vec::new(),
                parsed_stats: vec![],
            },
        );
    build.weapon_set = set;
    build
}

/// Rigwald's Ferocity: 30% increased Attack Speed and 15% less Damage in
/// the first set, 10% reduced Attack Speed and 30% more Damage in the
/// second.
#[test]
fn supported_skills_are_faster_in_the_first_set_and_stronger_in_the_second() {
    let data = data();
    let options = DataOrchestratorOptions::default();
    let first = calculate_with_data_session(&ice_shot(WeaponSet::First), &data, &options)
        .expect("calculates");
    let second = calculate_with_data_session(&ice_shot(WeaponSet::Second), &data, &options)
        .expect("calculates");

    let speed = first.output().action_rate / second.output().action_rate;
    assert!((speed - 1.30 / 0.90).abs() < 1e-9, "{speed}");
    let damage = second.output().total_hit_avg / first.output().total_hit_avg;
    assert!((damage - 1.30 / 0.85).abs() < 1e-6, "{damage}");
}

/// An imported build uses the weapon set its active item set names.
#[test]
fn an_imported_build_uses_the_set_its_item_set_names() {
    let xml = |second: bool| {
        format!(
            r#"<PathOfBuilding2>
<Build level="90" className="Ranger" ascendClassName="None"/>
<Items activeItemSet="1"><ItemSet id="1" useSecondWeaponSet="{second}"/></Items>
</PathOfBuilding2>"#
        )
    };
    let first = import_build(&xml(false)).expect("imports").build;
    let second = import_build(&xml(true)).expect("imports").build;
    assert_eq!(first.weapon_set, WeaponSet::First);
    assert_eq!(second.weapon_set, WeaponSet::Second);
}
