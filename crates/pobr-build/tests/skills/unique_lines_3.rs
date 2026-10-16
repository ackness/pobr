//! Output coverage for the ranked item/passive wording batch.
use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data_session, parse_build};
use pobr_core::calc::CalculationSession;
use pobr_gamedata::{GameData, data_version, repo_data_root};

fn data() -> BuildData {
    BuildData::load(&GameData::new(repo_data_root().join(data_version()))).unwrap()
}

fn run(
    data: &BuildData,
    skill: &str,
    items: &str,
    slots: &str,
    extra: &[&str],
) -> CalculationSession {
    let mark = if skill == "UnearthPlayer" {
        r#"<Skill enabled="true"><Gem skillId="WolfPounceMarkPlayer" level="20" enabled="true"/></Skill>"#
    } else {
        ""
    };
    let xml = format!(
        r#"<PathOfBuilding2><Build level="80" className="Warrior" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="{skill}" level="20" enabled="true"/></Skill>{mark}</Skills>
<Items activeItemSet="1">{items}<ItemSet id="1">{slots}</ItemSet></Items></PathOfBuilding2>"#
    );
    calculate_with_data_session(
        &parse_build(&xml).unwrap(),
        data,
        &DataOrchestratorOptions {
            enemy_tier: pobr_data::monster::EnemyTier::None,
            mode_effective: true,
            extra_modifier_texts: extra.iter().map(|line| line.to_string()).collect(),
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn body_armour_grants_require_a_normal_chest_and_change_the_named_outputs() {
    let data = data();
    let lines = [
        "Body Armour grants 30% increased Spirit",
        "Body Armour grants 20% increased Strength",
        "Body Armour grants +75% to Fire Resistance",
        "Body Armour grants +50% of Armour also applies to Elemental Damage",
        "Body Armour grants 25% of Physical Damage from Hits taken as Fire Damage",
        "Body Armour grants 20% increased Stun Threshold",
        "Body Armour grants +5% to Maximum Fire Resistance",
    ];
    for rarity in [
        None,
        Some("NORMAL"),
        Some("MAGIC"),
        Some("RARE"),
        Some("UNIQUE"),
    ] {
        let chest = rarity
            .map(|r| {
                format!(
                    "<Item id=\"1\">Rarity: {r}\nPlate Vest\nArmour: 1000\nImplicits: 0\n</Item>"
                )
            })
            .unwrap_or_default();
        let slot = if rarity.is_some() {
            r#"<Slot name="Body Armour" itemId="1"/>"#
        } else {
            ""
        };
        let base = run(&data, "FireballPlayer", &chest, slot, &["+100 to Spirit"]);
        for line in lines {
            let result = run(
                &data,
                "FireballPlayer",
                &chest,
                slot,
                &["+100 to Spirit", line],
            );
            assert!(
                result.unsupported_modifier_texts().is_empty(),
                "{line}: {:?}",
                result.unsupported_modifier_texts()
            );
            let get = |out: &pobr_core::calc::OutputTable| match line {
                "Body Armour grants 30% increased Spirit" => out.spirit,
                "Body Armour grants 20% increased Strength" => out.strength,
                "Body Armour grants +75% to Fire Resistance" => out.fire_resistance,
                "Body Armour grants +50% of Armour also applies to Elemental Damage" => {
                    out.fire_max_hit
                }
                "Body Armour grants 25% of Physical Damage from Hits taken as Fire Damage" => {
                    out.physical_max_hit
                }
                "Body Armour grants 20% increased Stun Threshold" => out.stun_threshold,
                _ => out.max_fire_resistance,
            };
            if rarity == Some("NORMAL") {
                assert_ne!(get(result.output()), get(base.output()), "{line}");
            } else {
                assert_eq!(
                    get(result.output()),
                    get(base.output()),
                    "{rarity:?}: {line}"
                );
            }
        }
    }
}

#[test]
fn charm_effect_scales_an_active_charm_and_not_an_inactive_one() {
    let data = data();
    let item = "<Item id=\"1\">Rarity: NORMAL\nRuby Charm\nImplicits: 0\n</Item>";
    for active in [true, false] {
        let slot = format!(r#"<Slot name="Charm 1" itemId="1" active="{active}"/>"#);
        let base = run(&data, "FireballPlayer", item, &slot, &["+1 Charm Slot"]);
        let increased = run(
            &data,
            "FireballPlayer",
            item,
            &slot,
            &[
                "+1 Charm Slot",
                "Charms applied to you have 25% increased Effect",
            ],
        );
        let reduced = run(
            &data,
            "FireballPlayer",
            item,
            &slot,
            &[
                "+1 Charm Slot",
                "Charms applied to you have 25% reduced Effect",
            ],
        );
        if active {
            assert!(increased.output().fire_resistance > base.output().fire_resistance);
            assert!(reduced.output().fire_resistance < base.output().fire_resistance);
        } else {
            assert_eq!(
                increased.output().fire_resistance,
                base.output().fire_resistance
            );
            assert_eq!(
                reduced.output().fire_resistance,
                base.output().fire_resistance
            );
        }
    }
}

#[test]
fn temporary_minion_limit_increases_the_count_and_predators_mark_damage() {
    let data = data();
    for (skill, temporary) in [
        ("UnearthPlayer", true),
        ("RagingSpiritsPlayer", true),
        ("SummonSkeletalReaversPlayer", false),
    ] {
        let base = run(&data, skill, "", "", &[]);
        let changed = run(
            &data,
            skill,
            "",
            "",
            &["Temporary Minion Skills have +2 to Limit of Minions summoned"],
        );
        assert_eq!(
            changed.config().multiplier("SummonedMinion")
                - base.config().multiplier("SummonedMinion"),
            if temporary { 2.0 } else { 0.0 },
            "{skill}"
        );
        if skill == "UnearthPlayer" {
            assert!(
                changed.output().dps > base.output().dps,
                "dps {} -> {}; damage {:?}",
                base.output().dps,
                changed.output().dps,
                changed.mods_named("Damage")
            );
        }
    }
}

#[test]
fn recovery_uniques_copy_life_recoup_and_suppress_applied_regeneration() {
    let data = data();
    let stats = [
        "20% of Damage taken Recouped as Life",
        "+500 to maximum Energy Shield",
        "Regenerate 100 Life per second",
    ];
    let base = run(&data, "FireballPlayer", "", "", &stats);
    let mut recoup = stats.to_vec();
    recoup.push("Damage taken Recouped as Life is also Recouped as Energy Shield");
    let copied = run(&data, "FireballPlayer", "", "", &recoup);
    assert!(base.output().life_recoup_rate > 0.0);
    assert_eq!(base.output().es_recoup_rate, 0.0);
    assert!(copied.output().es_recoup_rate > 0.0);
    let mut regen = stats.to_vec();
    regen.push("Life Recovery from Regeneration is not applied");
    let suppressed = run(&data, "FireballPlayer", "", "", &regen);
    assert!(base.output().net_life_regen > 0.0);
    assert_eq!(suppressed.output().net_life_regen, 0.0);
    assert_eq!(suppressed.output().life_regen, base.output().life_regen);
}
