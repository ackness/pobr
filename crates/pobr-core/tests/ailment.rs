use pobr_core::calc::ailment::{
    AilmentSource, DamagingAilmentOutput, StackConfig, bleed_instance, bleed_traced, chill_effect,
    chill_effect_with_mods, chill_traced, corrupted_blood_instance, effmult_for_ailment,
    electrocute_poise_buildup, electrocute_poise_buildup_traced, flat_chance, freeze_poise_buildup,
    freeze_poise_buildup_traced, ignite_instance, ignite_traced, player_ailment_threshold,
    poison_instance, roll_average, shock_effect, stack_potential, stacking_ailment_dps,
    stacking_ailment_dps_traced, threshold_derived_chance, weighted_source_damage,
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

// ---------------------------------------------------------------------------
// Step 3 (Lane B): 冰缓 effect / 冰冻+电击 Poise buildup / 叠层权重平均
// ---------------------------------------------------------------------------

// --- 冰缓 effect (chill-effect-missing) ---

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

// --- 冰冻/电击 Poise 积累 (freeze-electrocute-buildup-missing) ---

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

// --- 叠层权重平均 DPS (ailment-stacking) ---

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

    // 溢出：active > max → clamp 1.0
    let cfg_over = StackConfig::new(5, 8.0);
    assert_eq!(
        stack_potential(&cfg_over),
        1.0,
        "overflow → potential clamped 1.0"
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
