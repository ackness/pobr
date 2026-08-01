use pobr_core::calc::ailment::{
    AilmentSource, DamagingAilmentOutput, StackConfig, ailment_crit_chance, ailment_effect_mod,
    ailment_rate_mod, apply_dot_dps_cap, apply_effect_and_rate_mod,
    apply_effect_and_rate_mod_traced, apply_effect_mod_to_instance, apply_rate_mod_to_instance,
    bleed_instance, bleed_traced, chill_effect, chill_effect_with_mods, chill_traced,
    corrupted_blood_instance, cross_type_source_hit, cross_type_source_hit_at_roll,
    debuff_duration_mult, dps_with_effect_rate_cap, dps_with_effect_rate_cap_traced,
    effmult_for_ailment, electrocute_poise_buildup, electrocute_poise_buildup_traced,
    estimate_active_stacks, flat_chance, freeze_poise_buildup, freeze_poise_buildup_traced,
    ignite_instance, ignite_traced, player_ailment_threshold, poison_instance, roll_average,
    shock_effect, stack_potential, stacking_ailment_dps, stacking_ailment_dps_traced,
    threshold_derived_chance, weighted_source_damage,
};
use pobr_core::{CalcConfig, ModDb, Modifier, TraceGraph, TraceOperation};
use pobr_data::prelude::*;

#[test]
fn bleed_magnitude_is_15_percent_of_physical_hit_per_second() {
    let gc = GameConstants::poe2();
    let instance = bleed_instance(1000.0, &ModDb::new(), &CalcConfig::attack());

    assert_eq!(instance.ailment, AilmentType::Bleed);
    assert_eq!(instance.magnitude_dps, 1000.0 * gc.bleed_base_fraction);
    assert_eq!(instance.duration_secs, gc.bleed_base_duration);
    assert!(instance.bypasses_es);
}

#[test]
fn ignite_uses_fire_fraction_and_duration() {
    let gc = GameConstants::poe2();
    let instance = ignite_instance(500.0, &ModDb::new(), &CalcConfig::attack());

    assert_eq!(instance.ailment, AilmentType::Ignite);
    assert_eq!(instance.magnitude_dps, 500.0 * gc.ignite_base_fraction);
    assert_eq!(instance.duration_secs, gc.ignite_base_duration);
}

#[test]
fn poison_magnitude_scales_with_ailment_magnitude() {
    // PoE2: damaging ailment magnitude only scales with `AilmentMagnitude` (PoB2's
    // ailmentPercentBase factor). PoE1's PoisonDamage/DamageOverTime mods don't exist
    // in PoE2 and have no effect here.
    let gc = GameConstants::poe2();
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("AilmentMagnitude", ModType::Inc, 100.0));

    let instance = poison_instance(1000.0, &db, &CalcConfig::attack());

    let base = 1000.0 * gc.poison_base_fraction;
    assert_eq!(instance.magnitude_dps, base * 2.0);
    assert_eq!(instance.ailment, AilmentType::Poison);

    // The PoE1 mod name is a no-op in PoE2 — it must not scale the magnitude.
    let mut db_phantom = ModDb::new();
    db_phantom.add_mod(Modifier::number("PoisonDamage", ModType::Inc, 100.0));
    let phantom = poison_instance(1000.0, &db_phantom, &CalcConfig::attack());
    assert_eq!(phantom.magnitude_dps, base, "PoisonDamage 幻影名不生效");
}

/// Shock effect range for PoE2 0.5.0.
///
/// **Bug#9 fix**: shock's minimum is 20% (not PoE1's 5%), max is 100% (not PoE1's 50%).
/// Source: agent-docs/ailments.md §Shock `BaseShockMagnitude=20, max=100`;
///         PoB2 `nonDamagingAilmentsConfig.Shock, clamp [20, 100]`.
#[test]
fn shock_effect_is_clamped_between_20_and_100_percent_poe2() {
    // No hit → returns 0 (shock not applied)
    assert_eq!(shock_effect(0.0, 1000.0, SHOCK_MIN_EFFECT), 0.0);
    // Huge hit → shock capped at 100% (= 1.0 fraction)
    let huge = shock_effect(1_000_000.0, 100.0, SHOCK_MIN_EFFECT);
    assert_eq!(huge, 1.0);
    // Tiny hit (relative to threshold) → shock floored at 20% (= 0.20 fraction)
    let tiny = shock_effect(1.0, 1_000_000.0, SHOCK_MIN_EFFECT);
    assert_eq!(tiny, 0.20);
    // Hit at full threshold (ratio=1) → 50% shock (0.5 * 1.0^0.4 = 0.5, fraction 0.50)
    let at_threshold = shock_effect(1000.0, 1000.0, SHOCK_MIN_EFFECT);
    assert_eq!(at_threshold, 0.50);
}

#[test]
fn corrupted_blood_is_a_ten_stack_physical_debuff() {
    let debuff = corrupted_blood_instance(10.0);
    assert_eq!(debuff.max_stacks, 10);
    assert_eq!(debuff.total_dps(), 100.0);
}

#[test]
fn ailment_total_damage_is_dps_times_duration() {
    let instance = bleed_instance(1000.0, &ModDb::new(), &CalcConfig::attack());
    assert_eq!(
        instance.total_damage(),
        instance.magnitude_dps * instance.duration_secs
    );
}

// Step 2: apply chance + effMult + crit weighting + player threshold + trace

/// Player ailment threshold = max life × 0.5 (gap: player-ailment-threshold-bug).
#[test]
fn player_ailment_threshold_is_half_of_max_life() {
    assert_eq!(player_ailment_threshold(1000.0), 500.0);
    assert_eq!(player_ailment_threshold(2480.0), 1240.0);
    assert_eq!(player_ailment_threshold(0.0), 0.0);
}

/// Intrinsic chance (bleed/poison): base × (1+inc/100) × more, clamped to 100.
/// A chance of 0 means the ailment is never applied.
#[test]
fn flat_chance_scales_and_clamps() {
    // 25% base, no inc/more
    assert_eq!(flat_chance(25.0, 0.0, 0.0), 0.0); // more=0 → 0 (more is a multiplier; pass 1.0 for "no more")
    // Correct usage: more=1.0 means no more modifier
    assert_eq!(flat_chance(25.0, 0.0, 1.0), 25.0);
    // +100% inc → 50%
    assert_eq!(flat_chance(25.0, 100.0, 1.0), 50.0);
    // over 100 → clamped to 100
    assert_eq!(flat_chance(80.0, 100.0, 1.0), 100.0);
    // base=0 → 0 (never applied)
    assert_eq!(flat_chance(0.0, 500.0, 2.0), 0.0);
}

/// Threshold-derived chance (ignite/shock) rises monotonically with hit/threshold:
/// higher damage or a lower threshold → higher chance.
#[test]
fn threshold_derived_chance_increases_with_hit_and_decreases_with_threshold() {
    let mult = 20.0; // IgniteChanceMultiplier
    // Hit at full threshold (hit=threshold=1000): hit/thr*mult = 20% chance on hit
    let (on_hit, _) = threshold_derived_chance(1000.0, 1000.0, 1000.0, mult, 0.0, 0.0, 1.0);
    assert!((on_hit - 20.0).abs() < 1e-6);

    // Double the damage → double the chance (linear region, not yet clamped)
    let (on_hit2, _) = threshold_derived_chance(2000.0, 2000.0, 1000.0, mult, 0.0, 0.0, 1.0);
    assert!(on_hit2 > on_hit);
    assert!((on_hit2 - 40.0).abs() < 1e-6);

    // Higher threshold → lower chance
    let (on_hit_high_thr, _) =
        threshold_derived_chance(1000.0, 1000.0, 2000.0, mult, 0.0, 0.0, 1.0);
    assert!(on_hit_high_thr < on_hit);

    // Massive damage → clamped to 100
    let (capped, _) =
        threshold_derived_chance(1_000_000.0, 1_000_000.0, 1000.0, mult, 0.0, 0.0, 1.0);
    assert_eq!(capped, 100.0);
}

