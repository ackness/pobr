//! Contract tests for source-independent, conditional single-hit evaluation.

use pobr_core::calc::conditional_hit::{
    ConditionalHitContext, ConditionalHitError, ConditionalHitResult, ConditionalHitSource,
    ConditionalSkillHitV1, calculate_conditional_hit,
};
use pobr_core::calc::crit::ResolvedCriticalBase;
use pobr_core::skill_env::{StatMapCtx, StatMapMode, mapped_stat_modifiers};
use pobr_core::{ModTag, ModValue, Modifier};
use pobr_data::catalog::SkillDamageStat;
use pobr_data::prelude::*;

fn lightning(min: f64, max: f64) -> Vec<Modifier> {
    ["LightningDamageMin", "LightningDamageMax"]
        .into_iter()
        .zip([min, max])
        .map(|(name, value)| Modifier::number(name, ModType::Base, value))
        .collect()
}

fn input(modifiers: &[Modifier], chance: f64, bonus: f64) -> ConditionalSkillHitV1<'_> {
    ConditionalSkillHitV1 {
        context: ConditionalHitContext::new(ConditionalHitSource::Spell),
        critical_base: Some(ResolvedCriticalBase {
            chance_fraction: chance,
            damage_bonus_percent: bonus,
        }),
        modifiers,
    }
}

fn approximately(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
}

fn check_lightning(out: &ConditionalHitResult, min: f64, max: f64, chance: f64, bonus: f64) {
    let row = out
        .damage
        .iter()
        .find(|row| row.damage_type == DamageType::Lightning)
        .unwrap();
    let multiplier = 1.0 + bonus / 100.0;
    let mean = (min + max) / 2.0;
    approximately(row.non_critical.min, min);
    approximately(row.non_critical.max, max);
    approximately(row.non_critical.mean, mean);
    approximately(row.critical.min, min * multiplier);
    approximately(row.critical.max, max * multiplier);
    approximately(row.critical.mean, mean * multiplier);
    approximately(out.crit_probability, chance);
    approximately(out.crit_multiplier, multiplier);
    approximately(
        out.conditional_weighted_mean,
        mean * (1.0 + chance * bonus / 100.0),
    );
}

#[test]
fn owned_mapping_feeds_both_legs_and_retains_source_to_query_evidence() {
    // First source fixture: INTERNATIONAL 0.5.5.5 Spark GE 36 / GESS 38 /
    // GESSPL 1467, Stats 1468/1469 = 1/10; critical chance = 9%; generic
    // Character.ot critical damage BONUS = 100. Actor applicability is a
    // caller-owned policy; these test scalars do not certify that binding.
    let stats = [
        SkillDamageStat {
            stat: "spell_minimum_base_lightning_damage".into(),
            value: 1.0,
        },
        SkillDamageStat {
            stat: "spell_maximum_base_lightning_damage".into(),
            value: 10.0,
        },
    ];
    let mut records = Vec::new();
    let modifiers = mapped_stat_modifiers(
        &mut StatMapCtx {
            catalog: None,
            mode: StatMapMode::Compare,
            records: Some(&mut records),
        },
        &stats,
        SourceKind::SkillGem,
        "skill",
        "SparkPlayer",
        None,
    );
    assert_eq!(modifiers.len(), 2);
    assert_eq!(records.len(), 2);
    assert!(
        records
            .iter()
            .all(|record| record.classification == "mapped")
    );
    let out = calculate_conditional_hit(&input(&modifiers, 0.09, 100.0)).unwrap();
    check_lightning(&out, 1.0, 10.0, 0.09, 100.0);
    approximately(out.conditional_weighted_mean, 5.995);
    for (evidence, original) in out.modifiers.iter().zip(&modifiers) {
        assert_eq!(&evidence.modifier, original);
        assert!(evidence.counted);
        assert!(
            evidence
                .queries
                .contains(&format!("BASE {}", original.name))
        );
        assert_eq!(
            original.origin.as_ref().unwrap().source_id.kind,
            SourceKind::SkillGem
        );
    }
}

