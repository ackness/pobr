//! The Calcs fields that read terms the calculation already holds: the chance
//! to pass the accuracy check, the cull and reservation multipliers, the hits
//! of a burst, the enemy's ailment threshold, the leech rate ceilings, the
//! deflection effect, the enemy's crit chance, the second smallest maximum
//! hit, loot rarity and the gem level. Each expected number is worked out by
//! hand from the game rule written in the test.

use pobr_core::calc::ehp::{EnemyCritChanceTerms, EnemyCritMode};
use pobr_core::calc::{Actor, ActorBaseStats, Env, perform};
use pobr_core::{CalcConfig, Modifier, display_value};
use pobr_data::prelude::*;

fn environment(base: ActorBaseStats, mods: Vec<Modifier>) -> Env {
    let mut actor = Actor::new(1, base);
    actor.mod_db.add_list(mods);
    Env::new(actor)
}

fn hitting() -> ActorBaseStats {
    ActorBaseStats {
        hit_min: 100.0,
        hit_max: 100.0,
        action_rate: 1.0,
        ..ActorBaseStats::default()
    }
}

fn near(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-3,
        "expected {expected}, got {actual}"
    );
}

/// The accuracy check passes with `accuracy x 1.25 / (accuracy + evasion x
/// 0.3)`: 1000 Accuracy against 1000 Evasion is 1250 / 1300 = 96.1538%. The
/// enemy's block comes after the check, spells and `CannotBeEvaded` skip it.
#[test]
fn accuracy_hit_chance_is_the_check_before_the_enemy_blocks() {
    let attack = |mods: Vec<Modifier>, config: CalcConfig| {
        let mut env = environment(
            ActorBaseStats {
                accuracy: 1000.0,
                ..hitting()
            },
            mods,
        );
        env.enemy = Actor::new(
            1,
            ActorBaseStats {
                evasion: 1000.0,
                ..ActorBaseStats::default()
            },
        );
        env.enemy
            .mod_db
            .add_mod(Modifier::number("BlockChance", ModType::Base, 50.0));
        env.cfg = config;
        perform(&mut env).unwrap();
        env
    };

    let env = attack(vec![], CalcConfig::attack());
    near(env.player.output.accuracy_hit_chance, 96.1538);
    near(
        display_value(&env.player.output, "AccuracyHitChance"),
        96.1538,
    );

    // The block is only part of the effective view and comes on top: half of
    // the hits are blocked, the accuracy check itself is unchanged.
    let env = attack(vec![], CalcConfig::attack().with_mode_effective(true));
    near(env.player.output.accuracy_hit_chance, 96.1538);
    near(env.player.output.hit_chance, 0.961538 * 0.5);

    let env = attack(vec![], CalcConfig::spell());
    assert_eq!(env.player.output.accuracy_hit_chance, 100.0);

    let env = attack(vec![Modifier::flag("CannotBeEvaded")], CalcConfig::attack());
    assert_eq!(env.player.output.accuracy_hit_chance, 100.0);
}

/// Culling Strike kills at the rarity's threshold (game constant
/// `culling_strike_unique_threshold` = 5, rare 10, normal 35), so the damage
/// that has to be dealt is the rest: 100 / (100 - 5) = 1.0526 for a Unique;
/// 100% increased threshold doubles it to 10%: 100 / 90 = 1.1111.
#[test]
fn cull_multiplier_is_the_life_left_to_remove_at_the_rarity_threshold() {
    let cull = |mods: Vec<Modifier>, enemy: Vec<Modifier>| {
        let mut env = environment(hitting(), mods);
        env.enemy.mod_db.add_list(enemy);
        perform(&mut env).unwrap();
        env.player.output.cull_multiplier
    };
    // Nothing culls without the flag.
    assert_eq!(cull(vec![], vec![Modifier::flag("Condition:Unique")]), 1.0);
    near(
        cull(
            vec![Modifier::flag("CanCull")],
            vec![Modifier::flag("Condition:Unique")],
        ),
        100.0 / 95.0,
    );
    near(
        cull(
            vec![Modifier::flag("CanCull")],
            vec![Modifier::flag("Condition:RareOrUnique")],
        ),
        100.0 / 90.0,
    );
    near(cull(vec![Modifier::flag("CanCull")], vec![]), 100.0 / 65.0);
    near(
        cull(
            vec![
                Modifier::flag("CanCull"),
                Modifier::number("CullPercent", ModType::Inc, 100.0),
            ],
            vec![Modifier::flag("Condition:Unique")],
        ),
        100.0 / 90.0,
    );
}