/// Crit source damage exceeds non-crit (crit_avg = hit_avg × crit_mult); weighting
/// pushes the resulting base above the pure non-crit value.
#[test]
fn crit_weighting_raises_source_damage() {
    // 50% crit chance, 2x crit multiplier
    let source = AilmentSource::new(1000.0, 2.0, 0.5, false);
    assert_eq!(source.hit_avg, 1000.0);
    assert_eq!(source.crit_avg, 2000.0);

    // 100% hit chance, 100% crit chance: base should trend toward crit damage
    let (_chance, base_high_crit) = weighted_source_damage(&source, 100.0, 100.0);
    // Weighted: hit*(1-0.5)*chanceOnHit + crit*0.5*chanceOnCrit, normalized = 1500 (midpoint)
    assert!(
        base_high_crit > 1000.0,
        "crit weighting should exceed non-crit hit"
    );

    // AilmentsAreNeverFromCrit: crit source degrades to non-crit, base = non-crit damage
    let no_crit = AilmentSource::new(1000.0, 2.0, 0.5, true);
    assert_eq!(no_crit.crit_avg, 1000.0);
    assert_eq!(no_crit.crit_chance, 0.0);
    let (_c, base_no_crit) = weighted_source_damage(&no_crit, 100.0, 100.0);
    assert_eq!(base_no_crit, 1000.0);
}

/// effMult: 40% enemy fire resistance → ignite DPS effMult = 0.6 (gap: ailment-effmult-missing).
#[test]
fn effmult_reduced_by_enemy_resistance() {
    let cfg = CalcConfig::attack().with_mode_effective(true);
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("FireResist", ModType::Base, 40.0));

    let eff = effmult_for_ailment(&enemy, &cfg, DamageType::Fire, true);
    assert!(
        (eff - 0.6).abs() < 1e-6,
        "40% fire resist → effMult 0.6, got {eff}"
    );

    // mode_effective=false → effMult 1.0 (bare panel figure)
    let bare = effmult_for_ailment(&enemy, &cfg, DamageType::Fire, false);
    assert_eq!(bare, 1.0);
}

/// effMult: +50% enemy DamageTakenOverTime → effMult scales up by 1.5x.
#[test]
fn effmult_raised_by_damage_taken_over_time() {
    let cfg = CalcConfig::attack().with_mode_effective(true);
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("DamageTakenOverTime", ModType::Inc, 50.0));

    let eff = effmult_for_ailment(&enemy, &cfg, DamageType::Physical, true);
    assert!(
        (eff - 1.5).abs() < 1e-6,
        "+50% DamageTakenOverTime → 1.5, got {eff}"
    );
}

/// Physical ailments (bleed) ignore resistance mitigation: enemy physical resistance
/// does not affect effMult (only the "taken" chain applies).
#[test]
fn physical_ailment_ignores_resistance_in_effmult() {
    let cfg = CalcConfig::attack().with_mode_effective(true);
    let mut enemy = ModDb::new();
    // Physical "resistance" is meaningless for ailments; only the taken chain matters
    enemy.add_mod(Modifier::number("PhysicalDamageTaken", ModType::Inc, 20.0));

    let eff = effmult_for_ailment(&enemy, &cfg, DamageType::Physical, true);
    assert!((eff - 1.2).abs() < 1e-6);
}

/// Bleed panel: 100% chance + effMult, traceable via TraceGraph (gap: ailment-trace-attribution-missing).
#[test]
fn bleed_traced_writes_trace_and_applies_chance_effmult() {
    let cfg = CalcConfig::attack()
        .with_damage_type(DamageType::Physical)
        .with_mode_effective(true);
    let mut player = ModDb::new();
    player.add_mod(Modifier::number("BleedChance", ModType::Base, 100.0));
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("PhysicalDamageTaken", ModType::Inc, 50.0));

    let source = AilmentSource::new(1000.0, 2.0, 0.0, false);
    let mut trace = TraceGraph::new();
    let (out, node): (DamagingAilmentOutput, _) =
        bleed_traced(&source, &player, &enemy, &cfg, &mut trace);

    // 100% chance
    assert_eq!(out.chance, 1.0);
    // effMult 1.5 (+50% PhysicalDamageTaken)
    assert!((out.eff_mult - 1.5).abs() < 1e-6);
    // expected_dps = magnitude × chance; magnitude already includes effMult
    let gc = GameConstants::poe2();
    let expected_mag = (1000.0 * gc.bleed_base_fraction) * 1.5;
    assert!((out.magnitude_dps - expected_mag).abs() < 1e-3);
    assert!((out.expected_dps - expected_mag).abs() < 1e-3); // chance=1.0

    // trace should have nodes, and the output node should exist
    assert!(!trace.nodes().is_empty());
    assert!(trace.node(node).is_some());
    // BleedChance BASE contribution should enter the graph as a source node
    let has_chance_source = trace
        .nodes()
        .iter()
        .any(|n| n.label.contains("BleedChance") || n.label.contains("Bleed chance"));
    assert!(
        has_chance_source,
        "trace should contain bleed chance contribution"
    );

    // DPS output node should have incoming edges (chance + magnitude + effMult), so it's traceable.
    let incoming = trace.incoming(node);
    assert!(
        incoming.len() >= 3,
        "DPS node should aggregate chance + magnitude + effMult (got {} edges)",
        incoming.len()
    );
    // effMult node (carrying the enemy PhysicalDamageTaken contribution) should exist in the graph.
    let has_effmult = trace
        .nodes()
        .iter()
        .any(|n| n.label.contains("EffMult") || n.label.contains("DamageTaken"));
    assert!(
        has_effmult,
        "trace should contain effMult/DamageTaken nodes"
    );
}

/// Ignite chance derivation: high fire damage/low threshold → high chance → high expected DPS
/// (gap: no-ailment-chance-pipeline).
#[test]
fn ignite_traced_chance_scales_with_threshold() {
    let cfg = CalcConfig::attack().with_damage_type(DamageType::Fire);
    let player = ModDb::new();
    let enemy = ModDb::new();
    let source = AilmentSource::new(1000.0, 2.0, 0.0, false);

    let mut trace_low = TraceGraph::new();
    let (low_thr, _) = ignite_traced(&source, &player, &enemy, &cfg, 500.0, &mut trace_low);
    let mut trace_high = TraceGraph::new();
    let (high_thr, _) = ignite_traced(&source, &player, &enemy, &cfg, 5000.0, &mut trace_high);

    // Lower threshold → higher chance → higher expected DPS
    assert!(low_thr.chance > high_thr.chance);
    assert!(low_thr.expected_dps > high_thr.expected_dps);
}

// Step 3 (Lane B): chill effect / freeze+electrocute poise buildup / stacked weighted average

// Chill effect (chill-effect-missing)

/// Chill minimum threshold: returns 0 (discarded) below 30% magnitude, PoE2 0.5.0.
///
/// Source: agent-docs/ailments.md §Chill, PoB2 `nonDamagingAilmentsConfig.Chill` clamp [30,50],
///   `chillMinimumThreshold = enemyThreshold / ChillEffectMultiplier` (discarded below 30%).
#[test]
fn chill_effect_below_min_is_discarded() {
    // hit = 0 → not applied
    assert_eq!(chill_effect(0.0, 1000.0), 0.0);
    // threshold = 0 → not applied
    assert_eq!(chill_effect(500.0, 0.0), 0.0);
    // hit < 30% of threshold → magnitude < 30 → discarded
    // ratio = 100/1000 = 0.1 → raw = 100 * 0.1 = 10 < 30 → 0
    assert_eq!(chill_effect(100.0, 1000.0), 0.0);
    // ratio = 0.29 → raw = 29 < 30 → 0
    assert_eq!(chill_effect(290.0, 1000.0), 0.0);
}

/// Chill floor: hit at exactly 30% of threshold → effect is exactly 30% (the minimum apply threshold).
///
/// Source: agent-docs/ailments.md §Chill `min=30`.
#[test]
fn chill_effect_at_minimum_threshold() {
    // ratio = 300/1000 = 0.3 → raw = 100 * 0.3 = 30.0 → clamp [30,50] → 30
    let effect = chill_effect(300.0, 1000.0);
    assert!(
        (effect - 30.0).abs() < 1e-6,
        "30% threshold hit → chill 30%, got {effect}"
    );
}

