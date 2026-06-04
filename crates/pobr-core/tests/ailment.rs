use pobr_core::calc::ailment::{
    AilmentSource, DamagingAilmentOutput, bleed_instance, bleed_traced, corrupted_blood_instance,
    effmult_for_ailment, flat_chance, ignite_instance, ignite_traced, player_ailment_threshold,
    poison_instance, shock_effect, threshold_derived_chance, weighted_source_damage,
};
use pobr_core::{CalcConfig, ModDb, Modifier, TraceGraph};
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
fn poison_magnitude_scales_with_ailment_damage_modifiers() {
    let gc = GameConstants::poe2();
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("PoisonDamage", ModType::Inc, 100.0));

    let instance = poison_instance(1000.0, &db, &CalcConfig::attack());

    let base = 1000.0 * gc.poison_base_fraction;
    assert_eq!(instance.magnitude_dps, base * 2.0);
    assert_eq!(instance.ailment, AilmentType::Poison);
}

/// PoE2 0.5.0 感电效果范围测试。
///
/// **Bug#9 修正**：感电最小值 20%（非 PoE1 的 5%），最大值 100%（非 PoE1 的 50%）。
/// 出处：agent-docs/ailments.md §感电 `BaseShockMagnitude=20, max=100`；
///       PoB2 `nonDamagingAilmentsConfig.Shock, clamp [20, 100]`。
#[test]
fn shock_effect_is_clamped_between_20_and_100_percent_poe2() {
    // 无击中 → 返回 0（不施加感电）
    assert_eq!(shock_effect(0.0, 1000.0), 0.0);
    // 极大击中 → 感电上限 100%（= 1.0 fraction）
    let huge = shock_effect(1_000_000.0, 100.0);
    assert_eq!(huge, 1.0);
    // 极小击中（相对阈值）→ 感电下限 20%（= 0.20 fraction）
    let tiny = shock_effect(1.0, 1_000_000.0);
    assert_eq!(tiny, 0.20);
    // 满阈值击中（ratio=1）→ 50% 感电（0.5 * 1.0^0.4 = 0.5，fraction 0.50）
    let at_threshold = shock_effect(1000.0, 1000.0);
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

// ---------------------------------------------------------------------------
// Step 2: 施加几率 + effMult + 暴击加权 + 玩家阈值 + trace
// ---------------------------------------------------------------------------

/// 玩家异常阈值 = 最大生命 × 0.5（gap: player-ailment-threshold-bug）。
#[test]
fn player_ailment_threshold_is_half_of_max_life() {
    assert_eq!(player_ailment_threshold(1000.0), 500.0);
    assert_eq!(player_ailment_threshold(2480.0), 1240.0);
    assert_eq!(player_ailment_threshold(0.0), 0.0);
}

/// 内禀几率（流血/中毒）：base × (1+inc/100) × more，clamp 100。几率为 0 时不施加。
#[test]
fn flat_chance_scales_and_clamps() {
    // 25% base，无 inc/more
    assert_eq!(flat_chance(25.0, 0.0, 0.0), 0.0); // more=0 → 0（more 以乘子语义，0 表示无 more 应传 1）
    // 正确口径：more 为 1.0 表示无 more
    assert_eq!(flat_chance(25.0, 0.0, 1.0), 25.0);
    // +100% inc → 50%
    assert_eq!(flat_chance(25.0, 100.0, 1.0), 50.0);
    // 超 100 → clamp 100
    assert_eq!(flat_chance(80.0, 100.0, 1.0), 100.0);
    // base=0 → 0（不施加）
    assert_eq!(flat_chance(0.0, 500.0, 2.0), 0.0);
}

/// 几率派生（点燃/感电）随 hit/threshold 单调上升：更高伤害或更低阈值 → 更高几率。
#[test]
fn threshold_derived_chance_increases_with_hit_and_decreases_with_threshold() {
    let mult = 20.0; // IgniteChanceMultiplier
    // 满阈值伤害（hit=threshold=1000）：hit/thr*mult = 20% on hit
    let (on_hit, _) = threshold_derived_chance(1000.0, 1000.0, 1000.0, mult, 0.0, 0.0, 1.0);
    assert!((on_hit - 20.0).abs() < 1e-6);

    // 双倍伤害 → 双倍几率（线性段，未 clamp）
    let (on_hit2, _) = threshold_derived_chance(2000.0, 2000.0, 1000.0, mult, 0.0, 0.0, 1.0);
    assert!(on_hit2 > on_hit);
    assert!((on_hit2 - 40.0).abs() < 1e-6);

    // 更高阈值 → 更低几率
    let (on_hit_high_thr, _) =
        threshold_derived_chance(1000.0, 1000.0, 2000.0, mult, 0.0, 0.0, 1.0);
    assert!(on_hit_high_thr < on_hit);

    // 巨额伤害 → clamp 100
    let (capped, _) =
        threshold_derived_chance(1_000_000.0, 1_000_000.0, 1000.0, mult, 0.0, 0.0, 1.0);
    assert_eq!(capped, 100.0);
}

/// 暴击来源伤害比非暴击高（crit_avg = hit_avg × crit_mult），加权后 base > 纯非暴击。
#[test]
fn crit_weighting_raises_source_damage() {
    // 50% 暴击，2x 爆伤
    let source = AilmentSource::new(1000.0, 2.0, 0.5, false);
    assert_eq!(source.hit_avg, 1000.0);
    assert_eq!(source.crit_avg, 2000.0);

    // 100% 命中几率，100% 暴击几率：base 应趋向暴击伤害
    let (_chance, base_high_crit) = weighted_source_damage(&source, 100.0, 100.0);
    // 加权：hit*(1-0.5)*chanceOnHit + crit*0.5*chanceOnCrit，归一后 = 1500（中点）
    assert!(
        base_high_crit > 1000.0,
        "crit weighting should exceed non-crit hit"
    );

    // AilmentsAreNeverFromCrit：暴击来源退化为非暴击，base = 非暴击伤害
    let no_crit = AilmentSource::new(1000.0, 2.0, 0.5, true);
    assert_eq!(no_crit.crit_avg, 1000.0);
    assert_eq!(no_crit.crit_chance, 0.0);
    let (_c, base_no_crit) = weighted_source_damage(&no_crit, 100.0, 100.0);
    assert_eq!(base_no_crit, 1000.0);
}

/// effMult：敌方火抗 40% → 点燃 DPS effMult = 0.6（gap: ailment-effmult-missing）。
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

    // mode_effective=false → effMult 1.0（面板裸口径）
    let bare = effmult_for_ailment(&enemy, &cfg, DamageType::Fire, false);
    assert_eq!(bare, 1.0);
}