/// Blood Price takes the strongest minimum Life reserved on the enemy (20 and
/// 40 give 40, they do not add) in the effective view: 100 / (100 - 40) =
/// 1.6667. The base view leaves the damage alone.
#[test]
fn reservation_multiplier_uses_the_strongest_reserved_life() {
    let reserved = |effective: bool| {
        let mut env = environment(hitting(), vec![]);
        env.enemy.mod_db.add_list(vec![
            Modifier::number("LifeReservationPercent", ModType::Base, 20.0),
            Modifier::number("LifeReservationPercent", ModType::Base, 40.0),
        ]);
        env.cfg = CalcConfig::spell().with_mode_effective(effective);
        perform(&mut env).unwrap();
        env.player.output.reservation_dps_multiplier
    };
    near(reserved(true), 100.0 / 60.0);
    assert_eq!(reserved(false), 1.0);
}

/// A burst is one hit plus its repeats (RepeatCount 2 gives 3), or with
/// Flicker Strike the flickers of the consumed Power Charges: 1 + 2 per
/// charge x (3 charges + 1 extra) x 1.5 (50% double effect) = 13.
#[test]
fn average_burst_hits_count_the_repeats_and_the_flickers() {
    let mut env = environment(
        hitting(),
        vec![Modifier::number("RepeatCount", ModType::Base, 2.0)],
    );
    perform(&mut env).unwrap();
    assert_eq!(env.player.output.average_burst_hits, 3.0);

    let mut env = environment(hitting(), vec![]);
    perform(&mut env).unwrap();
    assert_eq!(env.player.output.average_burst_hits, 1.0);

    let mut actor = Actor::new(1, hitting());
    for (stat, value) in [
        ("flicker_strike_additional_flickers_from_power_charges", 2.0),
        ("chance_%_to_double_effect_of_removing_charges", 50.0),
    ] {
        actor
            .mod_db
            .add_list(pobr_core::rules::skill_stats::skill_modifiers(stat, value).unwrap());
    }
    actor.mod_db.add_mod(Modifier::number(
        "Multiplier:ExtraConsumablePowerCharges",
        ModType::Base,
        1.0,
    ));
    let mut env = Env::new(actor);
    env.cfg = CalcConfig::spell().with_multiplier("PowerCharge", 3.0);
    perform(&mut env).unwrap();
    assert_eq!(env.player.output.average_burst_hits, 13.0);
}

/// The monster Ailment Threshold of the game table `monster_scaling.json`:
/// level 1 is 15 and level 80 is 65527. 20% increased and 10% more raise
/// level 80 to 65527 x 1.2 x 1.1 = 86495.64.
#[test]
fn enemy_ailment_threshold_is_the_level_table_with_increases_and_more() {
    let threshold = |level: u8, mods: Vec<Modifier>| {
        let mut env = environment(hitting(), vec![]);
        env.enemy = Actor::new(level, ActorBaseStats::default());
        env.enemy.mod_db.add_list(mods);
        perform(&mut env).unwrap();
        env.player.output.enemy_ailment_threshold
    };
    assert_eq!(threshold(1, vec![]), 15.0);
    assert_eq!(threshold(80, vec![]), 65527.0);
    near(
        threshold(
            80,
            vec![
                Modifier::number("EnemyAilmentThreshold", ModType::Inc, 20.0),
                Modifier::number("EnemyAilmentThreshold", ModType::More, 10.0),
            ],
        ),
        65527.0 * 1.2 * 1.1,
    );
}