/// Chill ceiling: damage above 50% of threshold clamps to 50%.
///
/// Source: agent-docs/ailments.md §Chill `max=50 (ChillMaxEffect)`,
///   PoB2 `data.gameConstants["ChillMaxEffect"] = 50`.
#[test]
fn chill_effect_clamped_at_maximum() {
    // ratio ≥ 0.5 → raw ≥ 50 → clamp 50
    let at_max = chill_effect(500.0, 1000.0);
    assert!(
        (at_max - 50.0).abs() < 1e-6,
        "50% threshold hit → chill 50%"
    );
    let over_max = chill_effect(10_000.0, 1000.0);
    assert!(
        (over_max - 50.0).abs() < 1e-6,
        "huge hit → chill capped 50%"
    );
}

/// Chill linear scaling: doubling the damage doubles the effect (within [30,50]).
///
/// Source: PoB2 `chillEffect = 100 * (damage/threshold)` is a linear formula (not a power law).
#[test]
fn chill_effect_linear_scaling() {
    // ratio 0.35 → raw 35 → 35.0 (linear within [30,50])
    let e35 = chill_effect(350.0, 1000.0);
    assert!(
        (e35 - 35.0).abs() < 1e-6,
        "ratio 0.35 → chill 35%, got {e35}"
    );
    // ratio 0.45 → raw 45 → 45.0
    let e45 = chill_effect(450.0, 1000.0);
    assert!(
        (e45 - 45.0).abs() < 1e-6,
        "ratio 0.45 → chill 45%, got {e45}"
    );
    // linear: e45 / e35 ≈ 45/35 ≈ 1.286
    assert!(e45 > e35, "larger hit → larger chill");
}

/// Chill with effectMod: +100% AilmentMagnitude doubles the effect (if under the cap).
///
/// Source: agent-docs/ailments.md §`effectMod`.
#[test]
fn chill_effect_with_mods_scales_with_effect_mod() {
    // base ratio = 0.30 → raw = 30, effectMod = 2.0 → raw = 60 → clamp 50
    let with_mod = chill_effect_with_mods(300.0, 1000.0, 2.0);
    assert!(
        (with_mod - 50.0).abs() < 1e-6,
        "effectMod=2 → clamped to 50%, got {with_mod}"
    );
    // effectMod = 1.2 → raw = 36 → 36
    let e36 = chill_effect_with_mods(300.0, 1000.0, 1.2);
    assert!(
        (e36 - 36.0).abs() < 1e-6,
        "effectMod=1.2 → chill 36%, got {e36}"
    );
}

/// Chill traced: attribution nodes are written to TraceGraph correctly, and the effect
/// value matches the non-traced version.
#[test]
fn chill_traced_writes_trace_and_matches_non_traced() {
    let cfg = CalcConfig::attack();
    let player = ModDb::new();
    let mut trace = TraceGraph::new();

    let (effect, node) = chill_traced(350.0, 1000.0, &player, &cfg, &mut trace);
    let expected = chill_effect(350.0, 1000.0);
    assert!(
        (effect - expected).abs() < 1e-6,
        "traced chill should match non-traced: got {effect}, expected {expected}"
    );
    assert!(trace.node(node).is_some(), "ChillEffect node should exist");
    assert!(!trace.nodes().is_empty(), "trace should have nodes");
}

/// Chill traced with an AilmentMagnitude mod: effectMod is aggregated through ModDb,
/// scaling the effect value correctly.
#[test]
fn chill_traced_with_ailment_magnitude_mod() {
    let cfg = CalcConfig::attack();
    let mut player = ModDb::new();
    // +50% AilmentMagnitude → effectMod = 1.5 → raw = 100 * 0.35 * 1.5 = 52.5 → clamp 50
    player.add_mod(Modifier::number("AilmentMagnitude", ModType::Inc, 50.0));
    let mut trace = TraceGraph::new();

    let (effect, _) = chill_traced(350.0, 1000.0, &player, &cfg, &mut trace);
    assert!(
        (effect - 50.0).abs() < 1e-6,
        "+50% AilmentMagnitude: ratio=0.35*1.5=0.525 → clamp 50, got {effect}"
    );
    // trace should have an AilmentMagnitude inc node
    let has_mag = trace
        .nodes()
        .iter()
        .any(|n| n.label.contains("Chill magnitude"));
    assert!(
        has_mag,
        "trace should record chill magnitude mod contribution"
    );
}

// Freeze/electrocute poise buildup (freeze-electrocute-buildup-missing)

/// Freeze poise buildup decreases monotonically with the poise threshold: lower
/// threshold → higher buildup % per hit.
///
/// Source: agent-docs/ailments.md §Freeze buildup:
///   `poiseBuildup = FREEZE_DAMAGE_SCALE / enemyPoiseThreshold * inc_more * 100`
///   `FREEZE_DAMAGE_SCALE = 2.1`.
#[test]
fn freeze_poise_buildup_decreases_with_poise_threshold() {
    // threshold = 0 → 0 (safety)
    assert_eq!(freeze_poise_buildup(0.0, 0.0, 1.0), 0.0);

    // threshold = 210 → buildup = 2.1/210 * 100 = 1.0%
    let low_thr = freeze_poise_buildup(210.0, 0.0, 1.0);
    assert!(
        (low_thr - 1.0).abs() < 1e-6,
        "poise=210 → freeze buildup 1%, got {low_thr}"
    );

    // threshold = 2100 → buildup = 2.1/2100 * 100 = 0.1%
    let high_thr = freeze_poise_buildup(2100.0, 0.0, 1.0);
    assert!(
        (high_thr - 0.1).abs() < 1e-6,
        "poise=2100 → freeze buildup 0.1%, got {high_thr}"
    );

    // verify monotonic decrease
    assert!(
        low_thr > high_thr,
        "lower threshold → higher buildup per hit"
    );
}

/// Freeze poise buildup scales linearly with inc/more.
///
/// Source: PoB2 `poiseBuildup = ... * (1 + inc/100) * more * 100`.
#[test]
fn freeze_poise_buildup_scales_with_inc_and_more() {
    let base = freeze_poise_buildup(1000.0, 0.0, 1.0);

    // +100% inc → 2×
    let with_inc = freeze_poise_buildup(1000.0, 100.0, 1.0);
    assert!(
        (with_inc - base * 2.0).abs() < 1e-6,
        "+100% inc → 2× buildup, base={base} with_inc={with_inc}"
    );

    // more = 1.5 → 1.5×
    let with_more = freeze_poise_buildup(1000.0, 0.0, 1.5);
    assert!(
        (with_more - base * 1.5).abs() < 1e-6,
        "more=1.5 → 1.5× buildup"
    );
}

/// Electrocute poise buildup base value (ELECTROCUTE_DAMAGE_SCALE = 1.7).
///
/// Source: PoB2 `data.gameConstants["ElectrocuteDamageScale"] = 1.7`.
#[test]
fn electrocute_poise_buildup_uses_correct_scale() {
    // threshold = 170 → buildup = 1.7/170 * 100 = 1.0%
    let buildup = electrocute_poise_buildup(170.0, 0.0, 1.0);
    assert!(
        (buildup - 1.0).abs() < 1e-6,
        "electrocute poise=170 → 1%, got {buildup}"
    );

    // electrocute vs freeze scale ratio = 1.7/2.1 (electrocute builds up slower at equal threshold)
    let freeze_b = freeze_poise_buildup(1000.0, 0.0, 1.0);
    let elec_b = electrocute_poise_buildup(1000.0, 0.0, 1.0);
    let ratio = elec_b / freeze_b;
    assert!(
        (ratio - 1.7 / 2.1).abs() < 1e-6,
        "electrocute/freeze scale ratio should be 1.7/2.1, got {ratio}"
    );
}

