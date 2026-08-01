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
    // PoE2：伤害异常 magnitude 仅由 `AilmentMagnitude` 缩放（PoB2 ailmentPercentBase 因子）。
    // PoE1 的 PoisonDamage/DamageOverTime 在 PoE2 不存在、不再生效。
    let gc = GameConstants::poe2();
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("AilmentMagnitude", ModType::Inc, 100.0));

    let instance = poison_instance(1000.0, &db, &CalcConfig::attack());

    let base = 1000.0 * gc.poison_base_fraction;
    assert_eq!(instance.magnitude_dps, base * 2.0);
    assert_eq!(instance.ailment, AilmentType::Poison);

    // PoE1 幻影名不再缩放 PoE2 magnitude。
    let mut db_phantom = ModDb::new();
    db_phantom.add_mod(Modifier::number("PoisonDamage", ModType::Inc, 100.0));
    let phantom = poison_instance(1000.0, &db_phantom, &CalcConfig::attack());
    assert_eq!(phantom.magnitude_dps, base, "PoisonDamage 幻影名不生效");
}

/// PoE2 0.5.0 感电效果范围测试。
///
/// **Bug#9 修正**：感电最小值 20%（非 PoE1 的 5%），最大值 100%（非 PoE1 的 50%）。
/// 出处：agent-docs/ailments.md §感电 `BaseShockMagnitude=20, max=100`；
///       PoB2 `nonDamagingAilmentsConfig.Shock, clamp [20, 100]`。
#[test]
fn shock_effect_is_clamped_between_20_and_100_percent_poe2() {
    // 无击中 → 返回 0（不施加感电）
    assert_eq!(shock_effect(0.0, 1000.0, SHOCK_MIN_EFFECT), 0.0);
    // 极大击中 → 感电上限 100%（= 1.0 fraction）
    let huge = shock_effect(1_000_000.0, 100.0, SHOCK_MIN_EFFECT);
    assert_eq!(huge, 1.0);
    // 极小击中（相对阈值）→ 感电下限 20%（= 0.20 fraction）
    let tiny = shock_effect(1.0, 1_000_000.0, SHOCK_MIN_EFFECT);
    assert_eq!(tiny, 0.20);
    // 满阈值击中（ratio=1）→ 50% 感电（0.5 * 1.0^0.4 = 0.5，fraction 0.50）
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

// Step 2: 施加几率 + effMult + 暴击加权 + 玩家阈值 + trace

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

// Step 3 (Lane B): 冰缓 effect / 冰冻+电击 Poise buildup / 叠层权重平均

// 冰缓 effect (chill-effect-missing)

/// 冰缓最小阈值：< 30% 强度时返回 0（丢弃），PoE2 0.5.0。
///
/// 出处：agent-docs/ailments.md §冰缓、PoB2 `nonDamagingAilmentsConfig.Chill` clamp [30,50]、
///   `chillMinimumThreshold = enemyThreshold / ChillEffectMultiplier`（< 30% 丢弃）。
#[test]
fn chill_effect_below_min_is_discarded() {
    // 命中 = 0 → 不施加
    assert_eq!(chill_effect(0.0, 1000.0), 0.0);
    // threshold = 0 → 不施加
    assert_eq!(chill_effect(500.0, 0.0), 0.0);
    // 命中 < 30% 阈值 → 强度 < 30 → 丢弃
    // ratio = 100/1000 = 0.1 → raw = 100 * 0.1 = 10 < 30 → 0
    assert_eq!(chill_effect(100.0, 1000.0), 0.0);
    // ratio = 0.29 → raw = 29 < 30 → 0
    assert_eq!(chill_effect(290.0, 1000.0), 0.0);
}

/// 冰缓下限：30% 命中/阈值 → 效果恰好 30%（最小施加阈值）。
///
/// 出处：agent-docs/ailments.md §冰缓 `min=30`。
#[test]
fn chill_effect_at_minimum_threshold() {
    // ratio = 300/1000 = 0.3 → raw = 100 * 0.3 = 30.0 → clamp [30,50] → 30
    let effect = chill_effect(300.0, 1000.0);
    assert!(
        (effect - 30.0).abs() < 1e-6,
        "30% threshold hit → chill 30%, got {effect}"
    );
}

/// 冰缓上限：超过 50% 阈值伤害 → clamp 到 50%。
///
/// 出处：agent-docs/ailments.md §冰缓 `max=50 (ChillMaxEffect)`、
///   PoB2 `data.gameConstants["ChillMaxEffect"] = 50`。
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

/// 冰缓线性缩放：伤害加倍 → 效果加倍（在 [30,50] 区间内）。
///
/// 出处：PoB2 `chillEffect = 100 * (damage/threshold)` 线性公式（非幂律）。
#[test]
fn chill_effect_linear_scaling() {
    // ratio 0.35 → raw 35 → 35.0（在 [30,50] 区间内线性）
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
    // 线性：e45 / e35 ≈ 45/35 ≈ 1.286
    assert!(e45 > e35, "larger hit → larger chill");
}

/// 冰缓含 effectMod：+100% AilmentMagnitude → 效果加倍（若不超 cap）。
///
/// 出处：agent-docs/ailments.md §`effectMod`。
#[test]
fn chill_effect_with_mods_scales_with_effect_mod() {
    // base ratio = 0.30 → raw = 30，effectMod = 2.0 → raw = 60 → clamp 50
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

/// 冰缓 traced：归因节点正确写入 TraceGraph，效果值与非 traced 版一致。
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

/// 冰缓 traced 含 AilmentMagnitude mod：effectMod 通过 ModDb 聚合，效果值正确放大。
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
    // trace 中应有 AilmentMagnitude inc 节点
    let has_mag = trace
        .nodes()
        .iter()
        .any(|n| n.label.contains("Chill magnitude"));
    assert!(
        has_mag,
        "trace should record chill magnitude mod contribution"
    );
}

// 冰冻/电击 Poise 积累 (freeze-electrocute-buildup-missing)

/// 冰冻 Poise 积累随姿态阈值单调递减：阈值越低→每次击中积累%越高。
///
/// 出处：agent-docs/ailments.md §冰冻积累：
///   `poiseBuildup = FREEZE_DAMAGE_SCALE / enemyPoiseThreshold * inc_more * 100`
///   `FREEZE_DAMAGE_SCALE = 2.1`。
#[test]
fn freeze_poise_buildup_decreases_with_poise_threshold() {
    // threshold = 0 → 0（safety）
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

    // 单调递减验证
    assert!(
        low_thr > high_thr,
        "lower threshold → higher buildup per hit"
    );
}

/// 冰冻 Poise 积累随 inc/more 线性放大。
///
/// 出处：PoB2 `poiseBuildup = ... * (1 + inc/100) * more * 100`。
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

/// 电击 Poise 积累基础值（ELECTROCUTE_DAMAGE_SCALE = 1.7）。
///
/// 出处：PoB2 `data.gameConstants["ElectrocuteDamageScale"] = 1.7`。
#[test]
fn electrocute_poise_buildup_uses_correct_scale() {
    // threshold = 170 → buildup = 1.7/170 * 100 = 1.0%
    let buildup = electrocute_poise_buildup(170.0, 0.0, 1.0);
    assert!(
        (buildup - 1.0).abs() < 1e-6,
        "electrocute poise=170 → 1%, got {buildup}"
    );

    // 电击 vs 冰冻的 scale 比 = 1.7/2.1（同等阈值下，电击积累率更低）
    let freeze_b = freeze_poise_buildup(1000.0, 0.0, 1.0);
    let elec_b = electrocute_poise_buildup(1000.0, 0.0, 1.0);
    let ratio = elec_b / freeze_b;
    assert!(
        (ratio - 1.7 / 2.1).abs() < 1e-6,
        "electrocute/freeze scale ratio should be 1.7/2.1, got {ratio}"
    );
}

/// 冰冻 Poise 积累 traced：节点写入 TraceGraph，积累值与 non-traced 版一致。
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

/// 电击 Poise 积累 traced 含 mod：`EnemyElectrocuteBuildup` inc 正确缩放积累。
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

// 叠层权重平均 DPS (ailment-stacking)

/// 默认单层（StackConfig::single()）：DPS = single_layer_dps × 1。
///
/// 出处：agent-docs/ailments.md §叠层 `ailmentDPS = baseVal * activeAilments * ...`。
#[test]
fn stacking_ailment_dps_single_layer() {
    let cfg = StackConfig::single();
    let dps = stacking_ailment_dps(100.0, &cfg);
    assert!(
        (dps - 100.0).abs() < 1e-6,
        "single layer → DPS unchanged, got {dps}"
    );
}

/// 叠层 DPS 随 active_stacks 线性增长（替换 Wave1d 单层期望值简化）。
///
/// 出处：agent-docs/ailments.md §叠层 `activeAilments` 乘子。
#[test]
fn stacking_ailment_dps_scales_with_active_stacks() {
    let cfg = StackConfig::new(5, 3.0);
    let dps = stacking_ailment_dps(100.0, &cfg);
    // 3 活跃层 × 100 DPS/层 = 300
    assert!(
        (dps - 300.0).abs() < 1e-6,
        "3 active stacks × 100 = 300, got {dps}"
    );

    // active_stacks = 0 时退化到 max_stacks
    let cfg_no_active = StackConfig::new(4, 0.0);
    let dps_max = stacking_ailment_dps(100.0, &cfg_no_active);
    assert!(
        (dps_max - 400.0).abs() < 1e-6,
        "active=0 → use max_stacks=4 → 400, got {dps_max}"
    );
}

/// StackPotential = active/max，clamp [0,1]。
///
/// 出处：PoB2 `StackPotential = ailmentStacks / maxStacks`。
#[test]
fn stack_potential_is_ratio_of_active_to_max() {
    let cfg = StackConfig::new(10, 5.0);
    let sp = stack_potential(&cfg);
    assert!((sp - 0.5).abs() < 1e-6, "5/10 → potential 0.5, got {sp}");

    // 溢出：active > max → SP = 8/5 = 1.6（PoB2 不 clamp，可 >1；触发 over-stacking 放大）
    let cfg_over = StackConfig::new(5, 8.0);
    assert!(
        (stack_potential(&cfg_over) - 1.6).abs() < 1e-6,
        "overflow → potential 8/5 = 1.6 (PoB2 不 clamp)"
    );

    // 默认单层
    let sp_single = stack_potential(&StackConfig::single());
    assert_eq!(sp_single, 1.0, "single stack → potential 1.0");
}

/// RollAverage：未溢出时固定 50%；溢出时偏向高端。
///
/// 出处：PoB2 `CalcOffence.lua` RollAverage 段。
#[test]
fn roll_average_at_midpoint_when_not_overflow() {
    // active = max → 刚好不溢出 → 50
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

/// 05-01 活跃叠层估算：`stacks = hitChance × applyChance × duration × speed`。
#[test]
fn estimate_active_stacks_is_product_of_signals() {
    // 1.0 命中 × 1.0 施加 × 5s 持续 × 4/s 速率 = 20 层。
    let s = estimate_active_stacks(1.0, 1.0, 5.0, 4.0);
    assert!((s - 20.0).abs() < 1e-6, "1×1×5×4 = 20, got {s}");

    // 部分命中/施加按比例缩放：0.9 × 0.5 × 4 × 2 = 3.6。
    let s2 = estimate_active_stacks(0.9, 0.5, 4.0, 2.0);
    assert!((s2 - 3.6).abs() < 1e-6, "0.9×0.5×4×2 = 3.6, got {s2}");
}

/// 任一信号缺失（无速率 / 无持续 / 无命中 / 无施加几率）时返回 0（回退到 max_stacks 上界）。
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

/// 05-04 RollAverage 高位偏移：`cross_type_source_hit_at_roll` 在 [min,max] 上内插。
/// roll=50 退化为区间中点（= `cross_type_source_hit`）；roll>50 向高端偏移。
#[test]
fn cross_type_source_hit_shifts_with_roll() {
    use pobr_core::calc::DamageComponent;
    let player = ModDb::new();
    let cfg = CalcConfig::attack();
    // 物理分量 [600, 1400]，跨度 800。
    let components = vec![DamageComponent::new(DamageType::Physical, 600.0, 1400.0)];

    // roll=50 → 中点 1000（与 cross_type_source_hit 一致）。
    let mid = cross_type_source_hit_at_roll(AilmentType::Bleed, &components, &player, &cfg, 50.0);
    let legacy = cross_type_source_hit(AilmentType::Bleed, &components, &player, &cfg);
    assert!((mid - 1000.0).abs() < 1e-6, "roll 50 → 1000, got {mid}");
    assert!((mid - legacy).abs() < 1e-6, "roll 50 == legacy avg");

    // roll=75 → 600 + 800×0.75 = 1200（向高端偏移）。
    let high = cross_type_source_hit_at_roll(AilmentType::Bleed, &components, &player, &cfg, 75.0);
    assert!((high - 1200.0).abs() < 1e-6, "roll 75 → 1200, got {high}");
    assert!(high > mid, "high roll shifts source hit up");
}

/// 叠层 DPS traced：节点写入 TraceGraph，DPS 与 non-traced 一致。
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
    // ActiveStacks 节点应存在并连入 StackedDPS
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

/// 流血叠层 DPS 集成测试：从 bleed_instance + stacking 全链路验证（bleed/poison 叠层）。
///
/// 出处：agent-docs/ailments.md §叠层与权重平均（流血默认 5 层，3 活跃）。
#[test]
fn bleed_stacking_dps_integration() {
    let gc = GameConstants::poe2();
    // 1000 物理命中 → 单层 150 DPS（bleed_base_fraction=0.15）
    let instance = bleed_instance(1000.0, &ModDb::new(), &CalcConfig::attack());
    assert!((instance.magnitude_dps - 1000.0 * gc.bleed_base_fraction).abs() < 1e-6);

    // 3 活跃层：DPS = 150 × 3 = 450
    let stack_cfg = StackConfig::new(5, 3.0);
    let stacked = stacking_ailment_dps(instance.magnitude_dps, &stack_cfg);
    assert!(
        (stacked - 450.0).abs() < 1e-6,
        "3-stack bleed DPS = 450, got {stacked}"
    );

    // 中毒叠层：4 活跃层 × 200 DPS（poison_base_fraction=0.20, hit=1000）
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

// Feature 1: AilmentEffect / Faster / Slower 三维度

/// `AilmentEffect` MORE 乘区：无 mod 时为 1.0（中性）。
///
/// 出处：PoB2 `CalcOffence.lua` l.5190
///   `local effectMod = calcLib.mod(skillModList, dotCfg, "AilmentEffect")`（MORE 聚合）。
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

/// `AilmentEffect` MORE 聚合：两个 more 词条连乘。
///
/// 出处：PoB2 `calcLib.mod` = MORE 连乘语义。
#[test]
fn ailment_effect_mod_is_product_of_more_mods() {
    let cfg = CalcConfig::attack();
    let mut db = ModDb::new();
    // 两个独立 AilmentEffect More：1.5 × 1.2 = 1.8（但 ModDb 用 Inc/More 口径：
    // More mods 在 ModDb.more() 以"1 + value/100"连乘，0.5 = +50% more 词条）。
    // 直接用 More type（value 为附加比例），0.5 → factor 1.5，0.2 → factor 1.2。
    db.add_mod(Modifier::number("AilmentEffect", ModType::More, 50.0));
    db.add_mod(Modifier::number("AilmentEffect", ModType::More, 20.0));
    let eff = ailment_effect_mod(&db, &cfg);
    let expected = 1.5 * 1.2;
    assert!(
        (eff - expected).abs() < 1e-9,
        "50% more × 20% more → {expected}, got {eff}"
    );
}

/// `ailment_rate_mod`：无 Faster/Slower mod 时 → `faster / slower = 1.0/1.0 = 1.0`。
///
/// 出处：PoB2 `CalcOffence.lua` l.5035
///   `rateMod = calcLib.mod(skillModList, cfg, ailment.."Faster") / calcLib.mod(..., ailment.."Slower")`。
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

/// `ailment_rate_mod` + Faster：`BleedFaster More 50% → faster=1.5, rateMod=1.5`。
///
/// 出处：PoB2 rateMod = mod(BleedFaster)（MORE） / mod(BleedSlower)（MORE = 1.0 默认）。
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

/// （k3）：`ailment_rate_mod` 的 INC 腿——vendor `calcLib.mod`（CalcTools.lua:16-18）
/// = `(1 + ΣINC/100) × ΠMORE`；statmap `faster_burn_%` 族产 INC（SkillStatMap.lua:843-848）。
///
/// `IgniteFaster INC 50 + MORE 20 → faster = 1.5 × 1.2 = 1.8`。
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

    // Slower 的 INC 腿同形：BleedSlower INC 100 → slower=2.0 → rateMod=0.5。
    let mut slower_player = ModDb::new();
    slower_player.add_mod(Modifier::number("BleedSlower", ModType::Inc, 100.0));
    let rm = ailment_rate_mod(&slower_player, &enemy, &cfg, "Bleed");
    assert!(
        (rm - 0.5).abs() < 1e-9,
        "BleedSlower INC100 → rateMod=0.5, got {rm}"
    );
}

/// `ailment_rate_mod` + Slower：`BleedSlower More 25% → slower=1.25, rateMod = 1.0/1.25`。
///
/// 出处：PoB2 `rateMod = faster / slower`；Slower 让 rateMod < 1。
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

/// `apply_rate_mod_to_instance`：DPS × rateMod，duration / rateMod（总伤害不变）。
///
/// 出处：PoB2 `ailmentDPS *= rateMod`；`duration /= rateMod`；总伤害守恒验证。
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
    // 总伤害守恒：DPS × duration
    let total_before = inst.magnitude_dps * inst.duration_secs;
    let total_after = modified.magnitude_dps * modified.duration_secs;
    assert!(
        (total_after - total_before).abs() < 1e-3,
        "total damage should be conserved: before={total_before} after={total_after}"
    );
}

/// `apply_effect_mod_to_instance`：magnitude_dps × effectMod，duration 不变。
///
/// 出处：PoB2 `ailmentDPS = baseVal * effectMod * ...`；effectMod 不改时长。
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
    // duration 不变
    assert!(
        (modified.duration_secs - inst.duration_secs).abs() < 1e-9,
        "duration should not change with effectMod"
    );
}

