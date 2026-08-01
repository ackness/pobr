//! Lane 2 defence extension tests: ES recharge, avoidance chances, damage-taken multipliers,
//! and crit extra damage reduction.
//!
//! Sources: agent-docs/energy-shield.md, active-defences.md, recovery-charges-buffs.md (PoE2 0.5.0);
//!          PoB2 `src/Modules/CalcDefence.lua`, `src/Data/Misc.lua`.

use pobr_core::calc::defence::{
    ANY_TAKEN_REFLECT_ENABLED, AVOID_AILMENT_CAP, AVOID_HIT_CAP, CritExtraReduction, EsRecharge,
    HitSource, calc_avoidance, calc_crit_extra_reduction, calc_es_recharge, calc_taken_multi_suite,
    enemy_crit_effect, es_recharge_per_second, taken_mult_for_type, taken_mult_for_type_default,
    taken_mult_for_type_with_source, taken_mult_over_time,
};
use pobr_core::{CalcConfig, ModDb, Modifier};
use pobr_data::prelude::*;

// ES recharge tests (gap: es-recharge-missing)

/// PoB2 Misc.lua: `character_inherent_energy_shield_recharge_rate_per_minute_% = 750`
/// → 12.5%/s.
#[test]
fn es_recharge_default_rate_is_12_5_pct_per_second() {
    let db = ModDb::new();
    let cfg = CalcConfig::default();
    let result = calc_es_recharge(&db, &cfg, 1000.0, false);

    // 750 / 60 / 100 = 0.125
    assert!(
        (result.rate_fraction - 0.125).abs() < 1e-9,
        "default rate should be 12.5%/s, got {}",
        result.rate_fraction
    );
}

/// Default recharge delay is 4 seconds (confirmed against PoB2 CalcDefence.lua).
#[test]
fn es_recharge_default_delay_is_4_seconds() {
    let db = ModDb::new();
    let cfg = CalcConfig::default();
    let result = calc_es_recharge(&db, &cfg, 1000.0, false);

    assert_eq!(
        result.delay_seconds, 4.0,
        "default recharge delay should be 4s"
    );
}

/// `EnergyShieldRechargeRate` INC mods raise the recharge rate.
/// +100% INC → rate doubles (0.25/s).
#[test]
fn es_recharge_rate_scales_with_inc_modifier() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "EnergyShieldRechargeRate",
        ModType::Inc,
        100.0,
    ));
    let cfg = CalcConfig::default();
    let result = calc_es_recharge(&db, &cfg, 1000.0, false);

    // base 750%/min * 2.0 = 1500%/min = 25%/s = 0.25
    assert!(
        (result.rate_fraction - 0.25).abs() < 1e-9,
        "+100% INC should double rate to 0.25/s, got {}",
        result.rate_fraction
    );
}

/// PoB2 CalcDefence.lua:1762-1763: **INC** `EnergyShieldRechargeFaster` shortens the delay (denominator).
/// delay = (4 + Sum(BASE)) / (1 + Sum(INC)/100) = 4 / (1 + 100/100) = 2.0s.
#[test]
fn es_recharge_delay_halved_with_faster_inc_100pct() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "EnergyShieldRechargeFaster",
        ModType::Inc,
        100.0,
    ));
    let cfg = CalcConfig::default();
    let result = calc_es_recharge(&db, &cfg, 1000.0, false);
    assert_eq!(result.delay_seconds, 2.0, "100% INC faster → delay 2s");
}

/// BASE `EnergyShieldRechargeFaster` is in seconds and adds to the numerator (4 + base);
/// it does not scale the denominator.
/// delay = (4 + 2) / (1 + 0) = 6.0s.
#[test]
fn es_recharge_delay_base_adds_seconds_to_numerator() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "EnergyShieldRechargeFaster",
        ModType::Base,
        2.0,
    ));
    let cfg = CalcConfig::default();
    let result = calc_es_recharge(&db, &cfg, 1000.0, false);
    assert_eq!(result.delay_seconds, 6.0, "+2s BASE → delay 6s (4+2)");
}

