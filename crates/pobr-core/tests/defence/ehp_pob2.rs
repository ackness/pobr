//! （F-1）：EHP PoB2 口径新管线集成测试。
//!
//! vendor 对照：`vendor/PathOfBuilding-PoE2/src/Modules/CalcDefence.lua`
//! - `numberOfHitsToDie`：:2979-3145（循环扣池 + 递归加速 + overkill 折算）；
//! - 敌人进伤 placeholder：ConfigOptions.lua:1982-1996；
//! - 新 max hit：:3540-3697（TotalHitPool 池扩展层 + 二次方程解 + 平滑）；
//! - not-hit 层：:2015-2026；mitigation 层：:3155-3247；TotalEHP：:3322。
//!
//! F-3 口径切换后：canonical `total_ehp`/`*_max_hit` = 新口径值（`*_pob2` 为同值
//! 别名），旧 lowest-max-hit 口径保留在 `total_ehp_lowest_max_hit`。

use pobr_core::calc::actor::{Actor, ActorBaseStats};
use pobr_core::calc::env::Env;
use pobr_core::calc::perform::perform;
use pobr_core::calc::setup_env::setup_enemy;
use pobr_core::calc::{
    EhpLoopParams, MaxHitInputs, MitigationCtx, PoolCtx, PoolState, TypedDamage,
    enemy_damage_placeholder, max_hit_pob2, not_hit_suite, number_of_hits_to_die, reduce_pools,
    taken_hit_per_type,
};
use pobr_core::{CalcConfig, Modifier};
use pobr_data::prelude::*;

fn player_with(base: ActorBaseStats, mods: Vec<Modifier>) -> Env {
    let mut actor = Actor::new(85, base);
    actor.mod_db.add_list(mods);
    Env::new(actor)
}

fn default_params() -> EhpLoopParams {
    EhpLoopParams::from_constants(&RuntimeConstants::default(), false)
}

/// 朴素逐击路径（无递归加速）：与 vendor 主循环同语义但 iterationMultiplier 恒 1，
/// 末端同样做 overkill 小数折算。加速路径等价性的对照基准。
fn naive_hits_to_die(damage_in: &TypedDamage, pools_full: &PoolState, ctx: &PoolCtx) -> f64 {
    let per_hit = damage_in.total();
    if per_hit <= 0.0 {
        return f64::INFINITY;
    }
    let mut pool = pools_full.clone();
    let mut hits = 0.0;
    let mut last_overkill = 0.0;
    // 上限远大于 ehp_calc_max_iterations 能覆盖的击数，保证朴素路径不截断。
    for _ in 0..100_000 {
        if pool.life <= 0.0 {
            break;
        }
        let after = reduce_pools(&pool, damage_in, ctx);
        last_overkill = after.overkill;
        pool = after.pools;
        hits += 1.0;
    }
    if pool.life <= 0.0 {
        hits -= last_overkill / per_hit;
    }
    hits
}

// 敌人进伤 placeholder（ConfigOptions.lua:1982-1996）

/// 手算锁值：`monsterDamageTable[82] = 353.67`（monster_scaling.json）。
/// Pinnacle（DPSMult = 8/4.4）：round(353.67 × 1.5 × 8/4.4) = round(964.55) = 965；
/// chaos = round(965 / 2.5) = 386。None（1/4.4）：round(120.57) = 121、chaos 48。
#[test]
fn enemy_damage_placeholder_matches_vendor_formula() {
    // Arrange
    let constants = RuntimeConstants::default();

    // Act
    let pinnacle = enemy_damage_placeholder(&constants, 82, EnemyTier::Pinnacle);
    let none = enemy_damage_placeholder(&constants, 82, EnemyTier::None);

    // Assert
    assert_eq!(pinnacle.physical, 965.0);
    assert_eq!(pinnacle.fire, 965.0);
    assert_eq!(pinnacle.cold, 965.0);
    assert_eq!(pinnacle.lightning, 965.0);
    assert_eq!(pinnacle.chaos, 386.0);
    assert_eq!(none.physical, 121.0);
    assert_eq!(none.chaos, 48.0);
}

// numberOfHitsToDie（CalcDefence.lua:2979-3145）

