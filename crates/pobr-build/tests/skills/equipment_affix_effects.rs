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
    BuildData::load(&GameData::new(repo_data_root().join("4.5.5.2"))).unwrap()
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