#[test]
fn damage_chance_and_actor_bonus_vary_independently() {
    for (min, max) in [(1.0, 10.0), (3.0, 7.0), (17.0, 61.0), (0.0, 0.0)] {
        for chance in [0.0, 0.09, 0.37, 1.0] {
            for bonus in [0.0, 25.0, 100.0, 275.0] {
                let modifiers = lightning(min, max);
                let out = calculate_conditional_hit(&input(&modifiers, chance, bonus)).unwrap();
                if max == 0.0 {
                    assert_eq!(out.conditional_weighted_mean, 0.0);
                    approximately(out.crit_multiplier, 1.0 + bonus / 100.0);
                } else {
                    check_lightning(&out, min, max, chance, bonus);
                }
            }
        }
    }
}

#[test]
fn static_source_scopes_apply_through_existing_queries() {
    let mut modifiers = lightning(10.0, 20.0);
    modifiers.extend([
        Modifier::number("Damage", ModType::Inc, 10.0).with_keyword_flags(KeywordFlags::HIT),
        Modifier::number("Damage", ModType::Inc, 20.0).with_flags(ModFlags::SPELL),
        Modifier::number("Damage", ModType::Inc, 30.0).with_flags(ModFlags::ATTACK),
        Modifier::number("LightningDamage", ModType::Inc, 40.0)
            .with_tag(ModTag::DamageType(DamageType::Lightning)),
        Modifier::number("Damage", ModType::Inc, 50.0)
            .with_tag(ModTag::SkillTypes(SkillTypes::SPELL)),
        Modifier::number("ProjectileDamage", ModType::Inc, 60.0),
    ]);
    for (source, scale) in [
        (ConditionalHitSource::Spell, 2.8),
        (ConditionalHitSource::Attack, 2.4),
        (ConditionalHitSource::Secondary, 2.1),
    ] {
        let mut request = input(&modifiers, 0.25, 50.0);
        request.context.source = source;
        request.context.delivery_flags = ModFlags::PROJECTILE;
        request.context.damage_keywords = KeywordFlags::LIGHTNING;
        let out = calculate_conditional_hit(&request).unwrap();
        check_lightning(&out, 10.0 * scale, 20.0 * scale, 0.25, 50.0);
        assert_eq!(out.modifiers.len(), modifiers.len());
        assert_eq!(
            out.modifiers[4].counted,
            source == ConditionalHitSource::Attack
        );
        assert_eq!(
            out.modifiers[6].counted,
            source == ConditionalHitSource::Spell
        );
    }
}

#[test]
fn damage_type_tagged_generic_and_critical_increases_are_rejected() {
    for (name, source) in [
        ("Damage", ConditionalHitSource::Spell),
        ("AttackDamage", ConditionalHitSource::Attack),
        ("SpellDamage", ConditionalHitSource::Spell),
        ("ProjectileDamage", ConditionalHitSource::Spell),
        ("AreaDamage", ConditionalHitSource::Spell),
        ("MeleeDamage", ConditionalHitSource::Attack),
        ("CriticalStrikeChance", ConditionalHitSource::Spell),
        ("CriticalStrikeMultiplier", ConditionalHitSource::Spell),
    ] {
        for value in [0.0, 100.0] {
            let mut modifiers = lightning(10.0, 20.0);
            modifiers.push(
                Modifier::number(name, ModType::Inc, value)
                    .with_tag(ModTag::DamageType(DamageType::Lightning)),
            );
            let mut request = input(&modifiers, 0.25, 100.0);
            request.context.source = source;
            request.context.delivery_flags =
                ModFlags::PROJECTILE | ModFlags::AREA | ModFlags::MELEE;
            assert_eq!(
                calculate_conditional_hit(&request).unwrap_err(),
                ConditionalHitError::UnsupportedModifierTag { index: 2 },
                "{name} INC {value} must not be accepted and silently omitted"
            );
        }
    }
}