/// Override('EnergyShieldRechargeBase') replaces the base directly, then divides by INC: 1.0 / (1+100/100) = 0.5s.
#[test]
fn es_recharge_delay_respects_override_base() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "EnergyShieldRechargeBase",
        ModType::Override,
        1.0,
    ));
    db.add_mod(Modifier::number(
        "EnergyShieldRechargeFaster",
        ModType::Inc,
        100.0,
    ));
    let cfg = CalcConfig::default();
    let result = calc_es_recharge(&db, &cfg, 1000.0, false);
    assert_eq!(result.delay_seconds, 0.5, "override base 1.0s / 2 = 0.5s");
}

/// ZealotsOath: ES is driven by regen instead, so recharge is disabled (rate_fraction = 0).
#[test]
fn es_recharge_disabled_by_zealots_oath() {
    let db = ModDb::new();
    let cfg = CalcConfig::default();
    let result = calc_es_recharge(&db, &cfg, 1000.0, true /* zealots_oath */);

    assert_eq!(
        result.rate_fraction, 0.0,
        "ZealotsOath should disable ES recharge"
    );
}

/// No recharge when ES is 0 (rate = 0).
#[test]
fn es_recharge_zero_when_no_energy_shield() {
    let db = ModDb::new();
    let cfg = CalcConfig::default();
    let result = calc_es_recharge(&db, &cfg, 0.0, false);

    assert_eq!(result.rate_fraction, 0.0);
}

/// es_recharge_per_second: 1000 ES × 12.5%/s = 125/s.
#[test]
fn es_recharge_per_second_gives_absolute_value() {
    let recharge = EsRecharge {
        rate_fraction: 0.125,
        delay_seconds: 4.0,
    };
    assert_eq!(es_recharge_per_second(&recharge, 1000.0), 125.0);
}

// Avoidance tests (gap: avoidance-ailment-missing)

/// No mods → every avoidance chance is 0 (except stun: 50% when ES > totalTakenHit).
#[test]
fn avoidance_all_zero_without_modifiers_no_es() {
    let db = ModDb::new();
    let cfg = CalcConfig::default();
    let result = calc_avoidance(&db, &cfg, 0.0 /* no ES */, 0.0, false);

    assert_eq!(result.avoid_all_damage_from_hits, 0.0);
    assert_eq!(result.avoid_ignite, 0.0);
    assert_eq!(result.avoid_shock, 0.0);
    assert_eq!(result.avoid_chill, 0.0);
    assert_eq!(result.avoid_freeze, 0.0);
    assert_eq!(result.avoid_poison, 0.0);
    assert_eq!(result.avoid_bleeding, 0.0);
    // Stun avoidance = 0 when there's no ES
    assert_eq!(result.avoid_stun, 0.0);
}

/// ES > totalTakenHit (and not Eldritch Battery) → implicit 50% stun avoidance (PoB2 CalcDefence.lua:2554-2557).
#[test]
fn avoidance_stun_50pct_implicit_when_es_present() {
    let db = ModDb::new();
    let cfg = CalcConfig::default();
    // (CalcDefence.lua:2554-2557): the halving condition is ES > totalTakenHit and not EB.
    let result = calc_avoidance(&db, &cfg, 500.0 /* ES > takenHit */, 100.0, false);

    // notAvoidChance = 100; with ES → notAvoidChance *= 0.5 = 50; effectiveAvoid = 50%
    assert_eq!(
        result.avoid_stun, 50.0,
        "ES > totalTakenHit should give 50% implicit stun avoidance"
    );
}

/// AvoidStun mod at 70% + ES > totalTakenHit (the implicit halving applies to notAvoidChance).
/// notAvoidChance = 100 - 70 = 30; × 0.5 = 15; effectiveAvoid = 85%.
#[test]
fn avoidance_stun_combines_explicit_mod_and_es_implicit() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("AvoidStun", ModType::Base, 70.0));
    let cfg = CalcConfig::default();
    let result = calc_avoidance(&db, &cfg, 500.0, 100.0, false);

    // notAvoid = 100 - 70 = 30; × 0.5 = 15; effectiveAvoid = 85
    assert_eq!(result.avoid_stun, 85.0);
}