/// Leech may not exceed 20% of maximum Life or Mana, 10% of maximum Energy
/// Shield per second (a compatibility convention: the game has no stat for
/// it): 1000 Life, 500 Mana and 300 Energy Shield give 200, 100 and 30. 10%
/// increased and 20% more raise the Life ceiling to 20 x 1.1 x 1.2 = 26.4%.
#[test]
fn leech_rate_ceilings_are_percentages_of_each_pool() {
    let pools = ActorBaseStats {
        life: 1000.0,
        mana: 500.0,
        energy_shield: 300.0,
        ..ActorBaseStats::default()
    };
    let mut env = environment(pools, vec![]);
    perform(&mut env).unwrap();
    let out = &env.player.output;
    assert_eq!(out.max_life_leech_rate_percent, 20.0);
    assert_eq!(out.max_life_leech_rate, 200.0);
    assert_eq!(out.max_mana_leech_rate, 100.0);
    assert_eq!(out.max_energy_shield_leech_rate, 30.0);

    let mut env = environment(
        pools,
        vec![
            Modifier::number("MaxLifeLeechRate", ModType::Inc, 10.0),
            Modifier::number("MaxLifeLeechRate", ModType::More, 20.0),
        ],
    );
    perform(&mut env).unwrap();
    let out = &env.player.output;
    near(out.max_life_leech_rate_percent, 26.4);
    near(out.max_life_leech_rate, 264.0);
    // The other resources have their own modifiers.
    assert_eq!(out.max_mana_leech_rate, 100.0);
}

/// Deflection prevents the game constant `deflect_effect` = 40% of the damage
/// of a deflected hit, plus the added effect, within 0 to 100.
#[test]
fn deflect_effect_adds_to_the_base_effect_and_stays_a_percentage() {
    let effect = |added: f64| {
        let mut env = environment(
            hitting(),
            vec![Modifier::number("DeflectEffect", ModType::Base, added)],
        );
        perform(&mut env).unwrap();
        env.player.output.deflect_effect
    };
    assert_eq!(effect(0.0), 40.0);
    assert_eq!(effect(25.0), 65.0);
    assert_eq!(effect(80.0), 100.0);
    assert_eq!(effect(-50.0), 0.0);
}

/// The enemy's chance to crit is 5% (the preset), times its increases and the
/// share of hits that are evaded: 5 x (1 - 15%) = 4.25. Unlucky takes the
/// worse of two rolls, 4.25 x 4.25 / 100 = 0.180625.
#[test]
fn enemy_crit_chance_combines_base_increases_evasion_and_luck() {
    let terms = |mode, increased, evade, unlucky| EnemyCritChanceTerms {
        mode,
        base: 5.0,
        increased,
        evade,
        unlucky,
    };
    near(terms(EnemyCritMode::Rolled, 0.0, 0.0, false).chance(), 5.0);
    near(
        terms(EnemyCritMode::Rolled, 0.0, 15.0, false).chance(),
        4.25,
    );
    near(
        terms(EnemyCritMode::Rolled, 100.0, 50.0, false).chance(),
        5.0,
    );
    near(
        terms(EnemyCritMode::Rolled, 0.0, 15.0, true).chance(),
        0.180625,
    );
    assert_eq!(terms(EnemyCritMode::Never, 100.0, 0.0, false).chance(), 0.0);
    assert_eq!(
        terms(EnemyCritMode::Always, 0.0, 90.0, true).chance(),
        100.0
    );
    // A rolled chance cannot pass 100%: 5 x (1 + 3000%) = 155 stops at 100.
    assert_eq!(
        terms(EnemyCritMode::Rolled, 3000.0, 0.0, false).chance(),
        100.0
    );

    // The same rule reaches the output through the modifiers of both sides.
    let chance = |player: Vec<Modifier>, enemy: Vec<Modifier>| {
        let mut env = environment(hitting(), player);
        env.enemy.mod_db.add_list(enemy);
        perform(&mut env).unwrap();
        env.player.output.enemy_crit_chance
    };
    near(chance(vec![], vec![]), 5.0);
    near(
        chance(
            vec![Modifier::number("EnemyCritChance", ModType::Inc, 20.0)],
            vec![Modifier::number("CritChance", ModType::Inc, 30.0)],
        ),
        5.0 * 1.5,
    );
    near(
        chance(vec![Modifier::flag("EnemyUnluckyCrit")], vec![]),
        0.25,
    );
    assert_eq!(chance(vec![], vec![Modifier::flag("NeverCrit")]), 0.0);
    assert_eq!(chance(vec![], vec![Modifier::flag("AlwaysCrit")]), 100.0);
}

