use super::*;
use crate::calc::{MinimalInput, hit_read_probe, offence};
use pobr_data::catalog::runtime::query_probe;

fn damage() -> [Modifier; 2] {
    [
        Modifier::number("LightningDamageMin", ModType::Base, 1.0),
        Modifier::number("LightningDamageMax", ModType::Base, 10.0),
    ]
}

fn request(modifiers: &[Modifier]) -> ConditionalSkillHitV1<'_> {
    ConditionalSkillHitV1 {
        context: ConditionalHitContext::new(ConditionalHitSource::Spell),
        critical_base: Some(ResolvedCriticalBase {
            chance_fraction: 0.09,
            damage_bonus_percent: 100.0,
        }),
        modifiers,
    }
}

#[test]
fn conditional_hit_reaches_shared_body_without_core_dependency_reads() {
    let modifiers = damage();
    for source in [
        ConditionalHitSource::Spell,
        ConditionalHitSource::Attack,
        ConditionalHitSource::Secondary,
    ] {
        let mut input = request(&modifiers);
        input.context.source = source;
        let ((result, reads), constants) =
            query_probe::observe(|| hit_read_probe::observe(|| calculate_conditional_hit(&input)));
        let out = result.unwrap();
        assert!((out.conditional_weighted_mean - 5.995).abs() < 1e-12);
        assert!(reads.dependencies.is_empty(), "{reads:?}");
        assert!(reads.queries > 0);
        assert_eq!(reads.damage_bodies, 1);
        assert_eq!(reads.components, 2);
        assert_eq!(reads.critical_resolutions, 1);
        assert_eq!(constants, query_probe::Reads::default());
    }
}

fn rejected_before_queries(input: &ConditionalSkillHitV1<'_>) {
    let ((result, reads), constants) =
        query_probe::observe(|| hit_read_probe::observe(|| calculate_conditional_hit(input)));
    assert!(result.is_err());
    assert!(reads.dependencies.is_empty(), "{reads:?}");
    assert_eq!(reads.queries, 0);
    assert_eq!(reads.critical_resolutions, 0);
    assert_eq!(reads.damage_bodies, 0);
    assert_eq!(reads.components, 0);
    assert_eq!(constants, query_probe::Reads::default());
}

#[test]
fn runtime_constant_getter_probe_observes_real_access_and_resets() {
    let constants = pobr_data::catalog::RuntimeConstants::default();
    let (_, reads) = query_probe::observe(|| {
        constants.game();
        constants.character();
        constants.monster();
    });
    assert_eq!(
        reads,
        query_probe::Reads {
            game: 1,
            character: 1,
            monster: 1
        }
    );
    let (_, reads) = query_probe::observe(|| ());
    assert_eq!(reads, query_probe::Reads::default());
}

#[test]
fn conditional_hit_rejects_missing_invalid_and_unadmitted_inputs_before_queries() {
    let modifiers = damage();
    let mut input = request(&modifiers);
    input.critical_base = None;
    rejected_before_queries(&input);
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
        input.critical_base = Some(ResolvedCriticalBase {
            chance_fraction: value,
            damage_bonus_percent: 100.0,
        });
        rejected_before_queries(&input);
        input.critical_base = Some(ResolvedCriticalBase {
            chance_fraction: 0.09,
            damage_bonus_percent: value,
        });
        rejected_before_queries(&input);
    }
    input.critical_base = Some(ResolvedCriticalBase {
        chance_fraction: 1.01,
        damage_bonus_percent: 100.0,
    });
    rejected_before_queries(&input);
    for extra in [
        Modifier::number("Unknown", ModType::Inc, 0.0),
        Modifier::number("Damage", ModType::More, 5.0),
        Modifier::number("Damage", ModType::Inc, f64::NAN),
        Modifier::number("Damage", ModType::Inc, 0.0)
            .with_tag(ModTag::DamageType(DamageType::Lightning)),
        Modifier::number("SpellDamage", ModType::Inc, 10.0)
            .with_tag(ModTag::DamageType(DamageType::Lightning)),
        Modifier::number("CriticalStrikeChance", ModType::Inc, 100.0)
            .with_tag(ModTag::DamageType(DamageType::Lightning)),
        Modifier::number("Damage", ModType::Inc, 5.0).with_tag(ModTag::condition("LowLife", false)),
        Modifier::number("Damage", ModType::Inc, 5.0)
            .with_tag(ModTag::multiplier("Mana", 1.0, None)),
        Modifier::number("Damage", ModType::Inc, 5.0).with_tag(ModTag::PercentStat {
            stat: "Life".into(),
            percent: None,
        }),
        Modifier::number("Damage", ModType::Inc, 5.0).with_tag(ModTag::DistanceRamp {
            ramp: vec![(1.0, 2.0)],
        }),
        Modifier::number("Mana", ModType::Base, 1.0),
    ] {
        let modifiers = [modifiers[0].clone(), modifiers[1].clone(), extra];
        rejected_before_queries(&request(&modifiers));
    }
    let mut input = request(&modifiers);
    input.context.delivery_flags = ModFlags::DOT;
    rejected_before_queries(&input);
}