/// AvoidAllDamageFromHitsChance is capped at 75% (AVOID_HIT_CAP).
#[test]
fn avoidance_all_damage_capped_at_75() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "AvoidAllDamageFromHitsChance",
        ModType::Base,
        90.0,
    ));
    let cfg = CalcConfig::default();
    let result = calc_avoidance(&db, &cfg, 0.0, 0.0, false);

    assert_eq!(result.avoid_all_damage_from_hits, AVOID_HIT_CAP);
}

/// Ailment avoidance (ignite) is capped at 100% (AVOID_AILMENT_CAP).
#[test]
fn avoidance_ailment_capped_at_100() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("AvoidIgnite", ModType::Base, 120.0));
    let cfg = CalcConfig::default();
    let result = calc_avoidance(&db, &cfg, 0.0, 0.0, false);

    assert_eq!(result.avoid_ignite, AVOID_AILMENT_CAP);
}

/// `IgniteImmune` flag → sets avoidance directly to 100%.
#[test]
fn avoidance_immune_flag_sets_100() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::flag("IgniteImmune"));
    let cfg = CalcConfig::default();
    let result = calc_avoidance(&db, &cfg, 0.0, 0.0, false);

    assert_eq!(result.avoid_ignite, 100.0);
}

/// `ElementalAilmentImmune` → sets ignite/shock/chill/freeze avoidance all to 100%.
#[test]
fn avoidance_elemental_immune_covers_all_elemental_ailments() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::flag("ElementalAilmentImmune"));
    let cfg = CalcConfig::default();
    let result = calc_avoidance(&db, &cfg, 0.0, 0.0, false);

    assert_eq!(result.avoid_ignite, 100.0);
    assert_eq!(result.avoid_shock, 100.0);
    assert_eq!(result.avoid_chill, 100.0);
    assert_eq!(result.avoid_freeze, 100.0);
}

/// `ShockAvoidAppliesToElementalAilments` (Stormshroud) interaction:
/// 50% shock avoidance also applies to ignite/chill/freeze.
#[test]
fn avoidance_stormshroud_shock_applies_to_elemental_ailments() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("AvoidShock", ModType::Base, 50.0));
    db.add_mod(Modifier::flag("ShockAvoidAppliesToElementalAilments"));
    let cfg = CalcConfig::default();
    let result = calc_avoidance(&db, &cfg, 0.0, 0.0, false);

    assert_eq!(result.avoid_shock, 50.0);
    // ignite = AvoidIgnite(0) + AvoidElementalAilments(0) + shock_avoid_raw(50) = 50%
    assert_eq!(result.avoid_ignite, 50.0);
    assert_eq!(result.avoid_chill, 50.0);
    assert_eq!(result.avoid_freeze, 50.0);
}

// Taken-damage multiplier tests (gap: ehp-no-taken-multiplier)

/// No mods → taken multiplier = 1.0 (baseline).
#[test]
fn taken_mult_default_is_1() {
    let db = ModDb::new();
    let cfg = CalcConfig::default();

    assert_eq!(taken_mult_for_type(&db, &cfg, DamageType::Physical), 1.0);
    assert_eq!(taken_mult_for_type(&db, &cfg, DamageType::Fire), 1.0);
    assert_eq!(taken_mult_for_type(&db, &cfg, DamageType::Cold), 1.0);
    assert_eq!(taken_mult_for_type(&db, &cfg, DamageType::Lightning), 1.0);
    assert_eq!(taken_mult_for_type(&db, &cfg, DamageType::Chaos), 1.0);
}