/// 纯生命池整除：1000 life / 100 per hit = 10 击（overkill 0，无小数折算）。
#[test]
fn hits_to_die_exact_division_on_pure_life() {
    // Arrange
    let pools = PoolState {
        life: 1000.0,
        ..Default::default()
    };
    let ctx = PoolCtx {
        max_life: 1000.0,
        ..Default::default()
    };
    let hit = TypedDamage {
        physical: 100.0,
        ..Default::default()
    };

    // Act
    let hits = number_of_hits_to_die(&hit, &pools, &ctx, &default_params());

    // Assert
    assert!((hits - 10.0).abs() < 1e-9, "hits = {hits}");
}

/// overkill 小数折算（:3133-3135）：1000 life / 300 per hit → 4 击溢出 200 →
/// numHits = 4 − 200/300 = 3.3333…（手算）。
#[test]
fn hits_to_die_overkill_fractional_fold() {
    // Arrange
    let pools = PoolState {
        life: 1000.0,
        ..Default::default()
    };
    let ctx = PoolCtx {
        max_life: 1000.0,
        ..Default::default()
    };
    let hit = TypedDamage {
        physical: 300.0,
        ..Default::default()
    };

    // Act
    let hits = number_of_hits_to_die(&hit, &pools, &ctx, &default_params());

    // Assert
    assert!((hits - (4.0 - 200.0 / 300.0)).abs() < 1e-9, "hits = {hits}");
}

/// 进伤为 0 → ∞（:2984-2988）；WardNotBreak 且单击总伤低于 Ward → ∞（:2990）。
#[test]
fn hits_to_die_infinite_branches() {
    // Arrange
    let pools = PoolState {
        life: 1000.0,
        ward: 500.0,
        ..Default::default()
    };
    let ctx = PoolCtx {
        max_life: 1000.0,
        ward_not_break: true,
        ..Default::default()
    };

    // Act / Assert：零进伤。
    let zero = TypedDamage::default();
    assert!(number_of_hits_to_die(&zero, &pools, &ctx, &default_params()).is_infinite());
    // Act / Assert：永续结界完全吸收（300 < 500）。
    let small = TypedDamage {
        fire: 300.0,
        ..Default::default()
    };
    assert!(number_of_hits_to_die(&small, &pools, &ctx, &default_params()).is_infinite());
}

/// 加速路径 vs 朴素逐击路径等价（Track F 测试计划；加速只是跳步优化，
/// 对线性消耗的池形态结果必须一致）。覆盖三类池形态：
/// ① life+ES（单伤害类型，分层线性消耗）；② MoM 30%；③ guard 比例吸收层。
#[test]
fn accelerated_path_matches_naive_per_hit_path() {
    let params = default_params();

    // ① life + ES（物理单类型：分层池线性消耗，跳步与逐击逐值相等）。
    let pools_a = PoolState {
        life: 1200.0,
        energy_shield: 800.0,
        ..Default::default()
    };
    let ctx_a = PoolCtx {
        max_life: 1200.0,
        ..Default::default()
    };
    let hit_a = TypedDamage {
        physical: 90.0,
        ..Default::default()
    };

    // ② MoM 30%（:602-609）。
    let pools_b = PoolState {
        life: 1000.0,
        mana: 600.0,
        ..Default::default()
    };
    let ctx_b = PoolCtx {
        max_life: 1000.0,
        mom_shared: 30.0,
        ..Default::default()
    };
    let hit_b = TypedDamage {
        fire: 130.0,
        ..Default::default()
    };

    // ③ shared guard 20% 比例吸收（:563-568）。
    let pools_c = PoolState {
        life: 900.0,
        guard_shared: 400.0,
        guard_shared_rate: 20.0,
        ..Default::default()
    };
    let ctx_c = PoolCtx {
        max_life: 900.0,
        ..Default::default()
    };
    let hit_c = TypedDamage {
        physical: 75.0,
        lightning: 40.0,
        ..Default::default()
    };

    for (pools, ctx, hit) in [
        (&pools_a, &ctx_a, &hit_a),
        (&pools_b, &ctx_b, &hit_b),
        (&pools_c, &ctx_c, &hit_c),
    ] {
        // Act
        let fast = number_of_hits_to_die(hit, pools, ctx, &params);
        let naive = naive_hits_to_die(hit, pools, ctx);

        // Assert
        assert!(
            (fast - naive).abs() < 1e-6,
            "accelerated {fast} != naive {naive} (hit = {hit:?})"
        );
    }
}