/// Freeze poise buildup traced: nodes are written to TraceGraph, and the buildup
/// value matches the non-traced version.
#[test]
fn freeze_poise_buildup_traced_writes_trace() {
    let cfg = CalcConfig::attack();
    let player = ModDb::new();
    let mut trace = TraceGraph::new();

    let (buildup, node) = freeze_poise_buildup_traced(1000.0, &player, &cfg, &mut trace);
    let expected = freeze_poise_buildup(1000.0, 0.0, 1.0);
    assert!(
        (buildup - expected).abs() < 1e-6,
        "traced freeze buildup should match non-traced"
    );
    assert!(
        trace.node(node).is_some(),
        "FreezePoiseBuildup node should exist"
    );
}

/// Electrocute poise buildup traced with a mod: `EnemyElectrocuteBuildup` inc scales
/// the buildup correctly.
#[test]
fn electrocute_poise_buildup_traced_with_mod() {
    let cfg = CalcConfig::attack();
    let mut player = ModDb::new();
    // +100% EnemyElectrocuteBuildup → 2× buildup
    player.add_mod(Modifier::number(
        "EnemyElectrocuteBuildup",
        ModType::Inc,
        100.0,
    ));
    let mut trace = TraceGraph::new();

    let (buildup, _) = electrocute_poise_buildup_traced(1000.0, &player, &cfg, &mut trace);
    let base = electrocute_poise_buildup(1000.0, 0.0, 1.0);
    assert!(
        (buildup - base * 2.0).abs() < 1e-6,
        "+100% inc → 2× electrocute buildup, expected {}, got {}",
        base * 2.0,
        buildup
    );
    let has_inc = trace
        .nodes()
        .iter()
        .any(|n| n.label.contains("Electrocute poise buildup"));
    assert!(has_inc, "trace should record electrocute buildup inc");
}

// Stacked weighted-average DPS (ailment-stacking)

/// Default single stack (StackConfig::single()): DPS = single_layer_dps × 1.
///
/// Source: agent-docs/ailments.md §Stacking `ailmentDPS = baseVal * activeAilments * ...`.
#[test]
fn stacking_ailment_dps_single_layer() {
    let cfg = StackConfig::single();
    let dps = stacking_ailment_dps(100.0, &cfg);
    assert!(
        (dps - 100.0).abs() < 1e-6,
        "single layer → DPS unchanged, got {dps}"
    );
}

/// Stacked DPS grows linearly with active_stacks (replaces the Wave1d single-layer
/// expected-value simplification).
///
/// Source: agent-docs/ailments.md §Stacking `activeAilments` multiplier.
#[test]
fn stacking_ailment_dps_scales_with_active_stacks() {
    let cfg = StackConfig::new(5, 3.0);
    let dps = stacking_ailment_dps(100.0, &cfg);
    // 3 active stacks × 100 DPS/stack = 300
    assert!(
        (dps - 300.0).abs() < 1e-6,
        "3 active stacks × 100 = 300, got {dps}"
    );

    // active_stacks = 0 degrades to max_stacks
    let cfg_no_active = StackConfig::new(4, 0.0);
    let dps_max = stacking_ailment_dps(100.0, &cfg_no_active);
    assert!(
        (dps_max - 400.0).abs() < 1e-6,
        "active=0 → use max_stacks=4 → 400, got {dps_max}"
    );
}

/// StackPotential = active/max, clamped to [0,1].
///
/// Source: PoB2 `StackPotential = ailmentStacks / maxStacks`.
#[test]
fn stack_potential_is_ratio_of_active_to_max() {
    let cfg = StackConfig::new(10, 5.0);
    let sp = stack_potential(&cfg);
    assert!((sp - 0.5).abs() < 1e-6, "5/10 → potential 0.5, got {sp}");

    // Overflow: active > max → SP = 8/5 = 1.6 (PoB2 does not clamp; can exceed 1, triggering
    // the over-stacking amplification)
    let cfg_over = StackConfig::new(5, 8.0);
    assert!(
        (stack_potential(&cfg_over) - 1.6).abs() < 1e-6,
        "overflow → potential 8/5 = 1.6 (PoB2 不 clamp)"
    );

    // default single stack
    let sp_single = stack_potential(&StackConfig::single());
    assert_eq!(sp_single, 1.0, "single stack → potential 1.0");
}

/// RollAverage: fixed at 50% when not overflowing; skews toward the high end when overflowing.
///
/// Source: PoB2 `CalcOffence.lua` RollAverage section.
#[test]
fn roll_average_at_midpoint_when_not_overflow() {
    // active = max → exactly not overflowing → 50
    let cfg = StackConfig::new(5, 5.0);
    assert!(
        (roll_average(&cfg) - 50.0).abs() < 1e-6,
        "active=max → roll 50"
    );

    // active = 0 (fallback to max) → 50
    let cfg2 = StackConfig::new(3, 0.0);
    assert!(
        (roll_average(&cfg2) - 50.0).abs() < 1e-6,
        "active=0→max → roll 50"
    );
}

#[test]
fn roll_average_shifted_high_when_overflow() {
    // active=10, max=5 → overflow → roll > 50
    let cfg = StackConfig::new(5, 10.0);
    let ra = roll_average(&cfg);
    assert!(ra > 50.0, "overflow → roll_avg > 50, got {ra}");
    // formula: (10 - (5-1)/2) / (10+1) * 100 = (10-2)/11*100 = 8/11*100 ≈ 72.73
    assert!(
        (ra - 72.727_272_727).abs() < 1e-3,
        "overflow formula check: got {ra}"
    );
}

/// 05-01 active stack estimate: `stacks = hitChance × applyChance × duration × speed`.
#[test]
fn estimate_active_stacks_is_product_of_signals() {
    // 1.0 hit × 1.0 apply × 5s duration × 4/s rate = 20 stacks.
    let s = estimate_active_stacks(1.0, 1.0, 5.0, 4.0);
    assert!((s - 20.0).abs() < 1e-6, "1×1×5×4 = 20, got {s}");

    // Partial hit/apply scales proportionally: 0.9 × 0.5 × 4 × 2 = 3.6.
    let s2 = estimate_active_stacks(0.9, 0.5, 4.0, 2.0);
    assert!((s2 - 3.6).abs() < 1e-6, "0.9×0.5×4×2 = 3.6, got {s2}");
}

/// Returns 0 when any signal is missing (no rate / no duration / no hit / no apply
/// chance), falling back to the max_stacks upper bound.
#[test]
fn estimate_active_stacks_zero_when_any_signal_missing() {
    assert_eq!(estimate_active_stacks(1.0, 1.0, 5.0, 0.0), 0.0, "no speed");
    assert_eq!(
        estimate_active_stacks(1.0, 1.0, 0.0, 4.0),
        0.0,
        "no duration"
    );
    assert_eq!(estimate_active_stacks(0.0, 1.0, 5.0, 4.0), 0.0, "no hit");
    assert_eq!(estimate_active_stacks(1.0, 0.0, 5.0, 4.0), 0.0, "no chance");
}

/// 05-04 RollAverage high-end shift: `cross_type_source_hit_at_roll` interpolates over
/// [min,max]. roll=50 degenerates to the interval midpoint (= `cross_type_source_hit`);
/// roll>50 shifts toward the high end.
#[test]
fn cross_type_source_hit_shifts_with_roll() {
    use pobr_core::calc::DamageComponent;
    let player = ModDb::new();
    let cfg = CalcConfig::attack();
    // Physical component [600, 1400], span 800.
    let components = vec![DamageComponent::new(DamageType::Physical, 600.0, 1400.0)];

    // roll=50 → midpoint 1000 (matches cross_type_source_hit).
    let mid = cross_type_source_hit_at_roll(AilmentType::Bleed, &components, &player, &cfg, 50.0);
    let legacy = cross_type_source_hit(AilmentType::Bleed, &components, &player, &cfg);
    assert!((mid - 1000.0).abs() < 1e-6, "roll 50 → 1000, got {mid}");
    assert!((mid - legacy).abs() < 1e-6, "roll 50 == legacy avg");

    // roll=75 → 600 + 800×0.75 = 1200 (shifted toward the high end).
    let high = cross_type_source_hit_at_roll(AilmentType::Bleed, &components, &player, &cfg, 75.0);
    assert!((high - 1200.0).abs() < 1e-6, "roll 75 → 1200, got {high}");
    assert!(high > mid, "high roll shifts source hit up");
}

