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