/// 1000 Life: Chaos 0% resisted is 1000, Physical with 20% damage reduction
/// is 1000 / 0.8 = 1250, Fire at 50% is 2000, Cold at 60% is 2500 and
/// Lightning at 75% is 4000. The second smallest is the Physical 1250.
#[test]
fn second_minimal_maximum_hit_is_the_second_smallest_typed_maximum_hit() {
    let mut env = environment(
        ActorBaseStats {
            life: 1000.0,
            ..ActorBaseStats::default()
        },
        vec![
            Modifier::number("PhysicalDamageReduction", ModType::Base, 20.0),
            Modifier::number("FireResistance", ModType::Base, 50.0),
            Modifier::number("ColdResistance", ModType::Base, 60.0),
            Modifier::number("LightningResistance", ModType::Base, 75.0),
        ],
    );
    perform(&mut env).unwrap();
    let out = &env.player.output;
    near(out.fire_max_hit, 2000.0);
    near(out.cold_max_hit, 2500.0);
    near(out.lightning_max_hit, 4000.0);
    near(out.chaos_max_hit, 1000.0);
    near(out.second_minimal_maximum_hit_taken, 1250.0);

    let mut hits = [
        out.physical_max_hit,
        out.fire_max_hit,
        out.cold_max_hit,
        out.lightning_max_hit,
        out.chaos_max_hit,
    ];
    hits.sort_by(f64::total_cmp);
    assert_eq!(out.second_minimal_maximum_hit_taken, hits[1]);

    // Equal minima count separately: the second smallest of 5, 3, 3, 9, 7 is 3.
    assert_eq!(
        pobr_core::calc::ehp::second_minimal_maximum_hit([5.0, 3.0, 3.0, 9.0, 7.0]),
        3.0
    );
    assert_eq!(
        pobr_core::calc::ehp::second_minimal_maximum_hit([10.0, 20.0, 30.0, 40.0, 50.0]),
        20.0
    );
}

/// Item rarity is `(1 + 15% + 45% increased) x 1.2 more` = 1.92; with no
/// modifier it is 1.
#[test]
fn effective_loot_rarity_is_the_increase_and_more_factor() {
    let mut env = environment(hitting(), vec![]);
    perform(&mut env).unwrap();
    assert_eq!(env.player.output.effective_loot_rarity_mod, 1.0);

    let mut env = environment(
        hitting(),
        vec![
            Modifier::number("LootRarity", ModType::Inc, 15.0),
            Modifier::number("LootRarity", ModType::Inc, 45.0),
            Modifier::number("LootRarity", ModType::More, 20.0),
        ],
    );
    perform(&mut env).unwrap();
    near(env.player.output.effective_loot_rarity_mod, 1.92);
}