/// 混合类型 + ES 双倍扣减的跳步**近似**界：vendor 加速把多击合并为一记大击
/// （:3105-3119），而 chaos 对 ES 双倍（:582）使「每击内 chaos 先吃 ES」的次序
/// 在合并后改变——vendor 自身即接受该近似（PoB2 口径以加速值为准）。
/// 锁定：与朴素路径偏差有界（<10%），且加速值不高估生存（不大于朴素值 + ε）。
#[test]
fn accelerated_path_bounded_deviation_on_mixed_chaos_es() {
    // Arrange：chaos 双倍扣 ES + 物理混合（跳步跨 ES→life 边界的最坏形态）。
    let pools = PoolState {
        life: 1200.0,
        energy_shield: 800.0,
        ..Default::default()
    };
    let ctx = PoolCtx {
        max_life: 1200.0,
        ..Default::default()
    };
    let hit = TypedDamage {
        physical: 90.0,
        chaos: 60.0,
        ..Default::default()
    };

    // Act
    let fast = number_of_hits_to_die(&hit, &pools, &ctx, &default_params());
    let naive = naive_hits_to_die(&hit, &pools, &ctx);

    // Assert：手算 vendor 语义 fast = 10.9667、naive = 11.7333（偏差 ~6.5%）。
    assert!(
        (fast - naive).abs() / naive < 0.10,
        "fast {fast} vs naive {naive}"
    );
    assert!(
        fast <= naive + 1e-9,
        "加速路径不应高估生存：{fast} > {naive}"
    );
}

// taken_hit_per_type（panel 路径 :2171-2444）

/// 中性快照恒等：shift 恒等、乘数 1 → TakenHit = 进伤原值、面板 DR = 0。
#[test]
fn taken_hit_per_type_neutral_identity() {
    // Arrange
    let mit = MitigationCtx::default();
    let damage = TypedDamage {
        physical: 800.0,
        fire: 500.0,
        ..Default::default()
    };

    // Act
    let (taken, dr) = taken_hit_per_type(&damage, &mit);

    // Assert
    assert_eq!(taken.physical, 800.0);
    assert_eq!(taken.fire, 500.0);
    assert_eq!(taken.chaos, 0.0);
    assert_eq!(dr, [0.0; 5]);
}

/// 抗性 + 护甲减伤（:2402-2409/:2442）：fire 抗 75%、物理 effArmour 2000 vs 进伤 1000
/// → 护甲 DR = round(2000/(2000+10×1000)×100) = round(16.67) = **17%**（vendor
/// `calcs.armourReduction` 是取整变体，Common.lua round=floor(x+0.5)）→
/// 物理承受 = 1000 × (1−0.17) = 830；fire 承受 = 500 × 0.25 = 125。
#[test]
fn taken_hit_per_type_applies_resist_and_armour() {
    // Arrange
    let mut mit = MitigationCtx::default();
    mit.effective_applied_armour[0] = 2000.0;
    mit.resist_taken_multi[DamageType::Fire as usize] = 0.25;
    let damage = TypedDamage {
        physical: 1000.0,
        fire: 500.0,
        ..Default::default()
    };

    // Act
    let (taken, dr) = taken_hit_per_type(&damage, &mit);

    // Assert
    let armour_dr_pct = (2000.0_f64 / (2000.0 + 10.0 * 1000.0) * 100.0).round(); // 17
    assert!((dr[0] - armour_dr_pct).abs() < 1e-6);
    assert!((taken.physical - 1000.0 * (1.0 - armour_dr_pct / 100.0)).abs() < 1e-3);
    assert!((taken.fire - 125.0).abs() < 1e-9);
}

// max_hit_pob2（:3540-3697）

fn max_hit_inputs<'a>(
    mit: &'a MitigationCtx,
    pools: &'a PoolState,
    ctx: &'a PoolCtx,
    pool_by_type: [f64; 5],
) -> MaxHitInputs<'a> {
    MaxHitInputs {
        mit,
        pools_full: pools,
        ctx,
        total_hit_pool: pool_by_type,
        armour_ratio: 10.0,
        smoothing_passes: 8,
    }
}