/// `apply_effect_and_rate_mod` 组合：DPS × effectMod × rateMod，duration / rateMod。
///
/// 出处：PoB2 `ailmentDPS = baseVal * effectMod * rateMod * activeAilments * effMult`。
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
    // duration = base / rateMod（effectMod 不改时长）
    let expected_dur = base_dur / rate_mod;
    assert!(
        (modified.duration_secs - expected_dur).abs() < 1e-3,
        "duration / rateMod: expected {expected_dur:.3}, got {:.3}",
        modified.duration_secs
    );
}

/// `apply_effect_and_rate_mod_traced` 写入 TraceGraph 节点。
///
/// 出处：归因要求 effectMod / rateMod 各有独立节点连入 magnitude 节点。
#[test]
fn apply_effect_and_rate_mod_traced_writes_nodes() {
    let mut trace = TraceGraph::new();
    // 添加一个虚拟 magnitude 节点作为目标
    let mag_node = trace.add_node("BleedMagnitude", 100.0, TraceOperation::Multiply);

    let inst = bleed_instance(1000.0, &ModDb::new(), &CalcConfig::attack());
    let modified = apply_effect_and_rate_mod_traced(inst, 1.5, 2.0, "Bleed", mag_node, &mut trace);

    // DPS 已修正
    let expected_dps = inst.magnitude_dps * 1.5 * 2.0;
    assert!(
        (modified.magnitude_dps - expected_dps).abs() < 1e-2,
        "traced: DPS should be {expected_dps:.2}"
    );
    // trace 中应有 EffectMod 和 RateMod 节点
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
    // 两个节点都应链入 mag_node
    let incoming = trace.incoming(mag_node);
    assert!(
        incoming.len() >= 2,
        "mag_node should have ≥2 incoming edges (effectMod + rateMod)"
    );
}

