//! The weapon set in use. A build uses one of its two weapon sets, and
//! modifiers "while using Weapon Set 1" or "2" apply to that set only. An
//! imported build uses the set its active item set names
//! (`useSecondWeaponSet`), unless its main skill is used with the other set:
//! a skill a weapon of that set grants, or a group set to it.

use pobr_build::{
    Build, CharacterIdentity, DataOrchestratorOptions, SocketGroup, WeaponSet,
    calculate_with_data_session, import_build,
};
use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};

use crate::test_data as data;

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

/// A build code holding a staff in each set: the first grants Sigil of Power,
/// the second (held, `useSecondWeaponSet`) raises spell levels. `main` is the
/// main group: 1 lists the granted Sigil of Power, 2 is Fireball.
fn two_staves(main: u32) -> String {
    format!(
        r#"<PathOfBuilding2>
<Build level="90" className="Witch" ascendClassName="None" mainSocketGroup="{main}"/>
<Items activeItemSet="1">
<Item id="1">
Rarity: RARE
Granting Staff
Chiming Staff
Grants Skill: Level 20 Sigil of Power
</Item>
<Item id="2">
Rarity: RARE
Held Staff
Chiming Staff
+5 to Level of all Spell Skills
</Item>
<ItemSet id="1" useSecondWeaponSet="true">
<Slot name="Weapon 1" itemId="1"/>
<Slot name="Weapon 1 Swap" itemId="2"/>
</ItemSet>
</Items>
<Skills activeSkillSet="1"><SkillSet id="1">
<Skill enabled="true" mainActiveSkill="1"><Gem enabled="true" level="20" nameSpec="Sigil of Power" quality="0" skillId="SigilOfPowerPlayer"/></Skill>
<Skill enabled="true" mainActiveSkill="1"><Gem enabled="true" level="20" nameSpec="Fireball" quality="0" skillId="FireballPlayer"/></Skill>
<Skill enabled="true" slot="Weapon 1" source="Item:1:Granting Staff, Chiming Staff"><Gem enabled="true" level="20" nameSpec="Sigil of Power" quality="0" skillId="SigilOfPowerPlayer"/></Skill>
</SkillSet></Skills>
</PathOfBuilding2>"#
    )
}

fn has_line(item: Option<&Item>, line: &str) -> bool {
    item.is_some_and(|item| {
        item.implicit_texts
            .iter()
            .chain(&item.modifier_texts)
            .any(|text| text == line)
    })
}

/// A skill a weapon grants is used holding that weapon: when the main skill
/// is one, the calculation uses the weapon's set, and the skills usable in
/// either set are used in the set the character holds.
#[test]
fn a_main_skill_a_weapon_grants_is_used_in_that_weapons_set() {
    let granted = import_build(&two_staves(1)).expect("imports").build;
    assert_eq!(granted.weapon_set, WeaponSet::First);
    assert!(has_line(
        granted.items.get(&EquipmentSlot::Weapon1),
        "Grants Skill: Level 20 Sigil of Power"
    ));
    assert!(has_line(
        granted.other_weapons.get(&EquipmentSlot::Weapon1),
        "+5 to Level of all Spell Skills"
    ));
    let sets: Vec<_> = granted.socket_groups.iter().map(|g| g.weapon_set).collect();
    assert_eq!(sets, vec![Some(1), Some(2), Some(1)]);

    // Fireball is used in the held set.
    let held = import_build(&two_staves(2)).expect("imports").build;
    assert_eq!(held.weapon_set, WeaponSet::Second);
    assert!(has_line(
        held.items.get(&EquipmentSlot::Weapon1),
        "+5 to Level of all Spell Skills"
    ));
    let sets: Vec<_> = held.socket_groups.iter().map(|g| g.weapon_set).collect();
    assert_eq!(sets, vec![None, None, Some(1)]);
}

