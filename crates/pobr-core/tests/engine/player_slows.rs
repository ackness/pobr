use pobr_core::calc::skill_use_time::{action_speed_factor, calc_skill_use_time};
use pobr_core::calc::{Actor, ActorBaseStats, Env, OutputTable, perform};
use pobr_core::{CalcConfig, ModDb, ModTag, Modifier};
use pobr_data::prelude::*;

fn calculate(extra: &[&str]) -> OutputTable {
    let mut env = Env::new(Actor::new(
        80,
        ActorBaseStats {
            hit_min: 100.0,
            hit_max: 100.0,
            action_rate: 2.0,
            ..Default::default()
        },
    ));
    env.cfg = CalcConfig::spell();
    for text in [
        // A generic slow at the game's default maximum Chill magnitude.
        "50% reduced Action Speed",
        "20% increased Action Speed",
        "20% increased Movement Speed",
        "20% increased Skill Speed",
    ]
    .into_iter()
    .chain(extra.iter().copied())
    {
        let parsed = crate::support::parse_mod(text).unwrap();
        assert_eq!(
            parsed.status,
            pobr_core::mod_parser::ParseStatus::Parsed,
            "{text}: {parsed:?}"
        );
        env.player.mod_db.add_list(parsed.mods);
    }
    env.player.mod_db.add_list([
        Modifier::number("SkillTotemPlacementTimeBase", ModType::Base, 0.6),
        Modifier::number("SkillWarcryCastTimeBase", ModType::Base, 0.6),
    ]);
    perform(&mut env).unwrap();
    env.player.output
}

#[test]
fn slow_potency_and_unaffected_apply_to_every_action_speed_consumer() {
    // The game stat base_slow_potency_+% is -30 on Mail Coat;
    // slows_have_no_potency_on_you is Wanderlust / Relentless Pursuit.
    // 50% slow * (1 - 30/100) = 35%; positive 20% action speed stays intact.
    for (extra, rate, movement, placement) in [
        (vec![], 1.44, 0.72, 0.833333),
        (
            vec!["30% reduced Slowing Potency of Debuffs on You"],
            1.872,
            0.936,
            0.641026,
        ),
        (
            vec!["Your speed is unaffected by Slows"],
            2.88,
            1.44,
            0.416667,
        ),
        (
            vec!["150% reduced Slowing Potency of Debuffs on You"],
            2.88,
            1.44,
            0.416667,
        ),
    ] {
        let out = calculate(&extra);
        assert!((out.action_rate - rate).abs() < 1e-6, "{extra:?}: {out:?}");
        assert_eq!(out.effective_movement_speed_mod, movement);
        assert!((out.total_totem_placement_time - placement).abs() < 1e-6);
        assert!((out.warcry_cast_time - placement).abs() < 1e-6);
    }
}

#[test]
fn independent_slows_multiply_and_respect_conditions_and_potency() {
    let mut db = ModDb::new();
    db.add_list([
        Modifier::number("ActionSpeed", ModType::Inc, -50.0),
        Modifier::number("ActionSpeed", ModType::More, -20.0),
        Modifier::number("ActionSpeed", ModType::Inc, -90.0)
            .with_tag(ModTag::condition("OtherSlow", false)),
        Modifier::number("ActionSpeed", ModType::Inc, 20.0),
        Modifier::number("SlowEffectOnSelf", ModType::Inc, -30.0),
    ]);
    let cfg = CalcConfig::spell();
    db.track_reads();
    // Separate 50% and 20% slows become 35% and 14%: 1.2 * .65 * .86.
    assert!((action_speed_factor(&db, &cfg) - 0.6708).abs() < 1e-9);
    let reads = db.read_snapshot();
    for modifier in db.named("ActionSpeed") {
        assert_eq!(reads.counted(modifier), modifier.matches(&cfg));
    }
    assert_eq!(
        calc_skill_use_time(&db, &cfg, 0.5, 0.0, false).effective_rate,
        1.3416
    );
    // 200% increased potency can stop actions, but cannot make speed negative.
    db.add_mod(Modifier::number("SlowEffectOnSelf", ModType::Inc, 200.0));
    assert_eq!(action_speed_factor(&db, &cfg), 0.0);
    db.add_mod(Modifier::flag("UnaffectedBySlows"));
    assert_eq!(action_speed_factor(&db, &cfg), 1.2);
}