// Feature 2: 跨类型施加 (<Type>Can<Ailment>)

/// 默认：火命中不施加流血，物理命中才算流血来源。
///
/// 出处：agent-docs/ailments.md §元素/非元素归类；Bleed 默认 ScalesFrom=Physical。
#[test]
fn cross_type_source_hit_defaults_to_physical_for_bleed() {
    use pobr_core::calc::DamageComponent;
    let cfg = CalcConfig::attack();
    let player = ModDb::new(); // 无 FireCanBleed flag

    let components = vec![
        DamageComponent::new(DamageType::Physical, 800.0, 1200.0), // avg=1000
        DamageComponent::new(DamageType::Fire, 400.0, 600.0),      // avg=500，不计入
    ];

    let hit = cross_type_source_hit(AilmentType::Bleed, &components, &player, &cfg);
    assert!(
        (hit - 1000.0).abs() < 1e-6,
        "Bleed default source: Physical avg=1000, got {hit}"
    );
}

/// `FireCanBleed` 旗标：火伤也计入流血来源命中。
///
/// 出处：agent-docs/ailments.md §改写施加规则的例外（Blood Barbs 等 FireCanBleed）、
///   PoB2 `canDoAilment` l.4806 `skillModList:Flag(cfg, type.."Can"..damagingAilment)`。
#[test]
fn cross_type_source_hit_fire_can_bleed_adds_fire_damage() {
    use pobr_core::calc::DamageComponent;
    let cfg = CalcConfig::attack();
    let mut player = ModDb::new();
    player.add_mod(Modifier::flag("FireCanBleed"));

    let components = vec![
        DamageComponent::new(DamageType::Physical, 800.0, 1200.0), // avg=1000
        DamageComponent::new(DamageType::Fire, 400.0, 600.0),      // avg=500，现在计入
    ];

    let hit = cross_type_source_hit(AilmentType::Bleed, &components, &player, &cfg);
    assert!(
        (hit - 1500.0).abs() < 1e-6,
        "FireCanBleed: Physical(1000)+Fire(500)=1500, got {hit}"
    );
}