/// `DamageTaken` INC −20 (equivalent to Fortify) → multiplier 0.8.
#[test]
fn taken_mult_reduced_by_inc_modifier() {
    let mut db = ModDb::new();
    // Fortify: DamageTakenWhenHit MORE -10 (per stack), 10 stacks = -10% MORE
    db.add_mod(Modifier::number("DamageTakenWhenHit", ModType::Inc, -20.0));
    let cfg = CalcConfig::default();

    let mult = taken_mult_for_type(&db, &cfg, DamageType::Physical);
    assert!(
        (mult - 0.8).abs() < 1e-9,
        "-20% INC should give mult=0.8, got {mult}"
    );
}

/// `PhysicalDamageTakenWhenHit` INC only affects physical, not fire.
#[test]
fn taken_mult_type_specific_mod_only_affects_that_type() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "PhysicalDamageTakenWhenHit",
        ModType::Inc,
        -30.0,
    ));
    let cfg = CalcConfig::default();

    let phys = taken_mult_for_type(&db, &cfg, DamageType::Physical);
    let fire = taken_mult_for_type(&db, &cfg, DamageType::Fire);

    assert!(
        (phys - 0.7).abs() < 1e-9,
        "phys mult should be 0.7, got {phys}"
    );
    assert_eq!(fire, 1.0, "fire mult should be unaffected");
}

/// `ElementalDamageTaken` INC should apply to fire/cold/lightning but not chaos.
#[test]
fn taken_mult_elemental_mod_applies_to_elemental_types_only() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("ElementalDamageTaken", ModType::Inc, 20.0));
    let cfg = CalcConfig::default();

    let fire = taken_mult_for_type(&db, &cfg, DamageType::Fire);
    let cold = taken_mult_for_type(&db, &cfg, DamageType::Cold);
    let lightning = taken_mult_for_type(&db, &cfg, DamageType::Lightning);
    let chaos = taken_mult_for_type(&db, &cfg, DamageType::Chaos);

    assert!((fire - 1.2).abs() < 1e-9, "fire should be 1.2, got {fire}");
    assert!((cold - 1.2).abs() < 1e-9);
    assert!((lightning - 1.2).abs() < 1e-9);
    assert_eq!(
        chaos, 1.0,
        "chaos should not be affected by elemental taken"
    );
}

/// OverTime variant: `DamageTakenOverTime` INC only affects the OverTime multiplier, not WhenHit.
#[test]
fn taken_mult_over_time_independent_from_when_hit() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("DamageTakenOverTime", ModType::Inc, 50.0));
    let cfg = CalcConfig::default();

    let ot = taken_mult_over_time(&db, &cfg, DamageType::Physical);
    let hit = taken_mult_for_type(&db, &cfg, DamageType::Physical);

    assert!(
        (ot - 1.5).abs() < 1e-9,
        "OverTime mult should be 1.5, got {ot}"
    );
    assert_eq!(hit, 1.0, "WhenHit should be unaffected by OverTime mod");
}

/// MORE mod (Fortify actually injects DamageTakenWhenHit MORE −10 per stack).
/// 10 stacks of Fortify = DamageTakenWhenHit MORE −10 → mult = 0.9.
#[test]
fn taken_mult_more_modifier_multiplies_correctly() {
    let mut db = ModDb::new();
    // MORE = -10 means × (1 + -10/100) = × 0.9
    db.add_mod(Modifier::number("DamageTakenWhenHit", ModType::More, -10.0));
    let cfg = CalcConfig::default();

    let mult = taken_mult_for_type(&db, &cfg, DamageType::Physical);
    assert!(
        (mult - 0.9).abs() < 1e-9,
        "MORE -10 should give mult=0.9, got {mult}"
    );
}

/// calc_taken_multi_suite: each type in the suite is independent.
#[test]
fn taken_multi_suite_type_independent() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "FireDamageTakenWhenHit",
        ModType::Inc,
        30.0,
    ));
    let cfg = CalcConfig::default();

    let suite = calc_taken_multi_suite(&db, &cfg);
    assert!((suite.fire_when_hit - 1.3).abs() < 1e-9);
    assert_eq!(suite.physical_when_hit, 1.0);
    assert_eq!(suite.cold_when_hit, 1.0);
    assert_eq!(suite.chaos_when_hit, 1.0);
}

