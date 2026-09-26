//! Real item/jewel source tests for current trade affixes, not parser-only probes.
use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup,
    calculate_with_data_session,
};
use pobr_core::calc::{CalculationSession, MinimalInput};
use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};
use pobr_data::monster::EnemyTier;
use pobr_gamedata::{GameData, repo_data_root};

fn data() -> BuildData {
    BuildData::load(&GameData::new(pobr_gamedata::current_data_dir())).unwrap()
}

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

fn run(build: &Build, data: &BuildData) -> CalculationSession {
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
    let mut data = data();
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
    direct.jewels.push(item("Ruby", &["+100 to maximum Life"]));
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
    for version in [
        pobr_gamedata::data_version(),
        pobr_data::GOLDEN_PARITY_DATA_VERSION.to_owned(),
    ] {
        let data = BuildData::load(&GameData::new(repo_data_root().join(&version))).unwrap();
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
    let jewel_line = "Meta Skills gain 8% increased Energy";
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
    let mut data = data();
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

#[test]
fn time_lost_scaling_preserves_fractional_recovery_and_both_damage_endpoints() {
    let mut data = data();
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
    let mut data = data();
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