/// `ChaosCanShock` 旗标：混沌伤也计入感电来源。
///
/// 出处：agent-docs/ailments.md §例外（Voltaxic Rift：ChaosCanShock）、
///   PoB2 l.4806 `type.."Can"..damagingAilment`。
#[test]
fn cross_type_source_hit_chaos_can_shock() {
    use pobr_core::calc::DamageComponent;
    let cfg = CalcConfig::attack();
    let mut player = ModDb::new();
    player.add_mod(Modifier::flag("ChaosCanShock"));

    let components = vec![
        DamageComponent::new(DamageType::Lightning, 500.0, 700.0), // avg=600，默认感电来���
        DamageComponent::new(DamageType::Chaos, 200.0, 400.0),     // avg=300，now ChaosCanShock
    ];

    let hit = cross_type_source_hit(AilmentType::Shock, &components, &player, &cfg);
    assert!(
        (hit - 900.0).abs() < 1e-6,
        "ChaosCanShock: Lightning(600)+Chaos(300)=900, got {hit}"
    );
}

/// 无命中分量时返回 0。
#[test]
fn cross_type_source_hit_empty_components() {
    let cfg = CalcConfig::attack();
    let player = ModDb::new();
    let components = [];
    let hit = cross_type_source_hit(AilmentType::Bleed, &components, &player, &cfg);
    assert_eq!(hit, 0.0, "empty components → 0");
}