#[test]
fn damage_type_tags_on_typed_ranges_and_elemental_increases_stay_type_scoped() {
    let modifiers = [
        Modifier::number("LightningDamageMin", ModType::Base, 10.0)
            .with_tag(ModTag::DamageType(DamageType::Lightning)),
        Modifier::number("LightningDamageMax", ModType::Base, 20.0)
            .with_tag(ModTag::DamageType(DamageType::Lightning)),
        Modifier::number("FireDamageMin", ModType::Base, 10.0)
            .with_tag(ModTag::DamageType(DamageType::Fire)),
        Modifier::number("FireDamageMax", ModType::Base, 20.0)
            .with_tag(ModTag::DamageType(DamageType::Fire)),
        Modifier::number("ElementalDamage", ModType::Inc, 100.0)
            .with_tag(ModTag::DamageType(DamageType::Lightning)),
    ];
    let out = calculate_conditional_hit(&input(&modifiers, 0.25, 100.0)).unwrap();
    for (damage_type, min, max) in [
        (DamageType::Lightning, 20.0, 40.0),
        (DamageType::Fire, 10.0, 20.0),
    ] {
        let row = out
            .damage
            .iter()
            .find(|row| row.damage_type == damage_type)
            .unwrap();
        approximately(row.non_critical.min, min);
        approximately(row.non_critical.max, max);
        approximately(row.critical.min, min * 2.0);
        approximately(row.critical.max, max * 2.0);
    }
    assert!(out.modifiers.iter().all(|evidence| evidence.counted));
    approximately(out.conditional_weighted_mean, 56.25);
}

#[test]
fn other_damage_types_and_critical_increases_use_common_readers() {
    let modifiers = [
        Modifier::number("FireDamageMin", ModType::Base, 20.0),
        Modifier::number("FireDamageMax", ModType::Base, 40.0),
        Modifier::number("ChaosDamageMin", ModType::Base, 10.0),
        Modifier::number("ChaosDamageMax", ModType::Base, 10.0),
        Modifier::number("CriticalStrikeChance", ModType::Inc, 50.0),
        Modifier::number("CriticalStrikeMultiplier", ModType::Inc, 25.0),
    ];
    let out = calculate_conditional_hit(&input(&modifiers, 0.2, 80.0)).unwrap();
    approximately(out.crit_probability, 0.3);
    approximately(out.crit_multiplier, 2.0);
    approximately(out.conditional_weighted_mean, 52.0);
    assert!(out.modifiers.iter().all(|evidence| evidence.counted));
    assert!(
        out.crit_trace
            .nodes()
            .iter()
            .any(|node| node.label == "crit average factor" && node.value == 1.3)
    );
}

#[test]
fn weighted_result_uses_stored_means_without_another_effect_round() {
    let modifiers = lightning(1.123456789, 11.987654321);
    let out = calculate_conditional_hit(&input(&modifiers, 0.123456789, 33.333333333)).unwrap();
    let row = out
        .damage
        .iter()
        .find(|row| row.damage_type == DamageType::Lightning)
        .unwrap();
    let expected = row.critical.mean * out.crit_probability
        + row.non_critical.mean * (1.0 - out.crit_probability);
    assert_eq!(out.conditional_weighted_mean, expected);
    let rounded_effect =
        ((1.0 - out.crit_probability + out.crit_probability * out.crit_multiplier) * 1e9).round()
            / 1e9;
    let legacy_total = (row.non_critical.mean * rounded_effect * 1e9).round() / 1e9;
    assert_ne!(out.conditional_weighted_mean, legacy_total);
}

#[test]
fn missing_and_invalid_critical_bases_are_errors() {
    let modifiers = lightning(1.0, 10.0);
    let mut request = input(&modifiers, 0.09, 100.0);
    request.critical_base = None;
    assert_eq!(
        calculate_conditional_hit(&request).unwrap_err(),
        ConditionalHitError::MissingCriticalBase
    );
    for chance in [-0.01, 1.01, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            calculate_conditional_hit(&input(&modifiers, chance, 100.0)).unwrap_err(),
            ConditionalHitError::InvalidCriticalChance
        );
    }
    for bonus in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            calculate_conditional_hit(&input(&modifiers, 0.09, bonus)).unwrap_err(),
            ConditionalHitError::InvalidCriticalDamageBonus
        );
    }
}