#[test]
fn ordinary_and_conditional_hits_execute_the_same_body_and_preserve_legacy_rounding() {
    let modifiers = damage();
    let input = request(&modifiers);
    let mut db = ModDb::new();
    db.add_list(modifiers.to_vec());
    db.add_mod(Modifier::number("SkillBaseCritChance", ModType::Base, 9.0));
    let cfg = input.context.config();
    let enemy = ModDb::new();
    let minimal = MinimalInput::default();
    let (ordinary, reads) =
        hit_read_probe::observe(|| offence::hit_pass(&db, &enemy, &cfg, &minimal));
    assert_eq!(reads.damage_bodies, 1);
    assert_eq!(reads.components, 2);
    assert_eq!(reads.critical_resolutions, 1);
    assert!(reads.dependencies.contains(&"legacy critical constants"));
    assert!(reads.dependencies.contains(&"action rate"));
    assert!(reads.dependencies.contains(&"hit chance/target"));
    let conditional = calculate_conditional_hit(&input).unwrap();
    for row in &conditional.damage {
        let old = ordinary
            .crit_pass
            .stored_ranges
            .iter()
            .find(|old| old.damage_type == row.damage_type)
            .unwrap();
        assert_eq!(
            (row.non_critical.min, row.non_critical.max),
            (old.hit_min, old.hit_max)
        );
        assert_eq!(
            (row.critical.min, row.critical.max),
            (old.crit_min, old.crit_max)
        );
    }
    assert!(ordinary.crit_pass.short_circuited);
    assert_eq!(
        ordinary.crit_pass.total_hit_avg,
        super::super::round(5.5 * ordinary.crit.effect)
    );
    assert_eq!(
        conditional.conditional_weighted_mean,
        ordinary
            .crit_pass
            .stored_combined_avg
            .iter()
            .map(|(_, value)| value)
            .sum::<f64>()
    );

    // Existing conditional modifiers still take the real dual pass in ordinary
    // offence; they are intentionally not admitted by the bounded v1 contract.
    db.add_mod(
        Modifier::number("LightningDamage", ModType::Inc, 100.0)
            .with_tag(ModTag::condition("CriticalStrike", false)),
    );
    let ordinary = offence::hit_pass(&db, &enemy, &cfg, &minimal);
    assert!(!ordinary.crit_pass.short_circuited);
    assert_eq!(ordinary.crit_pass.stored_hit_avg[1].1, 5.5);
    assert_eq!(ordinary.crit_pass.stored_crit_avg[1].1, 22.0);
    assert_eq!(
        ordinary.crit_pass.total_hit_avg,
        super::super::round(5.5 * 0.91 + 22.0 * 0.09)
    );
}

#[test]
fn dependency_counters_observe_real_read_boundaries() {
    let cfg = CalcConfig::spell();
    let db = ModDb::new();
    let enemy = ModDb::new();
    let (_, reads) = hit_read_probe::observe(|| {
        cfg.stat("Mana");
        crate::EvalContext::new(&cfg).stat("Life");
        cfg.condition("LowLife");
        cfg.multiplier("PowerCharge");
        cfg.actor_snapshot(crate::ActorRef::Parent, "Mana");
        cfg.actor_multiplier(crate::ActorRef::Player, "Level");
        super::super::crit::resolve_crit(&db, &enemy, &cfg, 1.0, 0.09, true);
        super::super::crit::resolve_crit_traced(
            &db,
            &enemy,
            &cfg,
            1.0,
            0.09,
            true,
            &mut TraceGraph::new(),
        );
        super::super::scaled_damage::scaled_damage_effect(
            &db,
            &enemy,
            &cfg.clone().with_mode_effective(true),
            0.09,
        );
        super::super::skill_mechanics::skill_costs(&db, &cfg);
        super::super::skill_use_time::calc_skill_use_time(&db, &cfg, 0.7, 0.0, false);
        db.sum(ModType::Base, &cfg, &[ModName::from("Life")]);
        crate::HighPrecisionRules::default().precision_for("Damage", ModType::More);
    });
    for boundary in [
        "actor stat",
        "condition snapshot",
        "multiplier snapshot",
        "cross-actor stat",
        "cross-actor multiplier",
        "legacy critical constants",
        "enemy critical chance",
        "enemy critical bonus",
        "enemy critical trace",
        "enemy scaled damage",
        "skill costs",
        "skill use time/server tick",
        "actor pool/defence query",
        "modifier precision table",
    ] {
        assert!(
            reads.dependencies.contains(&boundary),
            "missing {boundary}: {reads:?}"
        );
    }
}