// Feature 3: DotDpsCap

/// `apply_dot_dps_cap`：普通 DPS 低于 cap 时原样返回。
///
/// 出处：PoB2 `ailmentDPSCapped = m_min(ailmentDPSUncapped, data.misc.DotDpsCap)`。
#[test]
fn apply_dot_dps_cap_passthrough_below_cap() {
    let dps = 1_000_000.0;
    let capped = apply_dot_dps_cap(dps, DOT_DPS_CAP);
    assert!(
        (capped - dps).abs() < 1.0,
        "1M DPS < cap → unchanged, got {capped}"
    );
}

/// `apply_dot_dps_cap`：超出上限时截断为 DOT_DPS_CAP（35,791,394）。
///
/// 出处：PoB2 `Data.lua` `DotDpsCap = 35791394`。
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

/// 05-07 硬化：`apply_dot_dps_cap` 始终用常量 `DOT_DPS_CAP`，**忽略 modDB 中的
/// `DotDpsCap`**（PoB2 中该 cap 是 `Data.lua` 硬编码常量，无 Override/modDB 机制）。
///
/// 出处：PoB2 全 src `m_min(_, data.misc.DotDpsCap)`，grep 无 `Override(..,"DotDpsCap")`。
#[test]
fn apply_dot_dps_cap_ignores_moddb_dotdpscap() {
    // modDB 中即便写入低值 DotDpsCap，也不影响 cap（PoB2-faithful：始终用常量）。
    let dps = 50_000.0; // 远低于 DOT_DPS_CAP
    let capped = apply_dot_dps_cap(dps, DOT_DPS_CAP);
    assert!(
        (capped - dps).abs() < 1.0,
        "50k < DOT_DPS_CAP → 原样返回（modDB DotDpsCap 不生效），got {capped}"
    );
}