#[test]
fn skill_speed_fanout_has_real_consumers_without_double_counting() {
    let parsed = crate::support::parse_mod("20% increased Skill Speed").unwrap();
    let mut db = ModDb::new();
    db.add_list(parsed.mods);
    db.add_list([
        // Shockwave Totem's cast_time is 600 ms; use the same synthetic
        // base for the warcry to isolate its separate speed bucket.
        Modifier::number("SkillTotemPlacementTimeBase", ModType::Base, 0.6),
        Modifier::number("SkillWarcryCastTimeBase", ModType::Base, 0.6),
    ]);
    let cfg = CalcConfig::spell();
    assert_eq!(
        calc_skill_use_time(&db, &cfg, 0.6, 0.0, false).effective_rate,
        2.0
    );
    let timings = pobr_core::calc::skill_mechanics::skill_timings(&db, &cfg, 0.6);
    assert_eq!(timings.totem, 0.5);
    assert_eq!(timings.warcry, 0.5);
}

fn outgoing_slow_mods() -> Vec<Modifier> {
    let parsed =
        crate::support::parse_mod("Debuffs you inflict have 20% increased Slow Magnitude").unwrap();
    assert_eq!(parsed.status, pobr_core::mod_parser::ParseStatus::Parsed);
    assert_eq!(parsed.mods.len(), 1);
    let rule = parsed.mods[0].exporter_rule.as_ref().unwrap();
    assert_eq!(rule.game_source, "base_debuff_slow_magnitude_+%");
    assert_eq!(rule.exporter, pobr_data::source::ExporterCoverage::Absent);
    parsed.mods
}

#[test]
fn outgoing_debuff_slow_never_creates_an_enemy_debuff_from_damage() {
    for combat in [false, true] {
        let run = |bonus| {
            let mut env = Env::new(Actor::new(80, ActorBaseStats::default()));
            env.cfg = CalcConfig::spell()
                .with_mode_combat(combat)
                .with_mode_effective(combat);
            env.player.mod_db.add_list([
                Modifier::number("ColdDamageMin", ModType::Base, 1_000_000.0),
                Modifier::number("ColdDamageMax", ModType::Base, 1_000_000.0),
            ]);
            if bonus {
                env.player.mod_db.add_list(outgoing_slow_mods());
            }
            perform(&mut env).unwrap();
            env
        };
        let plain = run(false);
        let boosted = run(true);
        assert!(boosted.player.output.chill_effect > 0.0);
        assert_eq!(boosted.player.output.chill_effect_mod, 1.2);
        assert_eq!(
            pobr_core::display_value(&boosted.player.output, "CurrentChill"),
            0.0
        );
        assert_eq!(boosted.cfg.conditions, plain.cfg.conditions);
        assert_eq!(
            boosted.enemy.mod_db.iter_mods().count(),
            plain.enemy.mod_db.iter_mods().count()
        );
        assert_eq!(
            action_speed_factor(&boosted.enemy.mod_db, &boosted.cfg),
            1.0
        );
        assert_eq!(
            boosted.player.output.action_rate,
            plain.player.output.action_rate
        );
        assert_eq!(boosted.player.output.dps, plain.player.output.dps);
    }
}