/// Stacked DPS traced: nodes are written to TraceGraph, and DPS matches the non-traced version.
#[test]
fn stacking_ailment_dps_traced_writes_trace() {
    let cfg = StackConfig::new(3, 3.0);
    let mut trace = TraceGraph::new();
    let (dps, node) = stacking_ailment_dps_traced(100.0, &cfg, AilmentType::Bleed, &mut trace);
    assert!(
        (dps - 300.0).abs() < 1e-6,
        "traced stacked dps = 300, got {dps}"
    );
    assert!(
        trace.node(node).is_some(),
        "BleedStackedDPS node should exist"
    );
    // ActiveStacks node should exist and feed into StackedDPS
    let has_stacks = trace
        .nodes()
        .iter()
        .any(|n| n.label.contains("BleedActiveStacks"));
    assert!(has_stacks, "trace should have BleedActiveStacks node");
    let incoming = trace.incoming(node);
    assert!(
        !incoming.is_empty(),
        "stacked DPS node should have incoming edge from stacks node"
    );
}

/// Bleed stacking DPS integration test: verifies the full chain from bleed_instance +
/// stacking (bleed/poison stacking).
///
/// Source: agent-docs/ailments.md §Stacking and weighted average (bleed defaults to 5
/// max stacks, 3 active).
#[test]
fn bleed_stacking_dps_integration() {
    let gc = GameConstants::poe2();
    // 1000 physical hit → 150 DPS per stack (bleed_base_fraction=0.15)
    let instance = bleed_instance(1000.0, &ModDb::new(), &CalcConfig::attack());
    assert!((instance.magnitude_dps - 1000.0 * gc.bleed_base_fraction).abs() < 1e-6);

    // 3 active stacks: DPS = 150 × 3 = 450
    let stack_cfg = StackConfig::new(5, 3.0);
    let stacked = stacking_ailment_dps(instance.magnitude_dps, &stack_cfg);
    assert!(
        (stacked - 450.0).abs() < 1e-6,
        "3-stack bleed DPS = 450, got {stacked}"
    );

    // Poison stacking: 4 active stacks × 200 DPS (poison_base_fraction=0.20, hit=1000)
    let poison_inst =
        pobr_core::calc::ailment::poison_instance(1000.0, &ModDb::new(), &CalcConfig::attack());
    let p_stack = StackConfig::new(10, 4.0);
    let p_stacked = stacking_ailment_dps(poison_inst.magnitude_dps, &p_stack);
    let expected_p = 1000.0 * gc.poison_base_fraction * 4.0;
    assert!(
        (p_stacked - expected_p).abs() < 1e-6,
        "4-stack poison DPS = {expected_p}, got {p_stacked}"
    );
}

// Feature 1: AilmentEffect / Faster / Slower dimensions

/// `AilmentEffect` MORE multiplier bucket: defaults to 1.0 (neutral) with no mods.
///
/// Source: PoB2 `CalcOffence.lua` l.5190
///   `local effectMod = calcLib.mod(skillModList, dotCfg, "AilmentEffect")` (MORE aggregation).
#[test]
fn ailment_effect_mod_defaults_to_one() {
    let cfg = CalcConfig::attack();
    let db = ModDb::new();
    let eff = ailment_effect_mod(&db, &cfg);
    assert!(
        (eff - 1.0).abs() < 1e-9,
        "no AilmentEffect mods → effectMod=1.0, got {eff}"
    );
}

/// `AilmentEffect` MORE aggregation: two more mods multiply together.
///
/// Source: PoB2 `calcLib.mod` = MORE multiplicative semantics.
#[test]
fn ailment_effect_mod_is_product_of_more_mods() {
    let cfg = CalcConfig::attack();
    let mut db = ModDb::new();
    // Two independent AilmentEffect More mods: 1.5 × 1.2 = 1.8 (ModDb uses the Inc/More
    // convention: More mods multiply in ModDb.more() as "1 + value/100", so 0.5 means
    // a +50% more mod).
    // Using More type directly (value is the added percentage): 0.5 → factor 1.5, 0.2 → factor 1.2.
    db.add_mod(Modifier::number("AilmentEffect", ModType::More, 50.0));
    db.add_mod(Modifier::number("AilmentEffect", ModType::More, 20.0));
    let eff = ailment_effect_mod(&db, &cfg);
    let expected = 1.5 * 1.2;
    assert!(
        (eff - expected).abs() < 1e-9,
        "50% more × 20% more → {expected}, got {eff}"
    );
}

/// `ailment_rate_mod`: with no Faster/Slower mods → `faster / slower = 1.0/1.0 = 1.0`.
///
/// Source: PoB2 `CalcOffence.lua` l.5035
///   `rateMod = calcLib.mod(skillModList, cfg, ailment.."Faster") / calcLib.mod(..., ailment.."Slower")`.
#[test]
fn ailment_rate_mod_defaults_to_one() {
    let cfg = CalcConfig::attack();
    let db = ModDb::new();
    let rm = ailment_rate_mod(&db, &db, &cfg, "Bleed");
    assert!(
        (rm - 1.0).abs() < 1e-9,
        "no Faster/Slower mods → rateMod=1.0, got {rm}"
    );
}

/// `ailment_rate_mod` + Faster: `BleedFaster More 50% → faster=1.5, rateMod=1.5`.
///
/// Source: PoB2 rateMod = mod(BleedFaster) (MORE) / mod(BleedSlower) (MORE = 1.0 default).
#[test]
fn ailment_rate_mod_scales_with_faster() {
    let cfg = CalcConfig::attack();
    let mut player = ModDb::new();
    player.add_mod(Modifier::number("BleedFaster", ModType::More, 50.0)); // ×1.5
    let enemy = ModDb::new();
    let rm = ailment_rate_mod(&player, &enemy, &cfg, "Bleed");
    assert!(
        (rm - 1.5).abs() < 1e-9,
        "+50% BleedFaster → rateMod=1.5, got {rm}"
    );
}

/// (k3): the INC leg of `ailment_rate_mod` — vendor `calcLib.mod` (CalcTools.lua:16-18)
/// = `(1 + ΣINC/100) × ΠMORE`; the statmap `faster_burn_%` family produces INC (SkillStatMap.lua:843-848).
///
/// `IgniteFaster INC 50 + MORE 20 → faster = 1.5 × 1.2 = 1.8`.
#[test]
fn ailment_rate_mod_includes_inc_leg() {
    let cfg = CalcConfig::attack();
    let mut player = ModDb::new();
    player.add_mod(Modifier::number("IgniteFaster", ModType::Inc, 50.0));
    player.add_mod(Modifier::number("IgniteFaster", ModType::More, 20.0));
    let enemy = ModDb::new();
    let rm = ailment_rate_mod(&player, &enemy, &cfg, "Ignite");
    assert!(
        (rm - 1.8).abs() < 1e-9,
        "IgniteFaster INC50 + MORE20 → rateMod=1.8, got {rm}"
    );

    // Slower's INC leg is symmetric: BleedSlower INC 100 → slower=2.0 → rateMod=0.5.
    let mut slower_player = ModDb::new();
    slower_player.add_mod(Modifier::number("BleedSlower", ModType::Inc, 100.0));
    let rm = ailment_rate_mod(&slower_player, &enemy, &cfg, "Bleed");
    assert!(
        (rm - 0.5).abs() < 1e-9,
        "BleedSlower INC100 → rateMod=0.5, got {rm}"
    );
}

/// `ailment_rate_mod` + Slower: `BleedSlower More 25% → slower=1.25, rateMod = 1.0/1.25`.
///
/// Source: PoB2 `rateMod = faster / slower`; Slower pulls rateMod below 1.
#[test]
fn ailment_rate_mod_reduced_by_slower() {
    let cfg = CalcConfig::attack();
    let mut player = ModDb::new();
    player.add_mod(Modifier::number("BleedSlower", ModType::More, 25.0)); // ×1.25
    let enemy = ModDb::new();
    let rm = ailment_rate_mod(&player, &enemy, &cfg, "Bleed");
    let expected = 1.0 / 1.25;
    assert!(
        (rm - expected).abs() < 1e-9,
        "+25% BleedSlower → rateMod={expected:.4}, got {rm}"
    );
}