/// `dps_with_effect_rate_cap`：effect + rate 同时作用后被 cap 截断。
///
/// 出处：PoB2 `ailmentDPS = m_min(baseVal * effectMod * rateMod * ..., DotDpsCap)`。
#[test]
fn dps_with_effect_rate_cap_applies_cap() {
    use pobr_data::constants::DOT_DPS_CAP;

    // 设置一个超大 base_dps，确保 effectMod × rateMod 后超过 cap
    let base_dps = DOT_DPS_CAP * 0.6; // 60% of cap
    let effect_mod = 2.0; // × 2.0 → 120% of cap → 超 cap
    let rate_mod = 1.0;

    let result = dps_with_effect_rate_cap(base_dps, effect_mod, rate_mod, DOT_DPS_CAP);
    assert!(
        (result - DOT_DPS_CAP).abs() < 1.0,
        "base × 2.0 exceeds cap → clamped to DOT_DPS_CAP, got {result}"
    );
}

/// `dps_with_effect_rate_cap_traced`：DPS 被截断时，trace 中应有 DotDpsCap 节点。
///
/// 出处：DotDpsCap 截断信息应归因到 TraceGraph（增量归因价值）。
#[test]
fn dps_with_effect_rate_cap_traced_adds_cap_node_when_truncated() {
    use pobr_data::constants::DOT_DPS_CAP;
    let mut trace = TraceGraph::new();

    // 超 cap 的情况
    let base_dps = DOT_DPS_CAP * 0.7;
    let (result, node) =
        dps_with_effect_rate_cap_traced(base_dps, 2.0, 1.0, DOT_DPS_CAP, "Ignite", &mut trace);

    // 结果截断到 cap
    assert!(
        (result - DOT_DPS_CAP).abs() < 1.0,
        "capped result should be DOT_DPS_CAP, got {result}"
    );
    // trace 中应存��� DotDpsCap 节点
    let has_cap = trace.nodes().iter().any(|n| n.label.contains("DotDpsCap"));
    assert!(has_cap, "trace should have DotDpsCap node when truncated");
    // 输出节点存在
    assert!(trace.node(node).is_some(), "output node should exist");
}