#[test]
fn outgoing_debuff_slow_scales_only_active_chill_sources_before_the_cap() {
    use pobr_core::calc::ailment_apply::apply_nondamaging_ailments;

    // An explicit modeled Chill source, gated like an enemy debuff. The
    // outgoing line neither supplies that source nor activates its condition.
    for (effective, active, include_absent, raw, expected) in [
        (false, true, true, 30.0, 0.0),
        (true, false, true, 30.0, 0.0),
        (true, true, false, 30.0, 36.0),
        (true, true, true, 30.0, 42.0),
        (true, true, true, 45.0, 50.0),
    ] {
        let mut env = Env::new(Actor::new(80, ActorBaseStats::default()));
        env.cfg = CalcConfig::spell()
            .with_mode_combat(effective)
            .with_mode_effective(effective)
            .with_condition("EnemyChilled", active);
        env.player
            .mod_db
            .set_exporter_rules(include_absent, Default::default());
        env.player.mod_db.add_list(outgoing_slow_mods());
        env.player.mod_db.add_list([
            Modifier::number("ChillBase", ModType::Base, raw)
                .with_tag(ModTag::condition("Effective", false))
                .with_tag(ModTag::condition("EnemyChilled", false)),
            Modifier::number("EnemyChillMagnitude", ModType::Inc, 20.0),
            // Incoming potency and outgoing magnitude have different actors.
            Modifier::number("ActionSpeed", ModType::Inc, -50.0),
            Modifier::number("SlowEffectOnSelf", ModType::Inc, -30.0),
        ]);
        env.enemy.mod_db.add_list([
            Modifier::flag("Condition:Chilled")
                .with_tag(ModTag::condition("Effective", false))
                .with_tag(ModTag::condition("EnemyChilled", false)),
            Modifier::number("SlowEffectOnSelf", ModType::More, -50.0),
        ]);
        let count = env.enemy.mod_db.iter_mods().count();
        let conditions = env.cfg.conditions.clone();
        apply_nondamaging_ailments(&mut env);
        assert_eq!(
            -env.enemy
                .mod_db
                .sum(ModType::Inc, &env.cfg, &[ModName::from("ActionSpeed")]),
            expected
        );
        assert!(
            (action_speed_factor(&env.enemy.mod_db, &env.cfg) - (1.0 - expected * 0.5 / 100.0))
                .abs()
                < 1e-9
        );
        assert_eq!(action_speed_factor(&env.player.mod_db, &env.cfg), 0.65);
        if expected == 0.0 {
            assert_eq!(env.enemy.mod_db.iter_mods().count(), count);
            assert_eq!(env.cfg.conditions, conditions);
        }
    }
}

#[test]
fn outgoing_debuff_slow_preserves_final_magnitudes_and_non_slow_components() {
    use pobr_core::TraceGraph;
    use pobr_core::calc::ailment::{chill_traced, non_damaging_ailment_terms};
    use pobr_core::calc::ailment_apply::apply_nondamaging_ailments;

    for source in ["ChillBase", "ChillMinimum", "ChillOverride", "ChillVal"] {
        let mut env = Env::new(Actor::new(80, ActorBaseStats::default()));
        env.cfg = CalcConfig::spell().with_mode_effective(true);
        env.player.mod_db.add_list(outgoing_slow_mods());
        env.enemy
            .mod_db
            .add_mod(Modifier::flag("Condition:Chilled"));
        let db = if source == "ChillVal" {
            &mut env.enemy.mod_db
        } else {
            &mut env.player.mod_db
        };
        db.add_mod(Modifier::number(source, ModType::Base, 30.0));
        apply_nondamaging_ailments(&mut env);
        let expected = if matches!(source, "ChillBase" | "ChillMinimum") {
            36.0
        } else {
            30.0
        };
        assert_eq!(
            -env.enemy
                .mod_db
                .sum(ModType::Inc, &env.cfg, &[ModName::from("ActionSpeed")]),
            expected,
            "{source}"
        );

        // Hit estimates share the same magnitude bucket. They retain the
        // game's 30% minimum and 50% maximum and never change the enemy db.
        for (damage, expected_hit) in [(0.0, 0.0), (200.0, 0.0), (300.0, 36.0), (500.0, 50.0)] {
            let (effect, _) = chill_traced(
                damage,
                1000.0,
                &env.player.mod_db,
                &env.enemy.mod_db,
                &env.cfg,
                0.0,
                &mut TraceGraph::new(),
            );
            assert_eq!(effect, expected_hit);
        }
        for ailment in [AilmentType::Chill, AilmentType::Shock, AilmentType::Freeze] {
            let terms = non_damaging_ailment_terms(
                ailment,
                &env.player.mod_db,
                &env.enemy.mod_db,
                &env.cfg,
                0.0,
            )
            .unwrap();
            assert_eq!(
                terms.effect_mod(),
                if ailment == AilmentType::Chill {
                    1.2
                } else {
                    1.0
                }
            );
            assert_eq!(
                terms.duration(),
                if ailment == AilmentType::Freeze {
                    4.0
                } else {
                    8.0
                }
            );
        }
    }
}