/// The combined Loreweave stat permits its own 84%, excluding Golden
/// Charm's 15% and even unrelated rarity on the same body armour.
#[test]
fn loreweave_rarity_preserves_only_the_combined_source_in_both_modes() {
    use pobr_core::ModTag;
    use pobr_core::calc::MinimalInput;
    use pobr_core::item::ingest_item_with_ctx;

    let rarity = "84% increased Rarity of Items found";
    let exclusion = "Your other Modifiers to Rarity of Items found do not apply";
    let combined = format!("{rarity}\n{exclusion}");
    let item = |lines: Vec<String>| Item {
        base: ItemBaseId::from("Ornate Ringmail"),
        rarity: ItemRarity::Unique,
        quality: 0,
        corrupted: false,
        implicit_texts: vec![],
        modifier_texts: lines,
        enchant_texts: vec![],
        rolled_defence: Default::default(),
        socketed: vec![],
        parsed_stats: vec![],
    };
    for split in [false, true] {
        for native in [false, true] {
            for enabled in [false, true] {
                let mut lines = vec!["10% increased Rarity of Items found".to_string()];
                lines.extend(if split {
                    vec![rarity.into(), exclusion.into()]
                } else {
                    vec![combined.clone()]
                });
                let armour = item(lines.clone());
                let mut ingest =
                    ingest_item_with_ctx(EquipmentSlot::BodyArmour, &armour, crate::support::ctx())
                        .unwrap();
                assert!(ingest.unsupported.is_empty(), "{:?}", ingest.unsupported);
                assert_eq!(armour.modifier_texts, lines);
                let marker = ingest
                    .modifiers
                    .iter_mut()
                    .find(|m| m.name.as_str() == "LootRarityExclusive")
                    .unwrap();
                marker
                    .tags
                    .push(ModTag::condition("ExclusiveRarity", false));
                let marker_origin = marker.origin.as_ref().unwrap().clone();
                let allowed = ingest
                    .modifiers
                    .iter()
                    .find(|m| m.name.as_str() == "LootRarity" && m.value.as_number() == Some(84.0))
                    .unwrap();
                let origin = allowed.origin.as_ref().unwrap();
                assert_eq!(origin.source_id, marker_origin.source_id);
                assert_eq!(origin.source_id.id, "item.bodyarmour.explicit");
                assert_eq!(origin.raw_text.as_deref(), Some(combined.as_str()));
                assert_eq!(origin.raw_text, marker_origin.raw_text);
                assert_ne!(origin.mod_type, marker_origin.mod_type);
                assert_ne!(origin.stat_id, marker_origin.stat_id);
                let mut session = crate::support::session(MinimalInput::default())
                    .with_config(CalcConfig::attack().with_condition("ExclusiveRarity", enabled));
                session.set_exporter_rules(native, Default::default());
                session.add_modifiers(ingest.modifiers);
                session.add_modifiers([Modifier::number("LootRarity", ModType::Inc, 15.0)
                    .with_origin(
                        ModifierSource::new(SourceId::new(SourceKind::Flask, "flask.charm3"))
                            .with_slot("charm3")
                            .with_raw_text("15% increased Rarity of Items found"),
                    )]);
                session.perform_minimal().unwrap();
                near(
                    session.output().effective_loot_rarity_mod,
                    if enabled { 1.84 } else { 2.09 },
                );
                let groups = session.detail_sources("EffectiveLootRarityMod");
                let group = groups.iter().find(|g| g.name == "LootRarity").unwrap();
                near(group.totals.1, if enabled { 84.0 } else { 109.0 });
                assert_eq!(group.rows.len(), 3);
                for row in &group.rows {
                    assert_eq!(row.active, !enabled || row.value == Some(84.0));
                    assert!(row.modifier.origin.is_some());
                }
            }
        }
    }

    // Only an adjacent exact pair in one section combines. Preserve a
    // separated/conditional exclusion as unsupported instead of guessing.
    for armour in [
        item(vec![
            rarity.into(),
            "+10 to maximum Life".into(),
            exclusion.into(),
        ]),
        item(vec![
            "84% increased Rarity of Items found while on Low Life".into(),
            exclusion.into(),
        ]),
        Item {
            implicit_texts: vec![rarity.into()],
            ..item(vec![exclusion.into()])
        },
    ] {
        let ingest =
            ingest_item_with_ctx(EquipmentSlot::BodyArmour, &armour, crate::support::ctx())
                .unwrap();
        assert!(
            !ingest
                .modifiers
                .iter()
                .any(|m| m.name.as_str() == "LootRarityExclusive")
        );
        assert!(ingest.unsupported.iter().any(|text| text == exclusion));
    }
}

/// The skill runs at its gem's level plus the levels added to it: 20 + 3 =
/// 23, and no skill gives 0.
#[test]
fn gem_level_is_the_gem_level_with_its_bonus_levels() {
    let mut env = environment(hitting(), vec![]);
    perform(&mut env).unwrap();
    assert_eq!(env.player.output.gem_level, 0.0);

    let mut env = environment(hitting(), vec![]);
    env.main_skill_level = Some((20, 3));
    perform(&mut env).unwrap();
    assert_eq!(env.player.output.gem_level, 23.0);
    assert_eq!(display_value(&env.player.output, "GemLevel"), 23.0);
}