/// `dps_with_effect_rate_cap_traced`：DPS 未被截断时，trace 中**不应**有 DotDpsCap 节点。
#[test]
fn dps_with_effect_rate_cap_traced_no_cap_node_when_not_truncated() {
    let mut trace = TraceGraph::new();

    // 很小的 base_dps，不会超 cap
    let (_, _) = dps_with_effect_rate_cap_traced(100.0, 1.0, 1.0, DOT_DPS_CAP, "Bleed", &mut trace);

    let has_cap = trace.nodes().iter().any(|n| n.label.contains("DotDpsCap"));
    assert!(!has_cap, "no cap node when DPS is below DOT_DPS_CAP");
}

// 05-01：异常暴击 over-stacking 修正（PoB2 CalcOffence.lua L5144
// ailmentCritChance = 100*(1-(1-c)^max(SP,1))）

#[test]
fn ailment_crit_chance_applies_over_stacking_correction() {
    let crit = 0.5_f64; // 50% 单次命中暴击率（fraction）

    // SP <= 1：退化为裸暴击率（指数被 max(.,1) 抬到 1）。
    assert!((ailment_crit_chance(crit, 1.0) - 0.5).abs() < 1e-9);
    assert!((ailment_crit_chance(crit, 0.4) - 0.5).abs() < 1e-9);

    // SP > 1（叠层溢出）：放大。SP=2 → 1 - 0.5^2 = 0.75。
    let amplified = ailment_crit_chance(crit, 2.0);
    assert!((amplified - 0.75).abs() < 1e-9);
    assert!(amplified > crit, "over-stacking 应抬高暴击份额");

    // 边界：0 暴击恒 0；满暴击恒 1。
    assert!((ailment_crit_chance(0.0, 3.0) - 0.0).abs() < 1e-9);
    assert!((ailment_crit_chance(1.0, 3.0) - 1.0).abs() < 1e-9);

    // 端到端：放大后的暴击份额喂入 weighted_source_damage，base 高于 SP=1。
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

//  Stored 族来源（stored_source_at_roll）+ CHANCE_AILMENT 合并

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

/// 默认类型门控 + 区间中点：点燃只吃火分量；roll=50 → 两腿各取 (min+max)/2。
/// 暴击腿独立取 Stored crit 区间（非 hit × CritMultiplier 近似）。
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

/// 跨类型施加：`ColdCanIgnite` 旗标使冰分量计入点燃来源（vendor canDoAilment 覆写）。
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

/// 逐类型 Buildup MORE（vendor `:4844`）：`PhysicalBleedBuildup` 只放大物理分量。
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

/// RollAverage 内插（vendor `:5125`）：roll=75 → min + (max−min)×0.75。
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

/// CHANCE_AILMENT 合并（vendor `:2498-2533`）：`max×s + min×(1−s)`，`s=min(1, stacks/max)`。
#[test]
fn merge_hand_ailment_dps_weights_by_stack_fill() {
    use pobr_core::calc::ailment::merge_hand_ailment_dps;
    // 叠层占满（stacks >= max）：全部按最大实例。
    assert_eq!(merge_hand_ailment_dps(100.0, 60.0, 5.0, 1.0), 100.0);
    // 半满（s=0.5）：100×0.5 + 60×0.5 = 80。
    assert_eq!(merge_hand_ailment_dps(100.0, 60.0, 1.0, 2.0), 80.0);
    // 估算缺失（stacks=0）：保守 s=1（全按最大实例）。
    assert_eq!(merge_hand_ailment_dps(100.0, 60.0, 0.0, 2.0), 100.0);
}

//  keyword 作用域（vendor dotCfg）+ duration MORE 腿

/// `AilmentMagnitude MORE kw=Poison`（Deadly Poison 实形）只放大中毒，
/// 不进点燃（vendor dotCfg keywordFlags 含 KeywordFlag[ailment]，
/// CalcOffence.lua:5005——PoBR `ailment_scoped_cfg` 同口径置位）。
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

/// duration 聚合带 MORE 腿（vendor durationMod = calcLib.mod = (1+inc)×more，
/// CalcOffence.lua:5037-5039）：Escalating Poison `PoisonDuration MORE -20`。
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

/// debuffDurationMult（vendor CalcOffence.lua:1833-1835）：敌侧 BuffExpireFaster
/// MORE 负值（Temporal Chains expire-slower）→ 乘区 > 1；下限
/// BuffExpirationSlowCap=0.25（Data.lua:177，至多 4 倍）；面板口径恒 1。
#[test]
fn debuff_duration_mult_from_enemy_buff_expire_faster() {
    let cfg = CalcConfig::attack().with_mode_effective(true);
    // 无词条 → 中性 1.0。
    assert_eq!(debuff_duration_mult(&ModDb::new(), &cfg), 1.0);

    // druid-oracle-comet oracle 中间值：Temporal Chains 经 Pinnacle boss
    // CurseEffectOnSelf 缩放后敌侧 BuffExpireFaster MORE -8 →
    // 聚合 0.92 → mult = 1/0.92 ≈ 1.0870（vendor 同式）。
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

    // 下限：MORE -90 → 聚合 0.10 < 0.25 → cap 到 0.25 → mult = 4。
    let mut capped = ModDb::new();
    capped.add_list(vec![Modifier::number(
        "BuffExpireFaster",
        ModType::More,
        -90.0,
    )]);
    assert_eq!(debuff_duration_mult(&capped, &cfg), 4.0);

    // 面板口径（mode_effective=false）恒 1（vendor :1834 门控）。
    let panel = CalcConfig::attack();
    assert_eq!(debuff_duration_mult(&enemy, &panel), 1.0);
}