/// 无护甲、全额转换的闭式分支（:3614-3617）：池 1000、fire 抗 75%（resMult 0.25）
/// → max hit = 1000 / 0.25 = 4000（手算）。
#[test]
fn max_hit_simple_branch_resist_only() {
    // Arrange
    let mut mit = MitigationCtx::default();
    mit.resist_taken_multi[DamageType::Fire as usize] = 0.25;
    let pools = PoolState::default();
    let ctx = PoolCtx::default();
    let inputs = max_hit_inputs(&mit, &pools, &ctx, [1000.0; 5]);

    // Act
    let hit = max_hit_pob2(DamageType::Fire, &inputs, 1.0);

    // Assert
    assert_eq!(hit, 4000.0);
}

/// 物理二次方程分支（:3620-3641）自洽性：解出的 RAW 经 takenHit 链回代应恰好
/// 等于 TotalHitPool（armour DR 随击中大小变化的自洽条件；floor 引入 <1 误差）。
#[test]
fn max_hit_quadratic_is_self_consistent_with_taken_hit() {
    // Arrange：effArmour 3000、池 2000、无 flat DR/overwhelm。
    let mut mit = MitigationCtx::default();
    mit.effective_applied_armour[0] = 3000.0;
    let pools = PoolState::default();
    let ctx = PoolCtx::default();
    let inputs = max_hit_inputs(&mit, &pools, &ctx, [2000.0; 5]);

    // Act
    let raw = max_hit_pob2(DamageType::Physical, &inputs, 1.0);

    // Assert：回代 taken = RAW × (1 − armour/(armour + 10×RAW))，应 ≈ 池（floor 容差 ≤ 2）。
    let armour_dr = 3000.0 / (3000.0 + 10.0 * raw);
    let taken_back = raw * (1.0 - armour_dr);
    assert!(
        (taken_back - 2000.0).abs() < 2.0,
        "raw = {raw}, taken_back = {taken_back}"
    );
}

/// 承受乘数 0（CI 的 ChaosDamageTaken MORE −100 等价）→ 免疫，max hit = ∞。
#[test]
fn max_hit_zero_taken_multi_is_immune() {
    // Arrange
    let mut mit = MitigationCtx::default();
    mit.after_reduction_multi[DamageType::Chaos as usize] = 0.0;
    let pools = PoolState::default();
    let ctx = PoolCtx::default();
    let inputs = max_hit_inputs(&mit, &pools, &ctx, [1000.0; 5]);

    // Act / Assert
    assert!(max_hit_pob2(DamageType::Chaos, &inputs, 1.0).is_infinite());
}

// not-hit 层（:2015-2026）

/// 四分型 NotHit 合成：evade 30% + avoidAll 20% →
/// melee NotHit = 100 − 0.7 × 0.8 × 100 = 44%（手算）；
/// projectile 再乘 avoidProj 10% → 100 − 0.7 × 0.8 × 0.9 × 100 = 49.6%。
#[test]
fn not_hit_suite_combines_evade_and_avoidance() {
    // Arrange
    let out = pobr_core::calc::OutputTable {
        melee_evade_chance: 30.0,
        projectile_evade_chance: 30.0,
        spell_evade_chance: 0.0,
        spell_projectile_evade_chance: 0.0,
        avoid_all_damage_from_hits: 20.0,
        avoid_projectile_damage: 10.0,
        ..Default::default()
    };

    // Act
    let nh = not_hit_suite(&out);

    // Assert
    assert!((nh.melee - 44.0).abs() < 1e-9);
    assert!((nh.projectile - 49.6).abs() < 1e-9);
    assert!((nh.spell - 20.0).abs() < 1e-9);
    assert!((nh.spell_projectile - 28.0).abs() < 1e-9);
    assert!((nh.average - (44.0 + 49.6 + 20.0 + 28.0) / 4.0).abs() < 1e-9);
}

// 端到端：perform 双跑不变式 + 新字段产出