/// `apply_rate_mod_to_instance`: DPS × rateMod, duration / rateMod (total damage unchanged).
///
/// Source: PoB2 `ailmentDPS *= rateMod`; `duration /= rateMod`; verifies total damage conservation.
#[test]
fn apply_rate_mod_scales_dps_and_shrinks_duration() {
    let inst = bleed_instance(1000.0, &ModDb::new(), &CalcConfig::attack());
    let rate_mod = 2.0;
    let modified = apply_rate_mod_to_instance(inst, rate_mod);

    // DPS × 2
    assert!(
        (modified.magnitude_dps - inst.magnitude_dps * 2.0).abs() < 1e-3,
        "DPS should double: {} → {}",
        inst.magnitude_dps,
        modified.magnitude_dps
    );
    // duration ÷ 2
    assert!(
        (modified.duration_secs - inst.duration_secs / 2.0).abs() < 1e-3,
        "duration should halve: {} → {}",
        inst.duration_secs,
        modified.duration_secs
    );
    // Total damage conservation: DPS × duration
    let total_before = inst.magnitude_dps * inst.duration_secs;
    let total_after = modified.magnitude_dps * modified.duration_secs;
    assert!(
        (total_after - total_before).abs() < 1e-3,
        "total damage should be conserved: before={total_before} after={total_after}"
    );
}

/// `apply_effect_mod_to_instance`: magnitude_dps × effectMod, duration unchanged.
///
/// Source: PoB2 `ailmentDPS = baseVal * effectMod * ...`; effectMod does not affect duration.
#[test]
fn apply_effect_mod_scales_dps_not_duration() {
    let inst = bleed_instance(1000.0, &ModDb::new(), &CalcConfig::attack());
    let effect_mod = 1.5;
    let modified = apply_effect_mod_to_instance(inst, effect_mod);

    // DPS × 1.5
    assert!(
        (modified.magnitude_dps - inst.magnitude_dps * 1.5).abs() < 1e-3,
        "DPS should be ×1.5: {} → {}",
        inst.magnitude_dps,
        modified.magnitude_dps
    );
    // duration unchanged
    assert!(
        (modified.duration_secs - inst.duration_secs).abs() < 1e-9,
        "duration should not change with effectMod"
    );
}

/// `apply_effect_and_rate_mod` combined: DPS × effectMod × rateMod, duration / rateMod.
///
/// Source: PoB2 `ailmentDPS = baseVal * effectMod * rateMod * activeAilments * effMult`.
#[test]
fn apply_effect_and_rate_mod_combines_both() {
    let inst = bleed_instance(1000.0, &ModDb::new(), &CalcConfig::attack());
    let base_dps = inst.magnitude_dps;
    let base_dur = inst.duration_secs;
    let effect_mod = 1.5;
    let rate_mod = 2.0;

    let modified = apply_effect_and_rate_mod(inst, effect_mod, rate_mod);

    // DPS = base × 1.5 × 2.0 = base × 3.0
    let expected_dps = base_dps * effect_mod * rate_mod;
    assert!(
        (modified.magnitude_dps - expected_dps).abs() < 1e-2,
        "DPS × effectMod × rateMod: expected {expected_dps:.2}, got {:.2}",
        modified.magnitude_dps
    );
    // duration = base / rateMod (effectMod does not affect duration)
    let expected_dur = base_dur / rate_mod;
    assert!(
        (modified.duration_secs - expected_dur).abs() < 1e-3,
        "duration / rateMod: expected {expected_dur:.3}, got {:.3}",
        modified.duration_secs
    );
}

/// `apply_effect_and_rate_mod_traced` writes nodes to TraceGraph.
///
/// Source: attribution requires effectMod / rateMod to each have their own node feeding
/// into the magnitude node.
#[test]
fn apply_effect_and_rate_mod_traced_writes_nodes() {
    let mut trace = TraceGraph::new();
    // Add a dummy magnitude node as the target
    let mag_node = trace.add_node("BleedMagnitude", 100.0, TraceOperation::Multiply);

    let inst = bleed_instance(1000.0, &ModDb::new(), &CalcConfig::attack());
    let modified = apply_effect_and_rate_mod_traced(inst, 1.5, 2.0, "Bleed", mag_node, &mut trace);

    // DPS has been adjusted
    let expected_dps = inst.magnitude_dps * 1.5 * 2.0;
    assert!(
        (modified.magnitude_dps - expected_dps).abs() < 1e-2,
        "traced: DPS should be {expected_dps:.2}"
    );
    // trace should have EffectMod and RateMod nodes
    let has_effect = trace
        .nodes()
        .iter()
        .any(|n| n.label.contains("BleedEffectMod"));
    let has_rate = trace
        .nodes()
        .iter()
        .any(|n| n.label.contains("BleedRateMod"));
    assert!(has_effect, "trace should have BleedEffectMod node");
    assert!(has_rate, "trace should have BleedRateMod node");
    // Both nodes should feed into mag_node
    let incoming = trace.incoming(mag_node);
    assert!(
        incoming.len() >= 2,
        "mag_node should have ≥2 incoming edges (effectMod + rateMod)"
    );
}

// Feature 2: cross-type application (<Type>Can<Ailment>)

/// Default: a fire hit does not apply bleed — only a physical hit counts as a bleed source.
///
/// Source: agent-docs/ailments.md §elemental/non-elemental classification; Bleed defaults
/// to ScalesFrom=Physical.
#[test]
fn cross_type_source_hit_defaults_to_physical_for_bleed() {
    use pobr_core::calc::DamageComponent;
    let cfg = CalcConfig::attack();
    let player = ModDb::new(); // no FireCanBleed flag

    let components = vec![
        DamageComponent::new(DamageType::Physical, 800.0, 1200.0), // avg=1000
        DamageComponent::new(DamageType::Fire, 400.0, 600.0),      // avg=500, excluded
    ];

    let hit = cross_type_source_hit(AilmentType::Bleed, &components, &player, &cfg);
    assert!(
        (hit - 1000.0).abs() < 1e-6,
        "Bleed default source: Physical avg=1000, got {hit}"
    );
}

/// `FireCanBleed` flag: fire damage now also counts toward the bleed source hit.
///
/// Source: agent-docs/ailments.md §exceptions that rewrite the application rules
///   (Blood Barbs' FireCanBleed etc.), PoB2 `canDoAilment` l.4806
///   `skillModList:Flag(cfg, type.."Can"..damagingAilment)`.
#[test]
fn cross_type_source_hit_fire_can_bleed_adds_fire_damage() {
    use pobr_core::calc::DamageComponent;
    let cfg = CalcConfig::attack();
    let mut player = ModDb::new();
    player.add_mod(Modifier::flag("FireCanBleed"));

    let components = vec![
        DamageComponent::new(DamageType::Physical, 800.0, 1200.0), // avg=1000
        DamageComponent::new(DamageType::Fire, 400.0, 600.0),      // avg=500, now included
    ];

    let hit = cross_type_source_hit(AilmentType::Bleed, &components, &player, &cfg);
    assert!(
        (hit - 1500.0).abs() < 1e-6,
        "FireCanBleed: Physical(1000)+Fire(500)=1500, got {hit}"
    );
}

/// `ChaosCanShock` flag: chaos damage now also counts toward the shock source.
///
/// Source: agent-docs/ailments.md §exceptions (Voltaxic Rift: ChaosCanShock),
///   PoB2 l.4806 `type.."Can"..damagingAilment`.
#[test]
fn cross_type_source_hit_chaos_can_shock() {
    use pobr_core::calc::DamageComponent;
    let cfg = CalcConfig::attack();
    let mut player = ModDb::new();
    player.add_mod(Modifier::flag("ChaosCanShock"));

    let components = vec![
        DamageComponent::new(DamageType::Lightning, 500.0, 700.0), // avg=600, default shock source
        DamageComponent::new(DamageType::Chaos, 200.0, 400.0),     // avg=300, now ChaosCanShock
    ];

    let hit = cross_type_source_hit(AilmentType::Shock, &components, &player, &cfg);
    assert!(
        (hit - 900.0).abs() < 1e-6,
        "ChaosCanShock: Lightning(600)+Chaos(300)=900, got {hit}"
    );
}