/// effMult：敌方 DamageTakenOverTime +50% → effMult 提升 1.5×。
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

/// 物理异常（流血）无视抗性减伤：敌方物抗不影响 effMult（仅吃 taken 链）。
#[test]
fn physical_ailment_ignores_resistance_in_effmult() {
    let cfg = CalcConfig::attack().with_mode_effective(true);
    let mut enemy = ModDb::new();
    // 物理"抗性"对异常无意义；只 taken 链起作用
    enemy.add_mod(Modifier::number("PhysicalDamageTaken", ModType::Inc, 20.0));

    let eff = effmult_for_ailment(&enemy, &cfg, DamageType::Physical, true);
    assert!((eff - 1.2).abs() < 1e-6);
}

/// 流血面板：100% 几率 + effMult，可由 TraceGraph 回溯（gap: ailment-trace-attribution-missing）。
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

    // 100% 几率
    assert_eq!(out.chance, 1.0);
    // effMult 1.5（+50% PhysicalDamageTaken）
    assert!((out.eff_mult - 1.5).abs() < 1e-6);
    // expected_dps = magnitude × chance；magnitude 含 effMult
    let gc = GameConstants::poe2();
    let expected_mag = (1000.0 * gc.bleed_base_fraction) * 1.5;
    assert!((out.magnitude_dps - expected_mag).abs() < 1e-3);
    assert!((out.expected_dps - expected_mag).abs() < 1e-3); // chance=1.0

    // trace 中应有节点，且输出节点存在
    assert!(!trace.nodes().is_empty());
    assert!(trace.node(node).is_some());
    // BleedChance BASE 贡献应作为 source node 进入图
    let has_chance_source = trace
        .nodes()
        .iter()
        .any(|n| n.label.contains("BleedChance") || n.label.contains("Bleed chance"));
    assert!(
        has_chance_source,
        "trace should contain bleed chance contribution"
    );

    // DPS 输出节点应有 incoming 边（chance + magnitude + effMult 链入），即可回溯。
    let incoming = trace.incoming(node);
    assert!(
        incoming.len() >= 3,
        "DPS node should aggregate chance + magnitude + effMult (got {} edges)",
        incoming.len()
    );
    // effMult 节点（带敌方 PhysicalDamageTaken 贡献）应存在于图中。
    let has_effmult = trace
        .nodes()
        .iter()
        .any(|n| n.label.contains("EffMult") || n.label.contains("DamageTaken"));
    assert!(
        has_effmult,
        "trace should contain effMult/DamageTaken nodes"
    );
}

/// 点燃几率派生：高火伤/低阈值 → 高几率 → 高期望 DPS（gap: no-ailment-chance-pipeline）。
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

    // 更低阈值 → 更高几率 → 更高期望 DPS
    assert!(low_thr.chance > high_thr.chance);
    assert!(low_thr.expected_dps > high_thr.expected_dps);
}