/// 端到端（setup_enemy 注入 placeholder → perform）：新字段全部产出，
/// 且 F-3 切换不变式成立——canonical `total_ehp` == `total_ehp_pob2`。
#[test]
fn perform_fills_pob2_ehp_fields_with_enemy() {
    // Arrange
    let base = ActorBaseStats {
        life: 2000.0,
        mana: 500.0,
        armour: 1500.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(base, vec![]);
    env.cfg = CalcConfig::attack().with_damage_type(DamageType::Physical);
    setup_enemy(&mut env, 82, EnemyTier::Pinnacle);

    // Act
    perform(&mut env).unwrap();
    let out = &env.player.output;

    // Assert：池口径字段。
    assert_eq!(out.life_recoverable, 2000.0);
    assert_eq!(out.energy_shield_recovery_cap, 0.0);
    // 敌人进伤（Pinnacle@82 placeholder：965×4 + 386 = 4246，手算自上方锁值）。
    assert_eq!(out.total_enemy_damage_in, 965.0 * 4.0 + 386.0);
    // 致死击数 / 新口径 EHP / 新 max hit 全部产出为正有限值。
    assert!(out.number_of_damaging_hits.is_finite() && out.number_of_damaging_hits > 0.0);
    assert!(out.number_of_mitigated_hits >= out.number_of_damaging_hits - 1e-9);
    assert!(out.total_ehp_pob2 > 0.0 && out.total_ehp_pob2.is_finite());
    assert!(out.physical_max_hit_pob2 > 0.0);
    assert!(out.chaos_max_hit_pob2 > 0.0);
    // 面板物理减伤（护甲 1500 vs 进伤 → DR > 0）。
    assert!(out.physical_damage_reduction > 0.0);

    // F-3 口径切换：canonical `total_ehp` = 新口径值（`total_ehp_pob2` 保留为同值
    // 别名）；旧 lowest-max-hit 口径保留在 `total_ehp_lowest_max_hit`（不删码）。
    assert_eq!(out.total_ehp, out.total_ehp_pob2);
    assert_eq!(out.physical_max_hit, out.physical_max_hit_pob2);
    assert_eq!(out.chaos_max_hit, out.chaos_max_hit_pob2);
    assert!(out.total_ehp > 0.0);
    assert!(out.total_ehp_lowest_max_hit > 0.0);
}

/// 裸 Env（无 setup_enemy → 无进伤 placeholder）：新管线中性短路——
/// 致死击数 ∞、`total_ehp_pob2 = 0`，旧口径字段照常产出（向后兼容）。
#[test]
fn perform_without_enemy_damage_is_neutral() {
    // Arrange
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(base, vec![]);

    // Act
    perform(&mut env).unwrap();
    let out = &env.player.output;

    // Assert
    assert_eq!(out.total_enemy_damage_in, 0.0);
    assert!(out.number_of_damaging_hits.is_infinite());
    assert_eq!(out.total_ehp_pob2, 0.0);
    // F-3 口径切换：canonical total_ehp = 新口径 → 无进伤时 0 中性；
    // 旧 lowest-max-hit 口径保留在附加字段（向后兼容）。
    assert_eq!(out.total_ehp, 0.0);
    assert!(out.total_ehp_lowest_max_hit > 0.0);
    // max hit 新管线在中性输入下与旧自洽迭代解数学等价（F-2 报告 §3.1）。
    assert_eq!(out.fire_max_hit, 1000.0);
}

/// MoM build 端到端：30% MoM（DamageTakenFromManaBeforeLife BASE 30）应显著提高
/// 致死击数与新口径 EHP（mana 池被折入）。
#[test]
fn perform_mom_increases_pob2_ehp() {
    // Arrange：同基础属性，带 / 不带 MoM 词条各跑一次。
    let base = ActorBaseStats {
        life: 1500.0,
        mana: 900.0,
        ..ActorBaseStats::default()
    };
    let mut plain = player_with(base, vec![]);
    setup_enemy(&mut plain, 82, EnemyTier::Pinnacle);
    perform(&mut plain).unwrap();

    let mut mom = player_with(
        base,
        vec![Modifier::number(
            "DamageTakenFromManaBeforeLife",
            ModType::Base,
            30.0,
        )],
    );
    setup_enemy(&mut mom, 82, EnemyTier::Pinnacle);
    perform(&mut mom).unwrap();

    // Assert：MoM 提升致死击数与新口径 EHP（PoB2 :602-609 / :2726-2771）。
    assert!(
        mom.player.output.number_of_damaging_hits > plain.player.output.number_of_damaging_hits
    );
    assert!(mom.player.output.total_ehp_pob2 > plain.player.output.total_ehp_pob2);
}