/// Returns 0 when there are no hit components.
#[test]
fn cross_type_source_hit_empty_components() {
    let cfg = CalcConfig::attack();
    let player = ModDb::new();
    let components = [];
    let hit = cross_type_source_hit(AilmentType::Bleed, &components, &player, &cfg);
    assert_eq!(hit, 0.0, "empty components → 0");
}

// Feature 3: DotDpsCap

/// `apply_dot_dps_cap`: returns the DPS unchanged when it's below the cap.
///
/// Source: PoB2 `ailmentDPSCapped = m_min(ailmentDPSUncapped, data.misc.DotDpsCap)`.
#[test]
fn apply_dot_dps_cap_passthrough_below_cap() {
    let dps = 1_000_000.0;
    let capped = apply_dot_dps_cap(dps, DOT_DPS_CAP);
    assert!(
        (capped - dps).abs() < 1.0,
        "1M DPS < cap → unchanged, got {capped}"
    );
}

/// `apply_dot_dps_cap`: truncates to DOT_DPS_CAP (35,791,394) when the DPS exceeds it.
///
/// Source: PoB2 `Data.lua` `DotDpsCap = 35791394`.
#[test]
fn apply_dot_dps_cap_clamps_huge_dps() {
    use pobr_data::constants::DOT_DPS_CAP;
    let dps = DOT_DPS_CAP + 1_000_000.0;
    let capped = apply_dot_dps_cap(dps, DOT_DPS_CAP);
    assert!(
        (capped - DOT_DPS_CAP).abs() < 1.0,
        "huge DPS clamped to DOT_DPS_CAP={DOT_DPS_CAP}, got {capped}"
    );
}

/// 05-07 hardening: `apply_dot_dps_cap` always uses the constant `DOT_DPS_CAP`, **ignoring
/// any `DotDpsCap` in modDB** (in PoB2 this cap is a `Data.lua` hardcoded constant with no
/// Override/modDB mechanism).
///
/// Source: across the PoB2 source, `m_min(_, data.misc.DotDpsCap)`; grep finds no
/// `Override(..,"DotDpsCap")`.
#[test]
fn apply_dot_dps_cap_ignores_moddb_dotdpscap() {
    // Even writing a low DotDpsCap value into modDB does not affect the cap
    // (PoB2-faithful: the constant is always used).
    let dps = 50_000.0; // far below DOT_DPS_CAP
    let capped = apply_dot_dps_cap(dps, DOT_DPS_CAP);
    assert!(
        (capped - dps).abs() < 1.0,
        "50k < DOT_DPS_CAP → 原样返回（modDB DotDpsCap 不生效），got {capped}"
    );
}

/// `dps_with_effect_rate_cap`: effect + rate apply together, then the result is
/// truncated by the cap.
///
/// Source: PoB2 `ailmentDPS = m_min(baseVal * effectMod * rateMod * ..., DotDpsCap)`.
#[test]
fn dps_with_effect_rate_cap_applies_cap() {
    use pobr_data::constants::DOT_DPS_CAP;

    // Set an oversized base_dps to guarantee effectMod × rateMod exceeds the cap
    let base_dps = DOT_DPS_CAP * 0.6; // 60% of cap
    let effect_mod = 2.0; // × 2.0 → 120% of cap → exceeds the cap
    let rate_mod = 1.0;

    let result = dps_with_effect_rate_cap(base_dps, effect_mod, rate_mod, DOT_DPS_CAP);
    assert!(
        (result - DOT_DPS_CAP).abs() < 1.0,
        "base × 2.0 exceeds cap → clamped to DOT_DPS_CAP, got {result}"
    );
}

/// `dps_with_effect_rate_cap_traced`: when DPS gets truncated, trace should have a
/// DotDpsCap node.
///
/// Source: DotDpsCap truncation should be attributable via TraceGraph (incremental
/// attribution value).
#[test]
fn dps_with_effect_rate_cap_traced_adds_cap_node_when_truncated() {
    use pobr_data::constants::DOT_DPS_CAP;
    let mut trace = TraceGraph::new();

    // Case where the cap is exceeded
    let base_dps = DOT_DPS_CAP * 0.7;
    let (result, node) =
        dps_with_effect_rate_cap_traced(base_dps, 2.0, 1.0, DOT_DPS_CAP, "Ignite", &mut trace);

    // Result truncated to the cap
    assert!(
        (result - DOT_DPS_CAP).abs() < 1.0,
        "capped result should be DOT_DPS_CAP, got {result}"
    );
    // trace should have a DotDpsCap node
    let has_cap = trace.nodes().iter().any(|n| n.label.contains("DotDpsCap"));
    assert!(has_cap, "trace should have DotDpsCap node when truncated");
    // Output node exists
    assert!(trace.node(node).is_some(), "output node should exist");
}

/// `dps_with_effect_rate_cap_traced`: when DPS is not truncated, trace should **not**
/// have a DotDpsCap node.
#[test]
fn dps_with_effect_rate_cap_traced_no_cap_node_when_not_truncated() {
    let mut trace = TraceGraph::new();

    // A small base_dps that will not exceed the cap
    let (_, _) = dps_with_effect_rate_cap_traced(100.0, 1.0, 1.0, DOT_DPS_CAP, "Bleed", &mut trace);

    let has_cap = trace.nodes().iter().any(|n| n.label.contains("DotDpsCap"));
    assert!(!has_cap, "no cap node when DPS is below DOT_DPS_CAP");
}

// 05-01: ailment crit over-stacking correction (PoB2 CalcOffence.lua L5144
// ailmentCritChance = 100*(1-(1-c)^max(SP,1)))

#[test]
fn ailment_crit_chance_applies_over_stacking_correction() {
    let crit = 0.5_f64; // 50% single-hit crit chance (fraction)

    // SP <= 1: degenerates to the bare crit chance (the exponent is floored to 1 by max(.,1)).
    assert!((ailment_crit_chance(crit, 1.0) - 0.5).abs() < 1e-9);
    assert!((ailment_crit_chance(crit, 0.4) - 0.5).abs() < 1e-9);

    // SP > 1 (stack overflow): amplified. SP=2 → 1 - 0.5^2 = 0.75.
    let amplified = ailment_crit_chance(crit, 2.0);
    assert!((amplified - 0.75).abs() < 1e-9);
    assert!(amplified > crit, "over-stacking 应抬高暴击份额");

    // Boundaries: 0 crit chance always gives 0; full crit chance always gives 1.
    assert!((ailment_crit_chance(0.0, 3.0) - 0.0).abs() < 1e-9);
    assert!((ailment_crit_chance(1.0, 3.0) - 1.0).abs() < 1e-9);

    // End to end: the amplified crit share feeds into weighted_source_damage, giving
    // a base higher than SP=1.
    let src_sp1 = AilmentSource::new(1000.0, 2.0, ailment_crit_chance(crit, 1.0), false);
    let (_c0, base_sp1) = weighted_source_damage(&src_sp1, 100.0, 100.0);
    assert!(
        (base_sp1 - 1500.0).abs() < 1.0,
        "SP=1: 1000*0.5+2000*0.5=1500"
    );

    let src_over = AilmentSource::new(1000.0, 2.0, ailment_crit_chance(crit, 2.0), false);
    let (_c1, base_over) = weighted_source_damage(&src_over, 100.0, 100.0);
    assert!(
        (base_over - 1750.0).abs() < 1.0,
        "SP=2: 1000*0.25+2000*0.75=1750"
    );
    assert!(base_over > base_sp1, "over-stacking 应抬高异常 base 伤害");
}

// Stored family sources (stored_source_at_roll) + CHANCE_AILMENT merge

fn range(
    damage_type: DamageType,
    hit_min: f64,
    hit_max: f64,
    crit_min: f64,
    crit_max: f64,
) -> pobr_core::calc::StoredDamageRange {
    pobr_core::calc::StoredDamageRange {
        damage_type,
        hit_min,
        hit_max,
        crit_min,
        crit_max,
    }
}