// Crit extra damage reduction tests (gap: crit-extra-damage-reduction-missing)

/// No mods → reduction_pct = 0, EnemyCritEffect = the full crit damage multiplier.
#[test]
fn crit_extra_reduction_default_zero() {
    let db = ModDb::new();
    let cfg = CalcConfig::default();
    let result = calc_crit_extra_reduction(&db, &cfg);

    assert_eq!(result.reduction_pct, 0.0);
}

/// 50% ReduceCritExtraDamage + enemy 50% crit chance + 100% crit damage bonus:
/// PoB2 formula: EnemyCritEffect = 1 + 0.5 * (100/100) * (1 − 50/100) = 1 + 0.5 * 0.5 = 1.25.
#[test]
fn enemy_crit_effect_with_50pct_reduction() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "ReduceCritExtraDamage",
        ModType::Base,
        50.0,
    ));
    let cfg = CalcConfig::default();
    let reduction = calc_crit_extra_reduction(&db, &cfg);

    assert_eq!(reduction.reduction_pct, 50.0);

    let effect = enemy_crit_effect(50.0, 100.0, &reduction);
    // 1 + 0.5 * 1.0 * 0.5 = 1.25
    assert!(
        (effect - 1.25).abs() < 1e-9,
        "EnemyCritEffect should be 1.25, got {effect}"
    );
}

/// 100% reduction → EnemyCritEffect = 1.0 (no extra crit damage taken).
#[test]
fn enemy_crit_effect_100pct_reduction_equals_no_extra() {
    let reduction = CritExtraReduction {
        reduction_pct: 100.0,
    };
    let effect = enemy_crit_effect(50.0, 200.0, &reduction);

    assert!(
        (effect - 1.0).abs() < 1e-9,
        "100% reduction should make EnemyCritEffect = 1.0, got {effect}"
    );
}

/// Cap: ReduceCritExtraDamage cannot exceed 100% (clamped to 100).
#[test]
fn crit_extra_reduction_capped_at_100() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "ReduceCritExtraDamage",
        ModType::Base,
        150.0,
    ));
    let cfg = CalcConfig::default();
    let result = calc_crit_extra_reduction(&db, &cfg);

    assert_eq!(result.reduction_pct, 100.0);
}

/// EnemyCritEffect = 1.0 when there's no crit (enemy_crit_chance = 0).
#[test]
fn enemy_crit_effect_no_crit_chance_returns_1() {
    let reduction = CritExtraReduction { reduction_pct: 0.0 };
    let effect = enemy_crit_effect(0.0, 100.0, &reduction);

    assert_eq!(effect, 1.0);
}

// Attack/Spell takenMult context + reflect deferral (finding 06-06)
// PoB2 CalcDefence.lua L2265-2269 (hitSourceList={"Attack","Spell"}).

/// `source=None` is equivalent to [`taken_mult_for_type`] (the base hit semantics, without an Attack/Spell layer).
#[test]
fn taken_mult_with_source_none_equals_base() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("FireDamageTaken", ModType::Inc, 30.0));
    let cfg = CalcConfig::default();

    let base = taken_mult_for_type(&db, &cfg, DamageType::Fire);
    let none = taken_mult_for_type_with_source(&db, &cfg, DamageType::Fire, None);
    assert!((base - none).abs() < 1e-9);
}

/// `AttackDamageTaken` only stacks in the Attack context; Spell/None don't read it.
#[test]
fn attack_damage_taken_only_in_attack_context() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("AttackDamageTaken", ModType::Inc, 25.0));
    let cfg = CalcConfig::default();

    let attack =
        taken_mult_for_type_with_source(&db, &cfg, DamageType::Physical, Some(HitSource::Attack));
    let spell =
        taken_mult_for_type_with_source(&db, &cfg, DamageType::Physical, Some(HitSource::Spell));
    let none = taken_mult_for_type(&db, &cfg, DamageType::Physical);

    assert!(
        (attack - 1.25).abs() < 1e-9,
        "Attack context +25% -> 1.25, got {attack}"
    );
    assert_eq!(
        spell, 1.0,
        "the Spell context doesn't read AttackDamageTaken"
    );
    assert_eq!(
        none, 1.0,
        "the base hit scope doesn't read AttackDamageTaken"
    );
}

