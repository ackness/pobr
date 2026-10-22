//! Real item/jewel source tests for current trade affixes, not parser-only probes.
use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup,
    calculate_with_data_session,
};
use pobr_core::calc::{CompletedCalculationSession, MinimalInput};
use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};
use pobr_data::monster::EnemyTier;

use crate::test_data as data;

fn item(base: &str, mods: &[&str]) -> Item {
    Item {
        base: ItemBaseId::from(base),
        rarity: ItemRarity::Rare,
        quality: 0,
        corrupted: false,
        implicit_texts: vec![],
        modifier_texts: mods.iter().map(|s| (*s).into()).collect(),
        enchant_texts: vec![],
        rolled_defence: RolledDefence::default(),
        socketed: Vec::new(),
        parsed_stats: vec![],
    }
}

fn build(skill: &str) -> Build {
    Build::new()
        .with_character(CharacterIdentity {
            level: 85,
            class_name: "Ranger".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(SocketGroup::new().with_active_skill(skill, 16))
}

fn run(build: &Build, data: &BuildData) -> CompletedCalculationSession {
    calculate_with_data_session(
        build,
        data,
        &DataOrchestratorOptions {
            base_input: MinimalInput::default(),
            inject_character_base: true,
            enemy_tier: EnemyTier::None,
            mode_effective: false,
            ..Default::default()
        },
    )
    .unwrap()
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-7, "{actual} != {expected}");
}

#[test]
fn granted_jewel_sockets_follow_selected_tree_data_after_renumbering() {
    let mut data = data().clone();
    let mut nodes = std::collections::HashMap::new();
    for (skill, id, name) in [
        (900001, "voices_jewel_slot1__", "Sinister Jewel Socket"),
        (900002, "voices_jewel_slot2", "Sinister Jewel Socket"),
        (900003, "future_named_socket", "Future Gift"),
    ] {
        let node = serde_json::from_value(serde_json::json!({
            "skill": skill, "id": id, "name": name, "kind": "jewel_socket"
        }))
        .unwrap();
        nodes.insert(skill, node);
    }
    data.versioned_passive_nodes
        .insert("future_tree".into(), nodes);
    let xml = |allocated: &str, count: usize, grant: &str, version: &str| {
        format!(
            r#"<PathOfBuilding2><Build level="85" className="Warrior"/>
        <Tree activeSpec="1"><Spec nodes="{allocated}" treeVersion="{version}"><Sockets>
        <Socket nodeId="1234" itemId="1"/><Socket nodeId="900001" itemId="2"/>
        <Socket nodeId="900002" itemId="3"/><Socket nodeId="900003" itemId="4"/>
        </Sockets></Spec></Tree><Items activeItemSet="1">
        <Item id="1">Rarity: UNIQUE
Voices
Diamond
Implicits: 0
Allocates {count} Sinister Jewel sockets</Item>
        <Item id="2">Rarity: MAGIC
Ruby
Implicits: 0
+11 to maximum Life</Item>
        <Item id="3">Rarity: MAGIC
Ruby
Implicits: 0
+23 to maximum Life</Item>
        <Item id="4">Rarity: MAGIC
Ruby
Implicits: 0
+47 to maximum Life</Item>
        <Item id="5">Rarity: RARE
Test Amulet
Amber Amulet
Implicits: 0
{grant}</Item>
        <ItemSet id="1"><Slot name="Amulet" itemId="5"/></ItemSet>
        </Items></PathOfBuilding2>"#
        )
    };
    let life = |allocated, count, grant, version| {
        let build = pobr_build::parse_build(&xml(allocated, count, grant, version)).unwrap();
        run(&build, &data).output().life
    };
    let baseline = life("1234", 0, "", "future_tree");
    let mut direct = pobr_build::parse_build(&xml("1234", 0, "", "future_tree")).unwrap();
    direct.jewels.push((
        pobr_build::build::JewelSocket::Tree,
        item("Ruby", &["+100 to maximum Life"]),
    ));
    let life_scale = (run(&direct, &data).output().life - baseline) / 100.0;
    assert!(life_scale > 0.0);
    close(
        life("1234", 1, "", "future_tree") - baseline,
        11.0 * life_scale,
    );
    close(
        life("1234", 99, "", "future_tree") - baseline,
        34.0 * life_scale,
    );
    close(life("", 2, "", "future_tree"), baseline);
    close(
        life("1234", 2, "Allocates Future Gift", "future_tree") - baseline,
        81.0 * life_scale,
    );
    close(
        life("1234", 2, "Allocates Future Gift", "unknown_tree"),
        baseline,
    );
    // A jewel allocates a socket as an item does (a Megalomaniac's
    // "Allocates Zarokh's Gift").
    let by_jewel = xml("1234", 2, "", "future_tree").replace(
        "Allocates 2 Sinister Jewel sockets",
        "Allocates 2 Sinister Jewel sockets\nAllocates Future Gift",
    );
    close(
        run(&pobr_build::parse_build(&by_jewel).unwrap(), &data)
            .output()
            .life
            - baseline,
        81.0 * life_scale,
    );
    // An inactive tree can reuse the same socket with another item. Neither
    // XML calculation nor the editable view may mix its jewels into this set.
    let first = xml("1234", 1, "", "future_tree").replace(
        "</Spec></Tree>",
        "</Spec><Spec nodes=\"900001\" treeVersion=\"future_tree\"><Sockets><Socket nodeId=\"900001\" itemId=\"4\"/></Sockets></Spec></Tree>",
    );
    close(
        run(&pobr_build::parse_build(&first).unwrap(), &data)
            .output()
            .life
            - baseline,
        11.0 * life_scale,
    );
    let second = first.replace("activeSpec=\"1\"", "activeSpec=\"2\"");
    close(
        run(&pobr_build::parse_build(&second).unwrap(), &data)
            .output()
            .life
            - baseline,
        47.0 * life_scale,
    );
}

#[test]
fn body_armour_granted_crit_reduction_requires_normal_chest() {
    {
        let version = pobr_gamedata::data_version();
        let data = data();
        for rarity in [
            None,
            Some(ItemRarity::Normal),
            Some(ItemRarity::Magic),
            Some(ItemRarity::Rare),
            Some(ItemRarity::Unique),
        ] {
            let mut baseline = build("SparkPlayer");
            if let Some(rarity) = rarity {
                let mut chest = item("Plate Vest", &[]);
                chest.rarity = rarity;
                baseline = baseline.set_item(EquipmentSlot::BodyArmour, chest);
            }
            let base = run(&baseline, &data);
            for percent in [100, 0, 1, 50, 99, 150] {
                let line = format!(
                    "Body Armour grants Hits against you have {percent}% reduced Critical Damage Bonus"
                );
                let parsed = pobr_core::mod_parser::ParseCtx::with_engine(
                    data.parser_rules.as_deref().unwrap(),
                )
                .parse(&line)
                .unwrap();
                assert_eq!(
                    parsed.special_meta.unwrap().entry_id,
                    "body_armour_grants_reduced_crit_damage_bonus_100"
                );
                let imported = pobr_core::parse_pob_xml_item(&format!(
                    "Rarity: RARE\nRule Acceptance Ring\nSapphire Ring\nImplicits: 0\n{line}"
                ))
                .unwrap();
                let result = run(
                    &baseline.clone().set_item(EquipmentSlot::Ring1, imported),
                    &data,
                );
                let expected = if rarity == Some(ItemRarity::Normal) {
                    f64::from(percent.min(100))
                } else {
                    0.0
                };
                close(result.output().crit_extra_damage_reduction, expected);
                close(result.output().life, base.output().life);
                close(result.output().dps, base.output().dps);
                if expected > 0.0 {
                    assert!(result.output().total_ehp > base.output().total_ehp);
                } else {
                    close(result.output().total_ehp, base.output().total_ehp);
                }
                assert!(result.unsupported_modifier_texts().is_empty());
                let modifiers = result.mods_named("ReduceCritExtraDamage");
                assert_eq!(modifiers.len(), 1, "{version}: {line}");
                assert_eq!(modifiers[0].value.as_number(), Some(f64::from(percent)));
                let origin = modifiers[0].origin.as_ref().unwrap();
                assert_eq!(
                    origin.source_id.kind,
                    pobr_data::source::SourceKind::ItemAffix
                );
                assert_eq!(origin.source_id.id, "item.ring1.explicit");
                assert_eq!(origin.slot.as_deref(), Some("ring1"));
                assert_eq!(origin.raw_text.as_deref(), Some(line.as_str()));
            }
        }
    }
}

#[test]
fn extracted_ring_effects_preserve_left_and_right_slot_scaling() {
    let data = data();
    let baseline = build("SparkPlayer")
        .set_item(
            EquipmentSlot::Ring1,
            item("Sapphire Ring", &["+100 to maximum Life"]),
        )
        .set_item(
            EquipmentSlot::Ring2,
            item("Ruby Ring", &["+200 to maximum Life"]),
        );
    let base = run(&baseline, &data);
    for (line, extra_life) in [
        ("27% increased bonuses gained from left Equipped Ring", 27.0),
        (
            "21% increased bonuses gained from right Equipped Ring",
            42.0,
        ),
    ] {
        let result = run(
            &baseline
                .clone()
                .set_item(EquipmentSlot::Belt, item("Linen Belt", &[line])),
            &data,
        );
        assert!(
            result.unsupported_modifier_texts().is_empty(),
            "{:?}",
            result.unsupported_modifier_texts()
        );
        close(result.output().life - base.output().life, extra_life);
    }
}

#[test]
fn ordinary_jewel_scales_quiver_like_other_global_sources() {
    let data = data();
    let baseline = build("IceShotPlayer")
        .set_item(
            EquipmentSlot::Weapon1,
            item("Twin Bow", &["Adds 30 to 60 Physical Damage"]),
        )
        .set_item(
            EquipmentSlot::Weapon2,
            item(
                "Primed Quiver",
                &[
                    "Adds 20 to 40 Physical Damage to Attacks",
                    "+100 to maximum Life",
                ],
            ),
        );
    let base = run(&baseline, &data);
    let line = "6% increased bonuses gained from Equipped Quiver";
    // Build.jewels contains the already validated/allocated socket sources.
    let jewel = item("Emerald", &[line]);
    let from_jewel = run(&baseline.clone().with_jewels(vec![jewel.clone()]), &data);
    let from_ring = run(
        &baseline
            .clone()
            .set_item(EquipmentSlot::Ring1, item("Sapphire Ring", &[line])),
        &data,
    );
    assert!(from_jewel.output().dps > base.output().dps);
    assert!(from_jewel.output().life > base.output().life);
    close(from_jewel.output().dps, from_ring.output().dps);
    close(from_jewel.output().life, from_ring.output().life);

    let mut magic = jewel;
    magic.rarity = ItemRarity::Magic;
    magic.corrupted = true;
    let mut adorned = item(
        "Diamond",
        &["100% increased Effect of Jewel Socket Passive Skills containing Corrupted Magic Jewels"],
    );
    adorned.rarity = ItemRarity::Unique;
    let amplified = run(&baseline.clone().with_jewels(vec![magic, adorned]), &data);
    let doubled = run(
        &baseline.clone().set_item(
            EquipmentSlot::Ring1,
            item(
                "Sapphire Ring",
                &["12% increased bonuses gained from Equipped Quiver"],
            ),
        ),
        &data,
    );
    close(amplified.output().dps, doubled.output().dps);
    close(amplified.output().life, doubled.output().life);
    let no_quiver = baseline.set_item(EquipmentSlot::Weapon2, item("Crystal Focus", &[]));
    close(
        run(
            &no_quiver
                .clone()
                .with_jewels(vec![item("Emerald", &[line])]),
            &data,
        )
        .output()
        .dps,
        run(&no_quiver, &data).output().dps,
    );
}

#[test]
fn every_stun_threshold_roll_uses_final_energy_shield() {
    let data = data();
    let baseline = build("SparkPlayer").set_item(
        EquipmentSlot::Helmet,
        item("Circlet", &["+1001 to maximum Energy Shield"]),
    );
    let base = run(&baseline, &data);
    assert!(base.output().energy_shield >= 1001.0);
    for percent in [2, 7, 8, 12, 13, 15, 20, 30] {
        let line =
            format!("Gain additional Stun Threshold equal to {percent}% of maximum Energy Shield");
        let result = run(
            &baseline
                .clone()
                .with_jewels(vec![item("Sapphire", &[&line])]),
            &data,
        );
        assert!(result.unsupported_modifier_texts().is_empty(), "{line}");
        close(
            result.output().stun_threshold,
            base.output().stun_threshold
                + (base.output().energy_shield * percent as f64 / 100.0).ceil(),
        );
        close(result.output().dps, base.output().dps);
        close(result.output().life, base.output().life);
    }
    // PoB2 scales/truncates the mod's BASE=1, leaving PercentStat.percent
    // unchanged. In particular, 50% Adorned keeps BASE=1 (not 8% -> 12%).
    for percent in [2, 8, 13] {
        for (effect, scaled_base) in [(50, 1), (100, 2)] {
            let line = format!(
                "Gain additional Stun Threshold equal to {percent}% of maximum Energy Shield"
            );
            let mut magic = item("Sapphire", &[&line]);
            magic.rarity = ItemRarity::Magic;
            magic.corrupted = true;
            let mut adorned = item(
                "Diamond",
                &[
                    &format!("{effect}% increased Effect of Jewel Socket Passive Skills"),
                    "containing Corrupted Magic Jewels",
                ],
            );
            adorned.rarity = ItemRarity::Unique;
            let result = run(&baseline.clone().with_jewels(vec![magic, adorned]), &data);
            assert!(
                result.unsupported_modifier_texts().is_empty(),
                "wrapped Adorned must use its dedicated consumer: {:?}",
                result.unsupported_modifier_texts()
            );
            close(
                result.output().stun_threshold,
                base.output().stun_threshold
                    + (base.output().energy_shield * percent as f64 * scaled_base as f64 / 100.0)
                        .ceil(),
            );
            close(result.output().life, base.output().life);
            close(result.output().dps, base.output().dps);
        }
    }
}

/// The Adorned names one jewel rarity. It scales the corrupted jewels of that
/// rarity and no others; the rare form used to be two unsupported lines.
#[test]
fn the_adorned_scales_corrupted_jewels_of_the_rarity_it_names() {
    let data = data();
    let baseline = build("SparkPlayer");
    let life = |jewels: Vec<Item>| {
        let result = run(&baseline.clone().with_jewels(jewels), &data);
        assert!(
            result.unsupported_modifier_texts().is_empty(),
            "{:?}",
            result.unsupported_modifier_texts()
        );
        result.output().life
    };
    let jewel = |rarity: ItemRarity, corrupted: bool| {
        let mut jewel = item("Sapphire", &["+100 to maximum Life"]);
        jewel.rarity = rarity;
        jewel.corrupted = corrupted;
        jewel
    };
    let adorned = |named: &str| {
        let mut adorned = item(
            "Diamond",
            &[
                "50% increased Effect of Jewel Socket Passive Skills",
                &format!("containing Corrupted {named} Jewels"),
            ],
        );
        adorned.rarity = ItemRarity::Unique;
        adorned
    };

    let without = life(vec![]);
    for (named, scaled, other) in [
        ("Rare", ItemRarity::Rare, ItemRarity::Magic),
        ("Magic", ItemRarity::Magic, ItemRarity::Rare),
    ] {
        let plain = life(vec![jewel(scaled, true)]) - without;
        assert!(plain > 0.0);
        close(
            life(vec![jewel(scaled, true), adorned(named)]) - without,
            plain * 1.5,
        );
        // Not corrupted, or the other rarity: unchanged.
        close(
            life(vec![jewel(scaled, false), adorned(named)]) - without,
            plain,
        );
        close(
            life(vec![jewel(other, true), adorned(named)]) - without,
            plain,
        );
    }

    // Only the tree's own sockets are "Jewel Socket Passive Skills": a jewel
    // in a socket something grants, or in a socket on equipment, is not scaled.
    let plain = life(vec![jewel(ItemRarity::Rare, true)]) - without;
    for socket in [
        pobr_build::build::JewelSocket::Granted,
        pobr_build::build::JewelSocket::Equipment,
    ] {
        let mut build = baseline.clone().with_jewels(vec![adorned("Rare")]);
        build.jewels.push((socket, jewel(ItemRarity::Rare, true)));
        close(run(&build, &data).output().life - without, plain);
    }
}

/// The Lich's Crystalline Phylactery holds a non-unique jewel and grants
/// "100% increased Effect of bonuses gained from Socketed Jewel": the jewel in
/// it counts twice. The passive is not a "Jewel Socket Passive Skill", so The
/// Adorned does not scale that jewel.
#[test]
fn the_crystalline_phylactery_doubles_the_jewel_it_holds() {
    const CRYSTALLINE_PHYLACTERY: u32 = 17788;
    let data = data();
    let baseline = build("SparkPlayer");
    let life = |build: &Build| run(build, &data).output().life;
    let without = life(&baseline);
    let mut jewel = item("Sapphire", &["+100 to maximum Life"]);
    jewel.rarity = ItemRarity::Rare;
    jewel.corrupted = true;
    let in_a_socket = life(&baseline.clone().with_jewels(vec![jewel.clone()])) - without;
    assert!(in_a_socket > 0.0);

    let mut phylactery = baseline.clone();
    phylactery.jewels.push((
        pobr_build::build::JewelSocket::Node(CRYSTALLINE_PHYLACTERY),
        jewel,
    ));
    close(life(&phylactery) - without, in_a_socket * 2.0);

    let mut adorned = item(
        "Diamond",
        &[
            "50% increased Effect of Jewel Socket Passive Skills",
            "containing Corrupted Rare Jewels",
        ],
    );
    adorned.rarity = ItemRarity::Unique;
    phylactery
        .jewels
        .push((pobr_build::build::JewelSocket::Tree, adorned));
    close(life(&phylactery) - without, in_a_socket * 2.0);
}

#[test]
fn gem_level_affixes_use_their_dedicated_consumer_without_diagnostic_errors() {
    let data = data();
    let baseline = run(&build("SparkPlayer"), &data);
    for line in [
        "+1 to Level of all Lightning Skills",
        "+1 to Level of all Projectile Skills",
    ] {
        let result = run(
            &build("SparkPlayer").set_item(EquipmentSlot::Amulet, item("Jade Amulet", &[line])),
            &data,
        );
        assert!(result.output().dps > baseline.output().dps, "{line}");
        assert!(
            result.unsupported_modifier_texts().is_empty(),
            "{line}: {:?}",
            result.unsupported_modifier_texts()
        );
    }
}

#[test]
fn lightning_ignite_affix_extends_ailment_source_without_inventing_hit_damage() {
    let data = data();
    let baseline = build("SparkPlayer");
    let base = run(&baseline, &data);
    let current = run(&baseline.clone().set_item(EquipmentSlot::Ring1, item("Sapphire Ring", &[
        "Lightning Damage from Hits also Contributes to Flammability and Ignite Magnitudes",
    ])), &data);
    let previous = run(
        &baseline.set_item(
            EquipmentSlot::Ring1,
            item("Sapphire Ring", &["Your Lightning Damage can Ignite"]),
        ),
        &data,
    );
    assert_eq!(base.output().ignite_dps, 0.0);
    assert!(current.output().ignite_dps > 0.0);
    close(current.output().ignite_dps, previous.output().ignite_dps);
    close(current.output().dps, base.output().dps);
    assert!(current.unsupported_modifier_texts().is_empty());

    let cold = build("IceShotPlayer").set_item(EquipmentSlot::Weapon1, item("Twin Bow", &[]));
    let cold_with_affix = cold.clone().set_item(
        EquipmentSlot::Ring1,
        item(
            "Sapphire Ring",
            &["Lightning Damage from Hits also Contributes to Flammability and Ignite Magnitudes"],
        ),
    );
    close(
        run(&cold_with_affix, &data).output().ignite_dps,
        run(&cold, &data).output().ignite_dps,
    );
}

#[test]
fn weapon_physical_leech_respects_hand_spell_and_conversion() {
    let data = data();
    let baseline = build("AxeChopPlayer")
        .set_item(
            EquipmentSlot::Weapon1,
            item("Wooden Club", &["Adds 100 to 200 Physical Damage"]),
        )
        .set_item(EquipmentSlot::Weapon2, item("Shortsword", &[]));
    let base = run(&baseline, &data);
    let with_offhand = baseline.clone().set_item(
        EquipmentSlot::Weapon2,
        item(
            "Shortsword",
            &[
                "Leeches 5.9% of Physical Damage as Life",
                "Leeches 4.9% of Physical Damage as Mana",
            ],
        ),
    );
    let off = run(&with_offhand, &data);
    assert!(off.unsupported_modifier_texts().is_empty());
    assert!(off.output().life_leech_rate > 0.0);
    assert!(off.output().mana_leech_rate > 0.0);
    close(off.output().dps, base.output().dps);

    // Increasing the other hand's damage must not increase off-hand leech.
    let stronger_main = with_offhand.clone().set_item(
        EquipmentSlot::Weapon1,
        item("Wooden Club", &["Adds 1000 to 2000 Physical Damage"]),
    );
    close(
        run(&stronger_main, &data).output().life_leech_rate,
        off.output().life_leech_rate,
    );
    close(
        run(&stronger_main, &data).output().mana_leech_rate,
        off.output().mana_leech_rate,
    );
    let stronger_off = with_offhand.clone().set_item(
        EquipmentSlot::Weapon2,
        item(
            "Shortsword",
            &[
                "Adds 100 to 200 Physical Damage",
                "Leeches 5.9% of Physical Damage as Life",
                "Leeches 4.9% of Physical Damage as Mana",
            ],
        ),
    );
    assert!(run(&stronger_off, &data).output().life_leech_rate > off.output().life_leech_rate);

    let converted = with_offhand.clone().set_item(
        EquipmentSlot::Ring1,
        item(
            "Sapphire Ring",
            &["100% of Physical Damage Converted to Cold Damage"],
        ),
    );
    let converted = run(&converted, &data);
    assert!(converted.output().dps > 0.0);
    assert_eq!(converted.output().life_leech_rate, 0.0);
    assert_eq!(converted.output().mana_leech_rate, 0.0);
    let cannot = with_offhand.set_item(
        EquipmentSlot::Ring1,
        item("Sapphire Ring", &["Cannot Leech Life"]),
    );
    assert_eq!(run(&cannot, &data).output().life_leech_rate, 0.0);

    let spell = build("FireballPlayer")
        .set_item(
            EquipmentSlot::Weapon1,
            item("Wooden Club", &["Leeches 5.9% of Physical Damage as Life"]),
        )
        .set_item(
            EquipmentSlot::Ring1,
            item(
                "Sapphire Ring",
                &["Adds 100 to 200 Physical Damage to Spells"],
            ),
        );
    let spell = run(&spell, &data);
    assert!(spell.output().dps > 0.0);
    assert_eq!(spell.output().life_leech_rate, 0.0);
}

#[test]
fn totem_child_damage_does_not_leech_resources_to_the_player() {
    let data = data();
    let equipped = |skill: &str| {
        build(skill)
            .set_item(
                EquipmentSlot::Weapon1,
                item("Twin Bow", &["Adds 100 to 200 Physical Damage"]),
            )
            .set_item(
                EquipmentSlot::Ring1,
                item(
                    "Sapphire Ring",
                    &[
                        "Leech 10% of Physical Attack Damage as Life",
                        "Leech 10% of Physical Attack Damage as Mana",
                    ],
                ),
            )
    };
    for skill in [
        "ShockwaveTotemQuakePlayer",
        "ArtilleryBallistaProjectilePlayer",
    ] {
        let result = run(&equipped(skill), &data);
        assert!(result.output().dps > 0.0, "{skill}");
        assert!(
            result.output().damage_components.iter().any(|component| {
                component.damage_type == pobr_data::constants::DamageType::Physical
                    && component.avg() > 0.0
            }),
            "{skill} must have physical hits to exercise the leech gate"
        );
        assert_eq!(result.output().life_leech_rate, 0.0, "{skill}");
        assert_eq!(result.output().mana_leech_rate, 0.0, "{skill}");
    }
    let self_attack = run(&equipped("IceShotPlayer"), &data);
    assert!(self_attack.output().life_leech_rate > 0.0);
    assert!(self_attack.output().mana_leech_rate > 0.0);

    // Check the other proxy type markers at the shared consumer boundary.
    // Merely being eligible for traps/mines is still an ordinary self attack.
    let mut mods = pobr_core::ModDb::new();
    mods.add_mod(pobr_core::Modifier::number(
        "PhysicalDamageLifeLeech",
        pobr_data::modifier::ModType::Base,
        10.0,
    ));
    for (skill_type, can_leech) in [
        ("Trapped", false),
        ("RemoteMined", false),
        ("UsedByTotem", false),
        ("UsedByProxy", false),
        ("Trappable", true),
        ("Mineable", true),
    ] {
        let cfg = pobr_core::CalcConfig::attack().with_skill_types(
            pobr_data::skill::SkillTypes::ATTACK
                | pobr_data::skill::SkillTypes::from_pob2_name(skill_type).unwrap(),
        );
        let result = pobr_core::calc::calc_leech_from_db(
            &mods,
            &cfg,
            1000.0,
            100.0,
            pobr_core::calc::LeechResource::Life,
        );
        assert_eq!(
            result.display_rate_per_second > 0.0,
            can_leech,
            "{skill_type}"
        );
    }
}

#[test]
fn real_equipment_unknown_affix_remains_visible() {
    let data = data();
    let line = "Leech Mana 25% slower";
    let result = run(
        &build("SparkPlayer").set_item(EquipmentSlot::Gloves, item("Linen Wraps", &[line])),
        &data,
    );
    assert!(
        result
            .unsupported_modifier_texts()
            .iter()
            .any(|text| text == line)
    );
    assert!(result.output().dps > 0.0);
    let jewel_line = "Gain additional Ailment Threshold equal to 7% of your Enemy's Energy Shield";
    let jewel = run(
        &build("SparkPlayer").with_jewels(vec![item("Sapphire", &[jewel_line])]),
        &data,
    );
    assert!(
        jewel
            .unsupported_modifier_texts()
            .iter()
            .any(|text| text == jewel_line)
    );
    let partial = "+100 to maximum Life with an unrecognized suffix";
    let parsed =
        pobr_core::mod_parser::ParseCtx::with_engine(data.parser_rules.as_deref().unwrap())
            .parse(partial)
            .unwrap();
    assert!(
        !parsed.mods.is_empty(),
        "fixture must exercise partial parsing"
    );
    assert!(parsed.unparsed.is_some());
    let baseline = run(&build("SparkPlayer"), &data);
    let partial_item = run(
        &build("SparkPlayer").set_item(EquipmentSlot::Ring1, item("Sapphire Ring", &[partial])),
        &data,
    );
    close(partial_item.output().life, baseline.output().life);
    assert!(
        partial_item
            .unsupported_modifier_texts()
            .iter()
            .any(|text| text == partial)
    );
}

#[test]
fn clipboard_metadata_and_locally_consumed_defences_are_not_unsupported() {
    let data = data();
    let helmet = pobr_core::parse_pob_xml_item(
        "Rarity: RARE\nSynthetic Circlet\nCirclet\n--------\nEnergy Shield: 200\n--------\nRequirements:\nLevel: 72\nInt: 50\n--------\nItem Level: 85\n--------\n100% increased Energy Shield\n+100 to maximum Life",
    )
    .unwrap();
    let baseline = run(&build("SparkPlayer"), &data);
    let result = run(
        &build("SparkPlayer").set_item(EquipmentSlot::Helmet, helmet),
        &data,
    );
    assert!(
        result.unsupported_modifier_texts().is_empty(),
        "{:?}",
        result.unsupported_modifier_texts()
    );
    close(result.output().energy_shield, 200.0);
    assert!(result.output().life > baseline.output().life);
}

#[test]
fn time_lost_jewels_use_selected_geometry_radius_upgrades_and_combined_effects() {
    let mut data = data().clone();
    let mut nodes = std::collections::HashMap::new();
    for (skill, kind, x, stats) in [
        (900010, "jewel_socket", 0.0, vec![]),
        (900011, "normal", 100.0, vec!["+7 to maximum Life"]),
        (900012, "normal", 1300.0, vec!["+7 to maximum Life"]),
        (900013, "notable", 100.0, vec!["+11 to maximum Life"]),
        (900014, "normal", 100.0, vec!["+5 to any Attribute"]),
        (
            900015,
            "notable",
            2000.0,
            vec!["50% increased effect of Small Passive Skills"],
        ),
    ] {
        nodes.insert(skill, serde_json::from_value(serde_json::json!({
            "skill": skill, "id": format!("fixture_{skill}"), "kind": kind, "x": x, "y": 0.0, "stats": stats
        })).unwrap());
    }
    // The current tree deliberately has different coordinates. The request must
    // select the historical geometry instead of borrowing current node positions.
    data.passive_nodes = nodes.clone();
    data.passive_nodes.get_mut(&900012).unwrap().x = Some(100.0);
    data.versioned_passive_nodes.insert("9_10".into(), nodes);
    let mut base = build("SparkPlayer").with_tree(pobr_data::passive_tree::PassiveTreeSpec {
        allocated_nodes: (900010..=900015)
            .map(pobr_data::passive_tree::NodeId)
            .collect(),
        ..Default::default()
    });
    base.tree_version = Some("9_10".into());
    let baseline = run(&base, &data).output().life;
    let life_scale = (run(
        &base.clone().set_item(
            EquipmentSlot::Ring1,
            item("Sapphire Ring", &["+100 to maximum Life"]),
        ),
        &data,
    )
    .output()
    .life
        - baseline)
        / 100.0;
    let calc = |upgrade: &str, effect: u32| {
        let text = format!(
            "Rarity: RARE\nReference\nTime-Lost Ruby\nRadius: Small\nImplicits: 0\n{upgrade}\n{effect}% increased Effect of Small Passive Skills in Radius\n25% increased Effect of Notable Passive Skills in Radius\nSmall Passive Skills in Radius also grant +3 to maximum Life\nNotable Passive Skills in Radius also grant +3 to maximum Life"
        );
        let jewel = pobr_build::radius_jewel_from_text(900010, &text).unwrap();
        run(
            &base
                .clone()
                .with_jewels(vec![pobr_core::parse_pob_xml_item(&text).unwrap()])
                .with_radius_jewels(vec![jewel]),
            &data,
        )
    };
    // 7 * (1 + .50 + .25) -> 12, versus global-only 10. Small grant
    // 3 * 1.75 -> 5; notable 11 * 1.25 -> 13 and grant 3 * 1.25 -> 3.
    let small = calc("", 25);
    assert!(
        small.unsupported_modifier_texts().is_empty(),
        "{:?}",
        small.unsupported_modifier_texts()
    );
    close(small.output().life - baseline, 12.0 * life_scale);
    close(
        calc("Upgrades Radius to Medium", 25).output().life - baseline,
        19.0 * life_scale,
    );
    // Numeric balance changes require no new handler or version-specific branch.
    close(
        calc("Upgrades Radius to Large", 50).output().life - baseline,
        25.0 * life_scale,
    );
}

/// The Adorned makes everything a qualifying jewel grants stronger, the
/// grants to passives in its radius included; the node the grant lands on
/// then scales it by its own effect.
#[test]
fn the_adorned_scales_what_a_radius_jewel_grants() {
    let mut data = data().clone();
    let mut nodes = std::collections::HashMap::new();
    for (skill, kind, x) in [(900040, "jewel_socket", 0.0), (900041, "notable", 100.0)] {
        nodes.insert(skill, serde_json::from_value(serde_json::json!({
            "skill": skill, "id": format!("fixture_{skill}"), "kind": kind, "x": x, "y": 0.0, "stats": []
        })).unwrap());
    }
    data.passive_nodes = nodes;
    let base = build("SparkPlayer").with_tree(pobr_data::passive_tree::PassiveTreeSpec {
        allocated_nodes: vec![
            pobr_data::passive_tree::NodeId(900040),
            pobr_data::passive_tree::NodeId(900041),
        ],
        ..Default::default()
    });
    let baseline = run(&base, &data).output().life;
    let life_scale = (run(
        &base.clone().set_item(
            EquipmentSlot::Ring1,
            item("Sapphire Ring", &["+100 to maximum Life"]),
        ),
        &data,
    )
    .output()
    .life
        - baseline)
        / 100.0;

    let mut adorned = item(
        "Diamond",
        &[
            "50% increased Effect of Jewel Socket Passive Skills",
            "containing Corrupted Rare Jewels",
        ],
    );
    adorned.rarity = ItemRarity::Unique;
    // `notable_effect`% on the notable itself; `state` is the jewel's last line.
    let gained = |state: &str, notable_effect: u32, socket: pobr_build::build::JewelSocket| {
        let effect_line = if notable_effect > 0 {
            format!("{notable_effect}% increased Effect of Notable Passive Skills in Radius\n")
        } else {
            String::new()
        };
        let text = format!(
            "Rarity: RARE\nReference\nTime-Lost Ruby\nRadius: Small\nImplicits: 0\n{effect_line}Notable Passive Skills in Radius also grant +20 to maximum Life\n{state}"
        );
        let mut radius = pobr_build::radius_jewel_from_text(900040, &text).unwrap();
        radius.socket = socket;
        let mut build = base.clone().with_jewels(vec![adorned.clone()]);
        build
            .jewels
            .push((socket, pobr_core::parse_pob_xml_item(&text).unwrap()));
        let result = run(&build.with_radius_jewels(vec![radius]), &data);
        assert!(
            result.unsupported_modifier_texts().is_empty(),
            "{:?}",
            result.unsupported_modifier_texts()
        );
        (result.output().life - baseline) / life_scale
    };
    let tree = pobr_build::build::JewelSocket::Tree;

    // Corrupted and rare: 20 * 1.5 = 30.
    close(gained("Corrupted", 0, tree), 30.0);
    // Then the notable's own 50%: 30 * 1.5 = 45.
    close(gained("Corrupted", 50, tree), 45.0);
    // Not corrupted, or not in one of the tree's own sockets: 20.
    close(gained("", 0, tree), 20.0);
    close(
        gained("Corrupted", 0, pobr_build::build::JewelSocket::Granted),
        20.0,
    );
}

#[test]
fn time_lost_scaling_preserves_fractional_recovery_and_both_damage_endpoints() {
    let mut data = data().clone();
    let mut nodes = std::collections::HashMap::new();
    for (skill, kind, stats) in [
        (900020, "jewel_socket", vec![]),
        (
            900021,
            "notable",
            vec!["Regenerate 0.07% of maximum Life per second"],
        ),
    ] {
        nodes.insert(
            skill,
            serde_json::from_value(serde_json::json!({
                "skill": skill, "id": format!("fixture_{skill}"), "kind": kind,
                "x": (skill - 900020) * 50, "y": 0.0, "stats": stats
            }))
            .unwrap(),
        );
    }
    data.passive_nodes = nodes;
    let base = build("SparkPlayer").with_tree(pobr_data::passive_tree::PassiveTreeSpec {
        allocated_nodes: vec![
            pobr_data::passive_tree::NodeId(900020),
            pobr_data::passive_tree::NodeId(900021),
        ],
        ..Default::default()
    });
    let text = "Rarity: RARE\nReference\nTime-Lost Sapphire\nRadius: Small\nImplicits: 0\n50% increased Effect of Notable Passive Skills in Radius\nNotable Passive Skills in Radius also grant Regenerate 0.07% of maximum Life per second\nNotable Passive Skills in Radius also grant Adds 2 to 5 Cold Damage to Spells";
    let equipped = base
        .with_jewels(vec![pobr_core::parse_pob_xml_item(text).unwrap()])
        .with_radius_jewels(vec![
            pobr_build::radius_jewel_from_text(900020, text).unwrap(),
        ]);
    let result = run(&equipped, &data);
    assert!(
        result.unsupported_modifier_texts().is_empty(),
        "{:?}",
        result.unsupported_modifier_texts()
    );
    // LifeRegenPercent uses two decimal places from the loaded JSON table:
    // the notable's own 0.07 and the granted 0.07 both scale to 0.10.
    close(result.output().life_regen, result.output().life * 0.002);
    for (name, expected) in [("ColdDamageMin", 3.0), ("ColdDamageMax", 7.0)] {
        let values: Vec<_> = result
            .mods_named(name)
            .into_iter()
            .filter_map(|m| m.value.as_number())
            .collect();
        close(values.iter().sum(), expected);
    }
}

#[test]
fn overlapping_time_lost_jewels_share_the_last_local_effect_per_node() {
    let mut data = data().clone();
    data.passive_nodes.clear();
    for (skill, kind, stats) in [
        (900031, "jewel_socket", vec![]),
        (900032, "jewel_socket", vec![]),
        (900033, "normal", vec!["+7 to maximum Life"]),
    ] {
        data.passive_nodes.insert(
            skill,
            serde_json::from_value(serde_json::json!({
                "skill": skill, "id": format!("fixture_{skill}"), "kind": kind,
                "x": (skill - 900031) * 50, "y": 0.0, "stats": stats
            }))
            .unwrap(),
        );
    }
    let base = build("SparkPlayer").with_tree(pobr_data::passive_tree::PassiveTreeSpec {
        allocated_nodes: (900031..=900033)
            .map(pobr_data::passive_tree::NodeId)
            .collect(),
        ..Default::default()
    });
    let baseline = run(&base, &data).output().life;
    let life_scale = (run(
        &base.clone().set_item(
            EquipmentSlot::Ring1,
            item("Sapphire Ring", &["+100 to maximum Life"]),
        ),
        &data,
    )
    .output()
    .life
        - baseline)
        / 100.0;
    let jewels: Vec<_> = [(900031, 25, 3), (900032, 50, 5)].into_iter().map(|(socket, effect, value)| {
        let text = format!("Rarity: RARE\nReference\nTime-Lost Ruby\nRadius: Small\nImplicits: 0\n{effect}% increased Effect of Small Passive Skills in Radius\nSmall Passive Skills in Radius also grant +{value} to maximum Life");
        (pobr_core::parse_pob_xml_item(&text).unwrap(), pobr_build::radius_jewel_from_text(socket, &text).unwrap())
    }).collect();
    for (order, expected) in [([0, 1], 14.0), ([1, 0], 10.0)] {
        let equipped = base
            .clone()
            .with_jewels(order.iter().map(|&i| jewels[i].0.clone()).collect())
            .with_radius_jewels(order.iter().map(|&i| jewels[i].1.clone()).collect());
        // At 50%: own node delta 3, grants 4 + 7. At 25%: 1 + 3 + 6.
        close(
            run(&equipped, &data).output().life - baseline,
            expected * life_scale,
        );
    }
}

/// "… per Idol in your Equipment" and "… per non-Idol Augment in your
/// Equipment" count what the equipment's sockets hold. Every augment is a
/// `SoulCore` among the game's base items and an Idol is tagged `idol`; a
/// socket entry that is no augment (a skill a Talisman grants) is neither.
#[test]
fn augments_in_equipment_are_counted_by_kind() {
    let data = data();
    let lines = [
        "+10 to maximum Life per Idol in your Equipment",
        "-4% to all Elemental Resistances per non-Idol Augment in your Equipment",
    ];
    let with_sockets = |socketed: &[&str]| {
        let mut chest = item("Plate Vest", &lines);
        chest.socketed = socketed.iter().map(|name| (*name).into()).collect();
        let session = run(
            &build("FireballPlayer").set_item(EquipmentSlot::BodyArmour, chest),
            &data,
        );
        (session.output().life, session.output().fire_resistance)
    };
    let (life, fire) = with_sockets(&[]);
    let (socketed_life, socketed_fire) = with_sockets(&[
        "Fox Idol",
        "Soul Core of Cholotl",
        "Greater Rune of Leadership",
        "Cackling Companions",
    ]);
    // One Idol, two other augments.
    close(socketed_life - life, 10.0);
    close(socketed_fire - fire, -8.0);
}

#[test]
fn ancient_idol_with_rune_base_tag_does_not_penalize_resistance() {
    let data = data();
    assert_eq!(data.base_items["Carved Majesty"].tags, ["rune"]);
    let mut gloves = item(
        "Secured Wraps",
        &[
            "+10 to maximum Life per Idol in your Equipment",
            "-4% to all Elemental Resistances per non-Idol Augment in your Equipment",
        ],
    );
    let baseline = run(
        &build("FireballPlayer").set_item(EquipmentSlot::Gloves, gloves.clone()),
        &data,
    );
    gloves.socketed = [
        "Carved Majesty",
        "Perfect Iron Rune",
        "Legacy of Wanderlust",
    ]
    .map(String::from)
    .into();
    let socketed = run(
        &build("FireballPlayer").set_item(EquipmentSlot::Gloves, gloves),
        &data,
    );
    // One ancient Idol grants 10 Life; two non-Idol augments cost 2 × 4%.
    close(socketed.output().life - baseline.output().life, 10.0);
    close(
        socketed.output().fire_resistance - baseline.output().fire_resistance,
        -8.0,
    );
    close(
        socketed.output().cold_resistance - baseline.output().cold_resistance,
        -8.0,
    );
    close(
        socketed.output().lightning_resistance - baseline.output().lightning_resistance,
        -8.0,
    );
}

/// A keystone an item grants ("Giant's Blood" on a jewel) applies before
/// the attributes are derived: its "Inherent Life granted by Strength is
/// halved" makes 100 Strength grant 100 Life instead of 200.
#[test]
fn item_granted_keystone_changes_what_attributes_grant() {
    let data = data();
    let life = |lines: &[&str]| {
        let session = run(
            &build("FireballPlayer").set_item(EquipmentSlot::Ring1, item("Iron Ring", lines)),
            &data,
        );
        session.output().life
    };
    close(life(&["+100 to Strength"]) - life(&[]), 200.0);
    close(
        life(&["+100 to Strength", "Giant's Blood"]) - life(&["Giant's Blood"]),
        100.0,
    );
}

/// "+7 to all Attributes per Socket filled" scales on the runes socketed in
/// its item, and the attributes it gives grant Life and Mana like any other:
/// each point of Strength grants Life and each point of Intelligence grants
/// Mana at the game's rates (`base/character_constants.json`).
#[test]
fn attributes_per_socket_filled_grant_life_and_mana() {
    let data = data();
    let runes = ["Iron Rune", "Glacial Rune"];
    let with_lines = |lines: &[&str]| {
        let mut chest = item("Plate Vest", lines);
        chest.socketed = runes.iter().map(|name| (*name).into()).collect();
        let session = run(
            &build("FireballPlayer").set_item(EquipmentSlot::BodyArmour, chest),
            &data,
        );
        (session.output().life, session.output().mana)
    };
    let (life, mana) = with_lines(&[]);
    let (per_socket_life, per_socket_mana) =
        with_lines(&["+7 to all Attributes per Socket filled"]);
    // Two filled sockets: 14 of each attribute.
    let attribute = 7.0 * runes.len() as f64;
    let constants = &data.constants.character_constants;
    close(
        per_socket_life - life,
        attribute * constants.life_per_strength,
    );
    close(
        per_socket_mana - mana,
        attribute * constants.mana_per_intelligence,
    );
}

/// A weapon's "+N to Accuracy Rating" is the game's local stat
/// `local_accuracy_rating`: the rating of attacks with that weapon. Shield
/// Wall attacks with the shield, so the mace's rating is not its own.
#[test]
fn weapon_accuracy_is_the_rating_of_attacks_with_that_weapon() {
    let data = data();
    let with_mace = |mods: &[&str], skill: &str| {
        let build = build(skill)
            .set_item(EquipmentSlot::Weapon1, item("Wooden Club", mods))
            .set_item(EquipmentSlot::Weapon2, item("Splintered Tower Shield", &[]));
        run(&build, &data).output().accuracy
    };
    let accurate = ["+500 to Accuracy Rating"];
    close(
        with_mace(&accurate, "ShieldWallPlayer"),
        with_mace(&[], "ShieldWallPlayer"),
    );
    assert!(with_mace(&accurate, "AxeChopPlayer") > with_mace(&[], "AxeChopPlayer"));
}

/// "Attacks with this Weapon Penetrate 15% Elemental Resistances" is about
/// the weapon's own attacks: the weapon's slot names the hand they are made
/// with, so the attack with it penetrates.
#[test]
fn a_line_about_this_weapon_applies_to_attacks_with_it() {
    let data = data();
    let dps = |mods: &[&str]| {
        let mut lines = vec!["Adds 100 to 200 Fire Damage"];
        lines.extend(mods);
        let build =
            build("AxeChopPlayer").set_item(EquipmentSlot::Weapon1, item("Wooden Club", &lines));
        let session = calculate_with_data_session(
            &build,
            &data,
            &DataOrchestratorOptions {
                mode_effective: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(session.unsupported_modifier_texts().is_empty());
        session.output().dps
    };
    let penetrates = dps(&["Attacks with this Weapon Penetrate 15% Elemental Resistances"]);
    let plain = dps(&[]);
    assert!(penetrates > plain, "{penetrates} > {plain}");
}

/// "Damage with Maces" is the damage of hits with a mace. Shield Wall
/// attacks with the shield, and a spell with no weapon, so neither takes it.
#[test]
fn weapon_type_damage_is_for_attacks_with_that_weapon() {
    let data = data();
    let with_mace = |mods: &[&str], skill: &str| {
        let build = build(skill)
            .set_item(EquipmentSlot::Weapon1, item("Wooden Club", &[]))
            .set_item(EquipmentSlot::Weapon2, item("Splintered Tower Shield", &[]))
            .set_item(EquipmentSlot::Ring1, item("Sapphire Ring", mods));
        run(&build, &data).output().dps
    };
    let maces = ["50% increased Damage with Maces"];
    for skill in ["ShieldWallPlayer", "FireballPlayer"] {
        close(with_mace(&maces, skill), with_mace(&[], skill));
    }
    assert!(with_mace(&maces, "AxeChopPlayer") > with_mace(&[], "AxeChopPlayer"));
}

/// "Spells which cost Life Gain 87% of Damage as Extra Physical Damage"
/// (`spells_gain_%_physical_damage_if_they_cost_life`): a spell costing Mana
/// gains nothing.
#[test]
fn spells_that_cost_mana_gain_nothing_from_a_life_cost_line() {
    let data = data();
    let fireball = |mods: &[&str]| {
        let build =
            build("FireballPlayer").set_item(EquipmentSlot::Ring1, item("Sapphire Ring", mods));
        let session = run(&build, &data);
        assert!(session.unsupported_modifier_texts().is_empty());
        session.output().dps
    };
    close(
        fireball(&["Spells which cost Life Gain 87% of Damage as Extra Physical Damage"]),
        fireball(&[]),
    );
}

/// "Defend against Hits as though you had 1% more Armour per 1% current
/// Energy Shield" (`armour_+%_final_vs_hits_per_1%_current_energy_shield`):
/// at full Energy Shield, Armour counts double against hits.
#[test]
fn armour_counts_more_against_hits_per_current_energy_shield() {
    let data = data();
    let reduction = |mods: &[&str]| {
        let build = build("FireballPlayer")
            .set_item(
                EquipmentSlot::Ring1,
                item("Sapphire Ring", &["+50 to Armour"]),
            )
            .set_item(EquipmentSlot::Ring2, item("Sapphire Ring", mods));
        let session = run(&build, &data);
        assert!(session.unsupported_modifier_texts().is_empty());
        session.output().physical_damage_reduction
    };
    let more = reduction(&[
        "Defend against Hits as though you had 1% more Armour per 1% current Energy Shield",
    ]);
    let plain = reduction(&[]);
    assert!(more > plain, "{more} > {plain}");
}

/// "You have Unholy Might" (`have_unholy_might`): the built-in buff, "Gain
/// 30% of Damage as extra Chaos Damage", is the character's.
#[test]
fn an_item_granting_unholy_might_gains_chaos_damage() {
    let data = data();
    let fireball = |mods: &[&str]| {
        let build =
            build("FireballPlayer").set_item(EquipmentSlot::Ring1, item("Sapphire Ring", mods));
        let session =
            calculate_with_data_session(&build, &data, &DataOrchestratorOptions::default())
                .unwrap();
        assert!(session.unsupported_modifier_texts().is_empty());
        session.output().dps
    };
    let mighty = fireball(&["You have Unholy Might"]);
    let plain = fireball(&[]);
    assert!(mighty > plain * 1.2, "{mighty} against {plain}");
}

#[test]
fn virtuous_barrier_counts_purity_of_fires_attribute_requirement() {
    let data = data();
    let imported = pobr_build::import_build_from_code(include_str!(
        "../../../../examples/reference-builds/tuned/builds/ninja-2026-10-13-mercenary-gemling-legionnaire-twister-682f26ed1957/code.txt"
    ))
    .unwrap();
    let result = calculate_with_data_session(
        &imported.build,
        &data,
        &DataOrchestratorOptions::reference_conditions(),
    )
    .unwrap();
    // The skill text counts skills requiring Intelligence, including the
    // sceptre's Purity of Fire: base 3 + Purity + Herald of Ice + Ghost Dance
    // + Trinity = 7 motes. Base 5% + floor(19 quality * 0.125) = 7% per mote.
    // The ring adds another 23% regeneration rate: 49 + 23 = 72% total.
    close(
        result.config().multipliers["IntelligenceMoteSkillCount"],
        7.0,
    );
    close(result.stat_totals("LifeRegen").1, 72.0);
    assert!((result.output().life_regen_recovery - 110.3391).abs() < 0.0001);
}

#[test]
fn spell_echo_area_uses_the_selected_repeat_view() {
    use pobr_core::rules::config_interpreter::ConfigInputValue;

    let data = data();
    let mut base = build("FrostBombPlayer");
    base.socket_groups[0] = SocketGroup::new()
        .with_gem_skill("FrostBombPlayer", 20)
        .with_gem_skill("SupportSpellEchoPlayer", 1)
        .with_gem_skill("SupportSpellCascadePlayer", 1);
    // Frost Bomb: 24 base + 7 at level 20 = 31 units. Spell Cascade:
    // 20% less area. Spell Echo: one echo with 40% increased area.
    for (mode, area, radius) in [
        ("NONE", 0.8, 27.0),
        ("AVERAGE", 0.96, 30.0),
        ("FINAL", 1.12, 32.0),
        ("FINAL_DPS", 1.12, 32.0),
    ] {
        let mut build = base.clone();
        build
            .config
            .raw_inputs
            .values
            .insert("repeatMode".into(), ConfigInputValue::Text(mode.into()));
        let result = run(&build, &data);
        assert!(
            (result.output().aoe_area_mod - area).abs() < 1e-7,
            "{mode}: {} != {area}",
            result.output().aoe_area_mod,
        );
        close(result.output().aoe_radius, radius);
    }
}

#[test]
fn chaos_max_hit_uses_the_imported_tacati_resistance() {
    let data = data();
    let imported = pobr_build::import_build_from_code(include_str!(
        "../../../../examples/reference-builds/tuned/builds/ninja-2026-10-14-warrior-smith-of-kitava-shield-wall-fb03d7dce08e/code.txt"
    ))
    .unwrap();
    let result = calculate_with_data_session(
        &imported.build,
        &data,
        &DataOrchestratorOptions::reference_conditions(),
    )
    .unwrap();
    // The boots say +11% Chaos Resistance from Tacati and +53% on the
    // item, not the socketable catalog's current +13%. A 386 Chaos hit
    // therefore deals 386 * (1 - 0.64) = 138.96, rather than 131.24.
    close(result.output().chaos_resistance, 64.0);
    close(result.ehp_terms().unwrap().taken_hit[4], 138.96);
    // The Brass Dome's 991 local Ward, with its base, sockets and quality,
    // contributes to 2162 Ward, which must remain in the max-hit pool.
    close(result.output().ward, 2162.0);
    close(result.output().chaos_max_hit, 8001.0);
}

#[test]
fn exporter_absent_item_rules_apply_only_on_the_game_basis() {
    let data = data();
    let build = build("FireballPlayer").set_item(
        EquipmentSlot::Ring1,
        item(
            "Sapphire Ring",
            &["39% increased Mana Cost Efficiency of Spells"],
        ),
    );
    let game = calculate_with_data_session(
        &build,
        &data,
        &DataOrchestratorOptions::against_reference_enemy(),
    )
    .unwrap();
    let reference = calculate_with_data_session(
        &build,
        &data,
        &DataOrchestratorOptions::reference_conditions(),
    )
    .unwrap();
    close(game.stat_totals("ManaCostEfficiency").1, 39.0);
    close(reference.stat_totals("ManaCostEfficiency").1, 0.0);
    let modifier = game.mod_db().named("ManaCostEfficiency").next().unwrap();
    assert_eq!(
        modifier.exporter_rule.as_ref().unwrap().id,
        "spell_mana_cost_efficiency"
    );
    assert!(
        modifier.origin.is_some(),
        "item provenance survives rule provenance"
    );
    let report = pobr_build::ImportReport {
        reference_stats: [("ManaCost".into(), 139.0), ("Life".into(), 100.0)].into(),
        ..Default::default()
    };
    let impacts = pobr_build::exporter_impact::exporter_rule_impacts(
        &build,
        &data,
        &DataOrchestratorOptions::reference_conditions(),
        &report,
        &reference,
    )
    .unwrap();
    assert_eq!(impacts.len(), 1);
    assert_eq!(
        impacts[0].changed_rows, 1,
        "efficiency changes ManaCost, not Life"
    );
}

fn defence_with_lines(
    build: &Build,
    data: &BuildData,
    lines: &[&str],
) -> CompletedCalculationSession {
    calculate_with_data_session(
        build,
        data,
        &DataOrchestratorOptions {
            inject_character_base: false,
            enemy_tier: EnemyTier::None,
            extra_modifier_texts: lines.iter().map(|line| (*line).into()).collect(),
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn focus_energy_shield_increase_shares_global_bucket_and_requires_a_focus() {
    let data = data();
    // Game base_items: Woven Focus has 15 ES, Grand Regalia has 50 ES.
    // Local 100% and 20% quality make the Focus 15 * 2 * 1.2 = 36 ES.
    // Dreamcatcher's 75% applies to those 36 alongside global 50%, not locally.
    let mut focus = item("Woven Focus", &["100% increased Energy Shield"]);
    focus.quality = 20;
    let mut build = build("FireballPlayer");
    build
        .items
        .insert(EquipmentSlot::BodyArmour, item("Grand Regalia", &[]));
    build.items.insert(EquipmentSlot::Weapon2, focus);
    let global = "50% increased maximum Energy Shield";
    let focus_bonus = "75% increased Energy Shield from Equipped Focus";
    close(
        defence_with_lines(&build, &data, &[global])
            .output()
            .energy_shield,
        129.0,
    );
    close(
        defence_with_lines(&build, &data, &[global, focus_bonus])
            .output()
            .energy_shield,
        156.0,
    );
    // A shield can have its own flat ES; sharing weapon2 does not make it a Focus.
    build.items.insert(
        EquipmentSlot::Weapon2,
        item("Splintered Tower Shield", &["+50 to maximum Energy Shield"]),
    );
    close(
        defence_with_lines(&build, &data, &[global, focus_bonus])
            .output()
            .energy_shield,
        150.0,
    );
    build.items.remove(&EquipmentSlot::Weapon2);
    close(
        defence_with_lines(&build, &data, &[global, focus_bonus])
            .output()
            .energy_shield,
        75.0,
    );
}

#[test]
fn shield_defences_increase_only_the_shield_alongside_global_increases() {
    let data = data();
    // Game bases: Grand Regalia = 182 armour / 165 evasion / 50 ES;
    // Splintered Tower Shield = 18 armour, with local flat 20 evasion / 50 ES.
    // Fortified Aegis gives 100% shield defences; global increases are 50%.
    let mut build = build("FireballPlayer");
    build
        .items
        .insert(EquipmentSlot::BodyArmour, item("Grand Regalia", &[]));
    build.items.insert(
        EquipmentSlot::Weapon2,
        item(
            "Splintered Tower Shield",
            &["+20 to Evasion Rating", "+50 to maximum Energy Shield"],
        ),
    );
    let global = "50% increased Armour, Evasion and Energy Shield";
    let shield_bonus = "100% increased Armour, Evasion and Energy Shield from Equipped Shield";
    let base = defence_with_lines(&build, &data, &[global]);
    close(base.output().armour, 300.0);
    close(base.output().evasion, 278.0);
    close(base.output().energy_shield, 150.0);
    let boosted = defence_with_lines(&build, &data, &[global, shield_bonus]);
    close(boosted.output().armour, 318.0);
    close(boosted.output().evasion, 298.0);
    close(boosted.output().energy_shield, 200.0);
    build
        .items
        .insert(EquipmentSlot::Weapon2, item("Woven Focus", &[]));
    close(
        defence_with_lines(&build, &data, &[global, shield_bonus])
            .output()
            .energy_shield,
        98.0,
    );
}

#[test]
fn recharge_rate_and_archon_start_bonuses_reach_build_outputs() {
    let data = data();
    let mut build = build("FireballPlayer");
    build
        .items
        .insert(EquipmentSlot::BodyArmour, item("Grand Regalia", &[]));
    // Grand Regalia's game base is 50 ES. Recharge recovers 12.5%/s;
    // JewelEnergyShieldRecharge's maximum roll is 20%, so 6.25 -> 7.5 ES/s.
    let base = defence_with_lines(&build, &data, &[]);
    close(base.output().es_recharge_per_second, 6.25);
    let recharge = "20% increased Energy Shield Recharge Rate";
    let archon = "20% faster start of Energy Shield Recharge while affected by an Archon Buff";
    let inactive = defence_with_lines(&build, &data, &[recharge, archon]);
    close(inactive.output().es_recharge_rate, 0.15);
    close(inactive.output().es_recharge_per_second, 7.5);
    close(inactive.output().es_recharge_delay, 4.0);
    build
        .config
        .conditions
        .insert("AffectedByArchon".into(), true);
    let active = defence_with_lines(&build, &data, &[recharge, archon]);
    // The conditional recharge bonus is not an Archon-granted buff to scale.
    assert!((active.output().es_recharge_delay - 4.0 / 1.2).abs() < 1e-7);
}

#[test]
fn static_defences_strong_chin_uses_local_helmet_defences() {
    use pobr_data::passive_tree::{NodeId, PassiveTreeSpec};
    let data = data();
    let mut helmet = item(
        "Brimmed Helm",
        &[
            "+14 to Armour",
            "+30 to Evasion Rating",
            "50% increased Armour and Evasion",
        ],
    );
    helmet.quality = 20;
    // Stale exported displays must not replace the current base/local calculation.
    helmet.rolled_defence.armour = Some(9999.0);
    helmet.rolled_defence.evasion = Some(9999.0);
    let mut base = build("SparkPlayer")
        .set_item(EquipmentSlot::Helmet, helmet)
        .with_tree(PassiveTreeSpec {
            allocated_nodes: vec![NodeId(10998)],
            ..Default::default()
        });
    base.config.global_modifier_texts = vec![
        "100% increased Armour and Evasion Rating".into(),
        "50% increased Stun Threshold".into(),
    ];
    let mut without = data.clone();
    without
        .passive_nodes
        .get_mut(&10998)
        .unwrap()
        .stats
        .retain(|line| !line.contains("Stun"));
    let plain = run(&base, &without);
    let with = run(&base, &data);
    // Brimmed Helm base 26/20: (26+14)*1.5*1.2=72 Armour;
    // (20+30)*1.5*1.2=90 Evasion. Global defence scaling is excluded.
    close(
        with.output().stun_threshold - plain.output().stun_threshold,
        72.0 * 1.5,
    );
    close(with.output().armour, plain.output().armour);
    let detail = with.derived_detail("StunThreshold").unwrap();
    let value = |id: &str| detail.inputs.iter().find(|input| input.0 == id).unwrap().1;
    close(value("ThresholdAdded"), 72.0);
    close(
        (value("ThresholdBase") + value("ThresholdAdded"))
            * value("ThresholdIncrease")
            * value("ThresholdMore"),
        with.output().stun_threshold,
    );
    for missing in [None, Some("Rusted Greathelm")] {
        let mut one_defence = base.clone();
        one_defence.items.remove(&EquipmentSlot::Helmet);
        if let Some(helmet) = missing {
            one_defence
                .items
                .insert(EquipmentSlot::Helmet, item(helmet, &[]));
        }
        close(
            run(&one_defence, &data).output().stun_threshold,
            run(&one_defence, &without).output().stun_threshold,
        );
    }
}

#[test]
fn static_defences_projectile_bulwark_uses_incoming_attack_category() {
    use pobr_core::rules::config_interpreter::ConfigInputValue;
    use pobr_data::passive_tree::{NodeId, PassiveTreeSpec};
    let data = data();
    let mut without = data.clone();
    without
        .passive_nodes
        .get_mut(&19236)
        .unwrap()
        .stats
        .retain(|line| !line.contains("Defend"));
    for skill in ["SparkPlayer", "IceShotPlayer", "Melee1HMacePlayer"] {
        assert!(data.granted_effects.contains_key(skill));
        for category in [
            "Projectile",
            "Melee",
            "Spell",
            "SpellProjectile",
            "Untyped",
            "Average",
            "",
        ] {
            let mut base = build(skill).with_tree(PassiveTreeSpec {
                allocated_nodes: vec![NodeId(19236)],
                ..Default::default()
            });
            base.config
                .global_modifier_texts
                .push("+10000 to Armour".into());
            for (name, value) in [
                ("enemyPhysicalDamage", 1000.0),
                ("enemyFireDamage", 0.0),
                ("enemyColdDamage", 0.0),
                ("enemyLightningDamage", 0.0),
                ("enemyChaosDamage", 0.0),
                ("enemyCritChance", 0.0),
            ] {
                base.config
                    .raw_inputs
                    .values
                    .insert(name.into(), ConfigInputValue::Number(value));
            }
            if !category.is_empty() {
                base.config.raw_inputs.values.insert(
                    "enemyDamageType".into(),
                    ConfigInputValue::Text(category.into()),
                );
            }
            let plain = run(&base, &without);
            let with = run(&base, &data);
            close(with.output().armour, plain.output().armour);
            let armour = with
                .ehp_terms()
                .unwrap()
                .mitigation
                .effective_applied_armour[0];
            let expected_scale = if category == "Projectile" { 1.2 } else { 1.0 };
            close(armour, plain.output().armour * expected_scale);
            if category == "Projectile" {
                assert!(
                    with.output().physical_max_hit > plain.output().physical_max_hit,
                    "{skill}"
                );
                assert!(
                    with.output().total_ehp > plain.output().total_ehp,
                    "{skill}"
                );
                let detail = with.derived_detail("PhysicalDamageReduction").unwrap();
                let dr = detail
                    .inputs
                    .iter()
                    .find(|input| input.0 == "ArmourReduction")
                    .unwrap()
                    .1;
                close(dr, with.ehp_terms().unwrap().damage_reduction[0].armour);
                assert!(
                    with.ehp_terms().unwrap().taken_hit[0]
                        < plain.ehp_terms().unwrap().taken_hit[0]
                );
                for stat in ["PhysicalTakenHit", "PhysicalMaxHit", "TotalEHP"] {
                    let detail = with.derived_detail(stat).unwrap();
                    let value =
                        |id: &str| detail.inputs.iter().find(|input| input.0 == id).unwrap().1;
                    let expected = match stat {
                        "PhysicalTakenHit" => {
                            (value("ConvertedIncomingDamage")
                                * value("ResistTakenMultiplier")
                                * value("ReductionTakenMultiplier")
                                + value("FlatDamageTaken"))
                            .max(0.0)
                                * value("AfterReductionMultiplier")
                        }
                        "PhysicalMaxHit" => {
                            (value("SolvedMaxHit") / value("EnemyDamageMultiplier")).round()
                        }
                        "TotalEHP" => {
                            value("NumberOfMitigatedHits") * value("EnemyDamage")
                                / (1.0 - value("NotHitChance") / 100.0)
                        }
                        _ => unreachable!(),
                    };
                    close(pobr_core::display_value(with.output(), stat), expected);
                }
            } else {
                close(
                    with.output().physical_max_hit,
                    plain.output().physical_max_hit,
                );
                close(with.output().total_ehp, plain.output().total_ehp);
            }
            if category == "Projectile" {
                base.config
                    .global_modifier_texts
                    .push("Defend with 150% of Armour".into());
                let strongest = run(&base, &data);
                close(
                    strongest
                        .ehp_terms()
                        .unwrap()
                        .mitigation
                        .effective_applied_armour[0],
                    strongest.output().armour * 1.5,
                );
            }
        }
    }
}