/// Default type gating + interval midpoint: ignite only consumes the fire component;
/// roll=50 → both legs take (min+max)/2. The crit leg independently takes the Stored
/// crit interval (not an approximation of hit × CritMultiplier).
#[test]
fn stored_source_ignite_takes_fire_with_independent_crit_leg() {
    let ranges = vec![
        range(DamageType::Fire, 100.0, 200.0, 500.0, 700.0),
        range(DamageType::Cold, 1000.0, 2000.0, 3000.0, 4000.0),
    ];
    let (hit, crit) = pobr_core::calc::ailment::stored_source_at_roll(
        AilmentType::Ignite,
        &ranges,
        &ModDb::new(),
        &CalcConfig::attack(),
        50.0,
    );
    assert_eq!(hit, 150.0, "hit 腿 = 火分量中点");
    assert_eq!(crit, 600.0, "crit 腿独立取 Stored crit 区间中点");
}

/// Cross-type application: the `ColdCanIgnite` flag makes the cold component count
/// toward the ignite source (vendor canDoAilment override).
#[test]
fn stored_source_cross_type_flag_adds_component() {
    let ranges = vec![
        range(DamageType::Fire, 100.0, 200.0, 100.0, 200.0),
        range(DamageType::Cold, 1000.0, 2000.0, 1000.0, 2000.0),
    ];
    let mut db = ModDb::new();
    db.add_list(vec![Modifier::flag("ColdCanIgnite")]);
    let (hit, _) = pobr_core::calc::ailment::stored_source_at_roll(
        AilmentType::Ignite,
        &ranges,
        &db,
        &CalcConfig::attack(),
        50.0,
    );
    assert_eq!(hit, 150.0 + 1500.0, "ColdCanIgnite → 冰分量并入来源");
}

/// Per-type Buildup MORE (vendor `:4844`): `PhysicalBleedBuildup` only amplifies the
/// physical component.
#[test]
fn stored_source_applies_per_type_buildup_more() {
    let ranges = vec![range(DamageType::Physical, 100.0, 100.0, 100.0, 100.0)];
    let mut db = ModDb::new();
    db.add_list(vec![Modifier::number(
        "PhysicalBleedBuildup",
        ModType::More,
        50.0,
    )]);
    let (hit, crit) = pobr_core::calc::ailment::stored_source_at_roll(
        AilmentType::Bleed,
        &ranges,
        &db,
        &CalcConfig::attack(),
        50.0,
    );
    assert_eq!(hit, 150.0);
    assert_eq!(crit, 150.0);
}

/// RollAverage interpolation (vendor `:5125`): roll=75 → min + (max−min)×0.75.
#[test]
fn stored_source_interpolates_at_roll() {
    let ranges = vec![range(DamageType::Fire, 100.0, 300.0, 400.0, 800.0)];
    let (hit, crit) = pobr_core::calc::ailment::stored_source_at_roll(
        AilmentType::Ignite,
        &ranges,
        &ModDb::new(),
        &CalcConfig::attack(),
        75.0,
    );
    assert_eq!(hit, 250.0);
    assert_eq!(crit, 700.0);
}

/// CHANCE_AILMENT merge (vendor `:2498-2533`): `max×s + min×(1−s)`, `s=min(1, stacks/max)`.
#[test]
fn merge_hand_ailment_dps_weights_by_stack_fill() {
    use pobr_core::calc::ailment::merge_hand_ailment_dps;
    // Stacks fully filled (stacks >= max): everything uses the max instance.
    assert_eq!(merge_hand_ailment_dps(100.0, 60.0, 5.0, 1.0), 100.0);
    // Half filled (s=0.5): 100×0.5 + 60×0.5 = 80.
    assert_eq!(merge_hand_ailment_dps(100.0, 60.0, 1.0, 2.0), 80.0);
    // Missing estimate (stacks=0): conservatively s=1 (everything uses the max instance).
    assert_eq!(merge_hand_ailment_dps(100.0, 60.0, 0.0, 2.0), 100.0);
}

// Keyword scoping (vendor dotCfg) + duration MORE leg

/// `AilmentMagnitude MORE kw=Poison` (modeled on Deadly Poison) only amplifies poison,
/// not ignite (vendor dotCfg keywordFlags include KeywordFlag[ailment],
/// CalcOffence.lua:5005 — PoBR's `ailment_scoped_cfg` sets the flag with the same semantics).
#[test]
fn keyword_scoped_magnitude_applies_only_to_matching_ailment() {
    let mut db = ModDb::new();
    db.add_list(vec![
        Modifier::number("AilmentMagnitude", ModType::More, 75.0)
            .with_keyword_flags(KeywordFlags::POISON),
    ]);
    let cfg = CalcConfig::attack();
    let poison = poison_instance(1000.0, &db, &cfg);
    let ignite = ignite_instance(1000.0, &db, &cfg);
    let bare_poison = poison_instance(1000.0, &ModDb::new(), &cfg);
    let bare_ignite = ignite_instance(1000.0, &ModDb::new(), &cfg);
    assert!(
        (poison.magnitude_dps - bare_poison.magnitude_dps * 1.75).abs() < 1e-6,
        "kw=Poison 量级词条应作用于中毒：{} vs {}",
        poison.magnitude_dps,
        bare_poison.magnitude_dps
    );
    assert_eq!(
        ignite.magnitude_dps, bare_ignite.magnitude_dps,
        "kw=Poison 量级词条不得作用于点燃"
    );
}

/// Duration aggregation with a MORE leg (vendor durationMod = calcLib.mod = (1+inc)×more,
/// CalcOffence.lua:5037-5039): Escalating Poison's `PoisonDuration MORE -20`.
#[test]
fn duration_applies_more_leg() {
    let mut db = ModDb::new();
    db.add_list(vec![Modifier::number(
        "PoisonDuration",
        ModType::More,
        -20.0,
    )]);
    let cfg = CalcConfig::attack();
    let with_more = poison_instance(1000.0, &db, &cfg);
    let bare = poison_instance(1000.0, &ModDb::new(), &cfg);
    assert!(
        (with_more.duration_secs - bare.duration_secs * 0.8).abs() < 1e-6,
        "MORE -20% 应缩短持续：{} vs {}",
        with_more.duration_secs,
        bare.duration_secs
    );
}

/// debuffDurationMult (vendor CalcOffence.lua:1833-1835): a negative enemy-side
/// BuffExpireFaster MORE value (Temporal Chains' expire-slower) → multiplier > 1;
/// floored at BuffExpirationSlowCap=0.25 (Data.lua:177, at most 4x); the panel figure
/// is always 1.
#[test]
fn debuff_duration_mult_from_enemy_buff_expire_faster() {
    let cfg = CalcConfig::attack().with_mode_effective(true);
    // No mods → neutral 1.0.
    assert_eq!(debuff_duration_mult(&ModDb::new(), &cfg), 1.0);

    // druid-oracle-comet oracle intermediate value: after Temporal Chains is scaled by
    // the Pinnacle boss's CurseEffectOnSelf, enemy-side BuffExpireFaster MORE -8 →
    // aggregate 0.92 → mult = 1/0.92 ≈ 1.0870 (same formula as vendor).
    let mut enemy = ModDb::new();
    enemy.add_list(vec![Modifier::number(
        "BuffExpireFaster",
        ModType::More,
        -8.0,
    )]);
    let mult = debuff_duration_mult(&enemy, &cfg);
    assert!(
        (mult - 1.0 / 0.92).abs() < 1e-9,
        "MORE -8 → 1/0.92，实得 {mult}"
    );

    // Floor: MORE -90 → aggregate 0.10 < 0.25 → capped to 0.25 → mult = 4.
    let mut capped = ModDb::new();
    capped.add_list(vec![Modifier::number(
        "BuffExpireFaster",
        ModType::More,
        -90.0,
    )]);
    assert_eq!(debuff_duration_mult(&capped, &cfg), 4.0);

    // Panel figure (mode_effective=false) is always 1 (vendor :1834 gate).
    let panel = CalcConfig::attack();
    assert_eq!(debuff_duration_mult(&enemy, &panel), 1.0);
}
