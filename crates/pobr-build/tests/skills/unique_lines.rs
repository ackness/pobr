//! Item and passive text templates from the modifier audit. Each assertion
//! follows the parsed line through to a calculation output.

use pobr_build::{
    Build, BuildConfig, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup,
    calculate_with_data,
};
use pobr_core::calc::{Actor, ActorBaseStats, BuffKind, BuffSpec, Env, OutputTable, perform};
use pobr_core::{CalcConfig, Modifier};
use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};
use pobr_data::prelude::*;
use pobr_gamedata::{GameData, current_data_dir};

fn data() -> BuildData {
    BuildData::load(&GameData::new(current_data_dir())).unwrap()
}

fn build(skill: &str) -> Build {
    Build::new()
        .with_character(CharacterIdentity {
            level: 80,
            class_name: "Sorceress".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(SocketGroup::new().with_gem_skill(skill, 20))
}

fn calc(data: &BuildData, build: &Build, lines: &[&str]) -> OutputTable {
    for line in lines {
        let parsed =
            pobr_core::mod_parser::parse_mod_engine(line, data.parser_rules.as_ref().unwrap());
        assert!(
            parsed
                .unparsed
                .as_deref()
                .is_none_or(|s| s.trim().is_empty()),
            "{line}: {:?}",
            parsed.unparsed
        );
    }
    calculate_with_data(
        build,
        data,
        &DataOrchestratorOptions {
            extra_modifier_texts: lines.iter().map(|s| (*s).into()).collect(),
            ..Default::default()
        },
    )
    .unwrap()
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 0.01, "{actual} != {expected}");
}

#[test]
fn blackheart_armour_mitigates_chaos_hits() {
    let data = data();
    let build = build("FireballPlayer");
    let plain = calc(&data, &build, &["+10000 to Armour"]);
    let protected = calc(
        &data,
        &build,
        &[
            "+10000 to Armour",
            "+20% of Armour also applies to Chaos Damage",
        ],
    );
    assert!(protected.chaos_max_hit > plain.chaos_max_hit);
    close(protected.armour, plain.armour);
    close(protected.physical_max_hit, plain.physical_max_hit);
    // 20% of 10000 is the same effective Armour as 100% of 2000.
    let equivalent = calc(
        &data,
        &build,
        &[
            "+2000 to Armour",
            "+100% of Armour also applies to Chaos Damage",
        ],
    );
    close(protected.chaos_max_hit, equivalent.chaos_max_hit);
}

#[test]
fn vigilant_view_caps_evasion_at_fifty_percent() {
    let data = data();
    let build = build("FireballPlayer");
    let plain = calc(&data, &build, &["+1000000 to Evasion Rating"]);
    let capped = calc(
        &data,
        &build,
        &[
            "+1000000 to Evasion Rating",
            "Maximum Chance to Evade is 50%",
        ],
    );
    assert!(plain.evade_chance > 50.0);
    close(capped.evade_chance, 50.0);
    close(capped.melee_evade_chance, 50.0);
    close(capped.projectile_evade_chance, 50.0);
}

#[test]
fn goregirdle_caps_physical_hit_reduction() {
    let data = data();
    let build = build("FireballPlayer");
    let plain = calc(&data, &build, &["+1000000 to Armour"]);
    let capped = calc(
        &data,
        &build,
        &[
            "+1000000 to Armour",
            "Maximum Physical Damage Reduction is 50%",
        ],
    );
    assert!(plain.physical_max_hit > capped.physical_max_hit);
    close(capped.physical_max_hit, 2.0 * capped.life);
    close(capped.fire_max_hit, plain.fire_max_hit);
}

#[test]
fn matsya_halves_spirit_reservation_after_efficiency() {
    let data = data();
    let build = build("AlchemistsBoonPlayer");
    close(calc(&data, &build, &[]).spirit_reserved, 30.0);
    close(
        calc(&data, &build, &["Skills reserve 50% less Spirit"]).spirit_reserved,
        15.0,
    );
    close(
        calc(
            &data,
            &build,
            &[
                "Skills reserve 50% less Spirit",
                "50% increased Reservation Efficiency",
            ],
        )
        .spirit_reserved,
        10.0,
    );
}

#[test]
fn plaguefinger_includes_elemental_hits_in_poison_without_granting_chance() {
    let data = data();
    let line = "All Damage from Hits Contributes to Poison Magnitude";
    for skill in ["FireballPlayer", "FrostboltPlayer", "SparkPlayer"] {
        let build = build(skill);
        close(
            calc(&data, &build, &["100% chance to Poison on Hit"]).poison_dps,
            0.0,
        );
        close(calc(&data, &build, &[line]).poison_dps, 0.0);
        let poison = calc(&data, &build, &[line, "100% chance to Poison on Hit"]).poison_dps;
        assert!(poison > 0.0, "{skill}: {poison}");
        let increased = calc(
            &data,
            &build,
            &[
                line,
                "100% chance to Poison on Hit",
                "100% increased Damage",
            ],
        )
        .poison_dps;
        close(increased, poison * 2.0);
    }
}

#[test]
fn culling_strike_changes_combined_dps_by_the_enemy_rarity_threshold() {
    let data = data();
    let build = build("FireballPlayer");
    let plain = calc(&data, &build, &[]);
    let cull = calc(&data, &build, &["Culling Strike"]);
    close(cull.dps, plain.dps);
    // An ordinary enemy is culled at 35% Life (culling_strike_normal_threshold).
    close(cull.combined_dps, plain.combined_dps / 0.65);
}

fn body(rarity: ItemRarity) -> Item {
    Item {
        base: ItemBaseId::from("Plate Vest"),
        rarity,
        quality: 0,
        corrupted: false,
        implicit_texts: vec![],
        modifier_texts: vec![],
        enchant_texts: vec![],
        rolled_defence: RolledDefence::default(),
        socketed: vec![],
        parsed_stats: vec![],
    }
}

#[test]
fn kitavan_engraving_grants_life_only_with_normal_body_armour() {
    let data = data();
    for rarity in [
        ItemRarity::Normal,
        ItemRarity::Magic,
        ItemRarity::Rare,
        ItemRarity::Unique,
    ] {
        let build = build("FireballPlayer").set_item(EquipmentSlot::BodyArmour, body(rarity));
        let plain = calc(&data, &build, &[]).life;
        let bonus = calc(
            &data,
            &build,
            &["Body Armour grants 15% increased maximum Life"],
        )
        .life;
        close(
            bonus,
            if rarity == ItemRarity::Normal {
                plain * 1.15
            } else {
                plain
            },
        );
    }
}

#[test]
fn leather_bindings_regenerates_three_percent_only_with_normal_body_armour() {
    let data = data();
    for rarity in [ItemRarity::Normal, ItemRarity::Rare] {
        let build = build("FireballPlayer").set_item(EquipmentSlot::BodyArmour, body(rarity));
        let plain = calc(&data, &build, &[]);
        let bonus = calc(
            &data,
            &build,
            &["Body Armour grants regenerate 3% of maximum Life per second"],
        );
        close(
            bonus.life_regen - plain.life_regen,
            if rarity == ItemRarity::Normal {
                plain.life * 0.03
            } else {
                0.0
            },
        );
    }
}

#[test]
fn venopuncture_and_icefang_chill_only_the_required_ailment_targets() {
    let data = data();
    for (state, line) in [
        (
            "EnemyBleeding",
            "All Damage from Hits against Bleeding targets Contributes to Chill Magnitude",
        ),
        (
            "EnemyPoisoned",
            "All Damage from Hits against Poisoned targets Contributes to Chill Magnitude",
        ),
    ] {
        for active in [false, true] {
            let build = build("FireballPlayer")
                .with_config(BuildConfig::default().with_condition(state, active));
            let output = calc(
                &data,
                &build,
                &[line, "Adds 1000000 to 1000000 Fire Damage to Spells"],
            );
            close(output.chill_effect, if active { 50.0 } else { 0.0 });
        }
    }
}

#[test]
fn hexproof_bypass_restores_the_hex_effect_on_damage() {
    let data = data();
    let dps = |bypass: bool| {
        let mut env = Env::new(Actor::new(
            80,
            ActorBaseStats {
                life: 1000.0,
                hit_min: 100.0,
                hit_max: 100.0,
                action_rate: 1.0,
                ..Default::default()
            },
        ));
        env.cfg = CalcConfig::attack()
            .with_mode_buffs(true)
            .with_mode_effective(true);
        env.player.mod_db.add_mod(Modifier::flag("CannotBeEvaded"));
        env.enemy.mod_db.add_mod(Modifier::flag("Hexproof"));
        if bypass {
            env.player.mod_db.add_list(
                pobr_core::mod_parser::parse_mod_engine(
                    "Curses you inflict can affect Hexproof Enemies",
                    data.parser_rules.as_ref().unwrap(),
                )
                .mods,
            );
        }
        env.buff_skills.push(BuffSpec {
            name: "Test Hex".into(),
            kind: BuffKind::Curse,
            skill_id: "TestHex".into(),
            mods: vec![Modifier::number("DamageTaken", ModType::Inc, 20.0)],
            magnitude: 1.0,
            slot: None,
            socket_index: 1,
            is_mark: false,
            ignore_curse_limit: false,
            local_effect_inc: 0.0,
            local_effect_more: 1.0,
            skill_types: SkillTypes::NONE,
        });
        perform(&mut env).unwrap();
        env.player.output.dps
    };
    assert!(dps(false) > 0.0);
    close(dps(true), dps(false) * 1.2);
}