/// `SpellDamageTaken` only stacks in the Spell context.
#[test]
fn spell_damage_taken_only_in_spell_context() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("SpellDamageTaken", ModType::Inc, -40.0));
    let cfg = CalcConfig::default();

    let spell =
        taken_mult_for_type_with_source(&db, &cfg, DamageType::Fire, Some(HitSource::Spell));
    let attack =
        taken_mult_for_type_with_source(&db, &cfg, DamageType::Fire, Some(HitSource::Attack));

    assert!(
        (spell - 0.6).abs() < 1e-9,
        "Spell context -40% -> 0.6, got {spell}"
    );
    assert_eq!(
        attack, 1.0,
        "the Attack context doesn't read SpellDamageTaken"
    );
}

/// base + WhenHit + Attack layers stack: DamageTaken+10, PhysicalDamageTakenWhenHit+10,
/// AttackDamageTaken+10 → (1 + 0.30) = 1.30 (same INC additive bucket).
#[test]
fn attack_context_stacks_with_base_and_when_hit() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("DamageTaken", ModType::Inc, 10.0));
    db.add_mod(Modifier::number(
        "PhysicalDamageTakenWhenHit",
        ModType::Inc,
        10.0,
    ));
    db.add_mod(Modifier::number("AttackDamageTaken", ModType::Inc, 10.0));
    let cfg = CalcConfig::default();

    let attack =
        taken_mult_for_type_with_source(&db, &cfg, DamageType::Physical, Some(HitSource::Attack));
    assert!(
        (attack - 1.30).abs() < 1e-9,
        "10+10+10 INC → 1.30, got {attack}"
    );
}

/// PoB2 default ("Average") = (Attack layer + Spell layer) / 2.
/// AttackDamageTaken+40 → Attack=1.4, Spell=1.0 → default 1.2.
#[test]
fn taken_mult_default_is_average_of_attack_and_spell() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("AttackDamageTaken", ModType::Inc, 40.0));
    let cfg = CalcConfig::default();

    let def = taken_mult_for_type_default(&db, &cfg, DamageType::Physical);
    assert!(
        (def - 1.2).abs() < 1e-9,
        "Average of 1.4 and 1.0 = 1.2, got {def}"
    );
}

/// Without Attack/Spell mods, default degenerates to the base hit semantics (kept consistent with existing regressions).
#[test]
fn taken_mult_default_degenerates_to_base_without_source_mods() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("DamageTaken", ModType::Inc, -25.0));
    db.add_mod(Modifier::number(
        "FireDamageTakenWhenHit",
        ModType::Inc,
        10.0,
    ));
    let cfg = CalcConfig::default();

    for dt in [
        DamageType::Physical,
        DamageType::Fire,
        DamageType::Cold,
        DamageType::Lightning,
        DamageType::Chaos,
    ] {
        let base = taken_mult_for_type(&db, &cfg, dt);
        let def = taken_mult_for_type_default(&db, &cfg, dt);
        assert!(
            (base - def).abs() < 1e-9,
            "{dt:?}: default {def} should equal the base hit scope {base} (no Attack/Spell mods)"
        );
    }
}

/// Reflect takenMult is currently deferred (PoB2 itself hardcodes AnyTakenReflect to false).
#[test]
fn any_taken_reflect_is_deferred_false() {
    // Read the const into a runtime variable to avoid a trivial assertion on a compile-time constant (clippy::assertions_on_constants).
    let enabled = std::hint::black_box(ANY_TAKEN_REFLECT_ENABLED);
    assert!(
        !enabled,
        "the reflect damage-taken chain is deferred, matching PoB2's current behaviour"
    );
}