#[test]
fn unadmitted_stats_types_and_dynamic_tags_fail_even_when_inactive() {
    let extras = [
        Modifier::number("UnknownDamageChannel", ModType::Inc, 0.0),
        Modifier::number("Mana", ModType::Base, 100.0),
        Modifier::number("ProjectileCount", ModType::Base, 4.0),
        Modifier::number("SkillDurationBase", ModType::Base, 2.0),
        Modifier::number("SkillBaseCritChance", ModType::Base, 9.0),
        Modifier::number("CriticalStrikeMultiplier", ModType::Base, 100.0),
        Modifier::number("Damage", ModType::More, 10.0),
        Modifier::number("Damage", ModType::Inc, 10.0)
            .with_tag(ModTag::condition("NeverSet", false)),
        Modifier::number("Damage", ModType::Inc, 10.0)
            .with_tag(ModTag::condition("CriticalStrike", false)),
        Modifier::number("Damage", ModType::Inc, 10.0)
            .with_tag(ModTag::multiplier("Mana", 1.0, None)),
        Modifier::number("Damage", ModType::Inc, 10.0).with_tag(ModTag::PerStat {
            stat: "Life".into(),
            div: 1.0,
            limit: None,
            limit_var: None,
            actor: None,
        }),
        Modifier::number("Damage", ModType::Inc, 10.0).with_tag(ModTag::PercentStat {
            stat: "Mana".into(),
            percent: None,
        }),
        Modifier::number("Damage", ModType::Inc, 10.0).with_tag(ModTag::DistanceRamp {
            ramp: vec![(1.0, 2.0)],
        }),
        Modifier::number("Damage", ModType::Inc, 10.0)
            .with_tag(ModTag::SkillTypes(SkillTypes::from_pob2_index(198))),
        Modifier::number("Damage", ModType::Inc, 10.0)
            .with_tag(ModTag::SlotName("Body Armour".into())),
        Modifier::number("Damage", ModType::Inc, 10.0).with_flags(ModFlags::DOT),
        Modifier::number("Damage", ModType::Inc, 10.0).with_keyword_flags(KeywordFlags::MINION),
        Modifier::new("Damage", ModType::Inc, ModValue::NestedMods(vec![])),
        Modifier::new("Damage", ModType::Inc, ModValue::Bool(true)),
    ];
    for extra in extras {
        let mut modifiers = lightning(1.0, 10.0);
        modifiers.push(extra);
        assert!(
            calculate_conditional_hit(&input(&modifiers, 0.09, 100.0)).is_err(),
            "{:?}",
            modifiers.last()
        );
    }
}

#[test]
fn nonfinite_numbers_bad_ranges_overflow_and_unadmitted_contexts_fail() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
        let modifiers = lightning(value, 10.0);
        assert_eq!(
            calculate_conditional_hit(&input(&modifiers, 0.09, 100.0)).unwrap_err(),
            ConditionalHitError::InvalidModifierValue { index: 0 }
        );
    }
    let reversed = lightning(20.0, 10.0);
    assert!(matches!(
        calculate_conditional_hit(&input(&reversed, 0.1, 100.0)),
        Err(ConditionalHitError::InvalidDamageRange { .. })
    ));
    let huge = lightning(f64::MAX, f64::MAX);
    assert_eq!(
        calculate_conditional_hit(&input(&huge, 0.1, 100.0)).unwrap_err(),
        ConditionalHitError::NonFiniteResult
    );
    let modifiers = lightning(1.0, 10.0);
    let mut request = input(&modifiers, 0.09, 100.0);
    request.context.delivery_flags = ModFlags::WEAPON;
    assert_eq!(
        calculate_conditional_hit(&request).unwrap_err(),
        ConditionalHitError::UnsupportedContext
    );
    request.context = ConditionalHitContext::new(ConditionalHitSource::Spell);
    request.context.damage_keywords = KeywordFlags::TOTEM;
    assert_eq!(
        calculate_conditional_hit(&request).unwrap_err(),
        ConditionalHitError::UnsupportedContext
    );
}