/// Enablement is a player choice; the active weapon context gates inspected
/// attacks and overview rows while all-set reservation still reads Blink.
#[test]
fn inactive_groups_keep_reservation_without_becoming_main_or_overview_attacks() {
    use pobr_build::{calculate_skill_metrics, resolve_main_skill_selection};
    let data = data();
    let mut other = SocketGroup::new().with_gem_skill("FireballPlayer", 20);
    other.weapon_set = Some(2);
    let mut blink = SocketGroup::new().with_gem_skill("BlinkReservationPlayer", 20);
    blink.weapon_set = Some(2);
    let mut build = Build::new()
        .add_socket_group(other)
        .add_socket_group(
            SocketGroup::new()
                .with_gem_skill("CometPlayer", 20)
                .with_enabled(false),
        )
        .add_socket_group(SocketGroup::new().with_gem_skill("SparkPlayer", 20))
        .add_socket_group(blink);
    let options = DataOrchestratorOptions::default();
    assert_eq!(
        resolve_main_skill_selection(&build, data),
        Some((2, 1, "SparkPlayer".into()))
    );
    let panel = calculate_with_data_session(&build, data, &options).unwrap();
    assert_eq!(panel.output().spirit_reserved, 60.0);
    let rows = calculate_skill_metrics(&build, data, &options).unwrap();
    assert_eq!(
        rows.skills
            .iter()
            .map(|row| row.group_index)
            .collect::<Vec<_>>(),
        [2]
    );
    assert_eq!(
        rows.skills[0].output.as_ref().unwrap().dps,
        panel.output().dps
    );

    // A stale explicit index cannot silently substitute another skill or use
    // the wrong weapons; it is valid again when that context is selected.
    build.main_socket_group = Some(1);
    assert!(resolve_main_skill_selection(&build, data).is_none());
    build.weapon_set = WeaponSet::Second;
    assert_eq!(
        resolve_main_skill_selection(&build, data),
        Some((0, 1, "FireballPlayer".into()))
    );
    let rows = calculate_skill_metrics(&build, data, &options).unwrap();
    assert_eq!(
        rows.skills
            .iter()
            .map(|row| row.group_index)
            .collect::<Vec<_>>(),
        [0, 2, 3, 3]
    );
    build.socket_groups[3].enabled = false;
    assert_eq!(
        calculate_with_data_session(&build, data, &options)
            .unwrap()
            .output()
            .spirit_reserved,
        0.0
    );
}

#[test]
fn energy_sources_are_evaluated_only_in_their_weapon_context() {
    let data = data();
    let mut meta = SocketGroup::new()
        .with_gem_skill("MetaCastOnCritPlayer", 20)
        .with_gem_skill("CometPlayer", 20);
    meta.main_active_skill = Some(2);
    let mut source = SocketGroup::new().with_gem_skill("SparkPlayer", 20);
    source.weapon_set = Some(2);
    let mut build = Build::new()
        .add_socket_group(meta)
        .add_socket_group(source)
        .with_main_socket_group(1);
    let options = DataOrchestratorOptions {
        extra_modifier_texts: vec!["10000% increased Spell Damage".into()],
        mode_effective: true,
        ..Default::default()
    };
    let first = calculate_with_data_session(&build, data, &options).unwrap();
    assert!(first.output().energy_trigger_source.is_none());
    build.weapon_set = WeaponSet::Second;
    let second = calculate_with_data_session(&build, data, &options).unwrap();
    assert_eq!(
        second.output().energy_trigger_source.as_deref(),
        Some("SparkPlayer")
    );

    // Inactive sources cannot occupy the bounded candidate list and hide a
    // slower source that can actually be used with the current weapons.
    build.weapon_set = WeaponSet::First;
    for _ in 0..5 {
        build.socket_groups.push(build.socket_groups[1].clone());
    }
    build
        .socket_groups
        .push(SocketGroup::new().with_gem_skill("FireballPlayer", 20));
    let crowded = calculate_with_data_session(&build, data, &options).unwrap();
    assert_eq!(
        crowded.output().energy_trigger_source.as_deref(),
        Some("FireballPlayer")
    );
}
