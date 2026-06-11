//! 扣池状态机 + 池整备集成测试（M2 Track A，13-G2/G3/G5）。
//!
//! 每个 fixture 的期望值**手算自 PoB2 公式**，注释标注
//! `vendor/PathOfBuilding-PoE2/src/Modules/CalcDefence.lua` 行号。
//! 末段两组 oracle 对拍数值取自 pob2-oracle（tools/pob2-oracle）对
//! `examples/demo-bd-test/builds/{sorceress-stormweaver-comet,
//! witch-abyssal-lich-detonate-dead}/decoded.xml` 的 mainOutput 平铺（2026-06-11 跑取）。

use pobr_core::calc::pool_damage::{
    AllyLayer, PoolCtx, PoolState, TypedDamage, extend_total_hit_pool, reduce_pools,
    total_hit_pool_base,
};
use pobr_core::calc::pool_setup::{PoolBaseStats, build_pool_ctx, build_pool_state, mom_hit_pools};
use pobr_core::rules::DefenceKeystones;
use pobr_core::{CalcConfig, ModDb, Modifier};
use pobr_data::constants::DamageType;
use pobr_data::modifier::ModType;

const EPS: f64 = 1e-9;

fn phys(amount: f64) -> TypedDamage {
    TypedDamage {
        physical: amount,
        ..Default::default()
    }
}

fn life_only(life: f64) -> PoolState {
    PoolState {
        life,
        ..Default::default()
    }
}

fn ctx_with_life(max_life: f64) -> PoolCtx {
    PoolCtx {
        max_life,
        ..Default::default()
    }
}

/// 按层 ID 汇总某类型的扣量明细。
fn lost(after: &pobr_core::calc::pool_damage::PoolsAfter, dtype: DamageType, layer: &str) -> f64 {
    after
        .resources_lost
        .iter()
        .filter(|(t, l, _)| *t == dtype && *l == layer)
        .map(|(_, _, v)| v)
        .sum()
}

// ---------------------------------------------------------------------------
// 逐层独立 fixture
// ---------------------------------------------------------------------------

/// 纯 life：部分扣减（:651-655）。手算：life 1000 − 400 = 600；recoupable = 400
/// （:531，allies 后全额计入）；hit_pool_remaining = floor(600)（:659，无 MoM/ES）。
#[test]
fn pure_life_partial_hit() {
    // Arrange
    let pools = life_only(1000.0);
    let ctx = ctx_with_life(1000.0);

    // Act
    let after = reduce_pools(&pools, &phys(400.0), &ctx);

    // Assert
    assert!((after.pools.life - 600.0).abs() < EPS);
    assert!((after.recoupable_by_type[DamageType::Physical as usize] - 400.0).abs() < EPS);
    assert_eq!(after.overkill, 0.0);
    assert_eq!(after.hit_pool_remaining, 600.0);
    assert!((lost(&after, DamageType::Physical, "life") - 400.0).abs() < EPS);
}

/// 纯 life 溢出击杀（:656-657 overkill 折算）。手算：life 100、hit 250 →
/// life 0、overkill 150、hit_pool_remaining 0。
#[test]
fn pure_life_overkill() {
    // Arrange / Act
    let after = reduce_pools(&life_only(100.0), &phys(250.0), &ctx_with_life(100.0));

    // Assert
    assert_eq!(after.pools.life, 0.0);
    assert!((after.overkill - 150.0).abs() < EPS);
    assert_eq!(after.hit_pool_remaining, 0.0);
    assert!((lost(&after, DamageType::Physical, "overkill") - 150.0).abs() < EPS);
}

/// life + ES（:594-601 普通分支，bypass=0 → MoMEBPool = ES 全额）。
/// 手算：hit 1200，ES 500 全吃 → rem 700 → life 1000−700 = 300；
/// esPoolRemaining = 0 → hit_pool_remaining = floor(300+0) = 300。
#[test]
fn life_plus_energy_shield() {
    // Arrange
    let pools = PoolState {
        life: 1000.0,
        energy_shield: 500.0,
        ..Default::default()
    };

    // Act
    let after = reduce_pools(&pools, &phys(1200.0), &ctx_with_life(1000.0));

    // Assert
    assert_eq!(after.pools.energy_shield, 0.0);
    assert!((after.pools.life - 300.0).abs() < EPS);
    assert_eq!(after.hit_pool_remaining, 300.0);
    assert!((lost(&after, DamageType::Physical, "energy_shield") - 500.0).abs() < EPS);
}

/// chaos 对 ES 双倍（:582 `esDamageTypeMultiplier=2`）。手算：chaos 600、ES 500 →
/// ES 能挡 500/2 = 250 伤害（耗 500 ES）→ rem 350 → life 650；
/// `ChaosNotDoubleESDamage` 时恢复 1 倍：挡 500 → rem 100 → life 900。
#[test]
fn chaos_double_es_damage_and_opt_out() {
    // Arrange
    let pools = PoolState {
        life: 1000.0,
        energy_shield: 500.0,
        ..Default::default()
    };
    let hit = TypedDamage {
        chaos: 600.0,
        ..Default::default()
    };

    // Act：默认双倍。
    let after = reduce_pools(&pools, &hit, &ctx_with_life(1000.0));
    // Act：ChaosNotDoubleESDamage 关闭双倍。
    let ctx_no_double = PoolCtx {
        max_life: 1000.0,
        chaos_not_double_es: true,
        ..Default::default()
    };
    let after_no_double = reduce_pools(&pools, &hit, &ctx_no_double);

    // Assert
    assert_eq!(after.pools.energy_shield, 0.0);
    assert!((after.pools.life - 650.0).abs() < EPS);
    assert!((lost(&after, DamageType::Chaos, "energy_shield") - 500.0).abs() < EPS);
    assert!((after_no_double.pools.life - 900.0).abs() < EPS);
}

/// ES bypass 30%（:594-601）。手算：hit 1000、ES 700、life 1000、无 MoM：
/// MoMEBPool = min((0+1000)/0.3 − 1000, 700) = 700；temp = min(1000×0.7, 700) = 700
/// → ES 0、rem 300 → life 700。
#[test]
fn es_bypass_thirty_percent() {
    // Arrange
    let pools = PoolState {
        life: 1000.0,
        energy_shield: 700.0,
        ..Default::default()
    };
    let mut ctx = ctx_with_life(1000.0);
    ctx.es_bypass_by_type = [30.0; 5];

    // Act
    let after = reduce_pools(&pools, &phys(1000.0), &ctx);

    // Assert
    assert_eq!(after.pools.energy_shield, 0.0);
    assert!((after.pools.life - 700.0).abs() < EPS);
}

/// MoM 30%（:585-586/:602-608）。手算：hit 1000、life 2000、mana 500：
/// lifeHitPool = 2000；MoMPool = min(2000/0.7 − 2000, 500) = 500；
/// MoM 份额 = 1000×0.3 = 300 → mana 200、rem 700 → life 1300；
/// MoMPoolRemaining = 200 → hit_pool_remaining = floor(1300+200) = 1500。
#[test]
fn mom_thirty_percent() {
    // Arrange
    let pools = PoolState {
        life: 2000.0,
        mana: 500.0,
        ..Default::default()
    };
    let ctx = PoolCtx {
        max_life: 2000.0,
        mom_shared: 30.0,
        ..Default::default()
    };

    // Act
    let after = reduce_pools(&pools, &phys(1000.0), &ctx);

    // Assert
    assert!((after.pools.mana - 200.0).abs() < EPS);
    assert!((after.pools.life - 1300.0).abs() < EPS);
    assert_eq!(after.hit_pool_remaining, 1500.0);
    assert!((lost(&after, DamageType::Physical, "mana") - 300.0).abs() < EPS);
}

/// MoM 100%（:585 `MoMEffect=1` → MoMPool = mana 全额）。手算：hit 1000、
/// life 800、mana 600：MoM 份额 = 1000×1 = 1000，cap mana 600 → rem 400 → life 400。
#[test]
fn mom_full_takes_all_mana_first() {
    // Arrange
    let pools = PoolState {
        life: 800.0,
        mana: 600.0,
        ..Default::default()
    };
    let ctx = PoolCtx {
        max_life: 800.0,
        mom_shared: 100.0,
        ..Default::default()
    };

    // Act
    let after = reduce_pools(&pools, &phys(1000.0), &ctx);

    // Assert
    assert_eq!(after.pools.mana, 0.0);
    assert!((after.pools.life - 400.0).abs() < EPS);
}

/// guard：shared 按 AbsorbRate% 比例吸收、cap 于池量（:563-568）；per-type 同构
/// （:557-562）。手算：shared rate 20%、池 300、hit 1000 → 吸 200（rem 800）；
/// fire guard rate 50%、池 100、fire hit 300 → 吸 min(150,100)=100（rem 200）。
#[test]
fn guard_absorb_rate_and_limit() {
    // Arrange
    let mut pools = PoolState {
        life: 5000.0,
        guard_shared: 300.0,
        guard_shared_rate: 20.0,
        ..Default::default()
    };
    pools.guard_by_type[DamageType::Fire as usize] = 100.0;
    pools.guard_rate_by_type[DamageType::Fire as usize] = 50.0;
    let hit = TypedDamage {
        physical: 1000.0,
        fire: 300.0,
        ..Default::default()
    };
    let ctx = ctx_with_life(5000.0);

    // Act
    let after = reduce_pools(&pools, &hit, &ctx);

    // Assert：正序 Physical 先消耗 shared guard 200，Fire 先 per-type 100、
    // 再 shared 剩余 100 的 20% 份额 min(200×0.2,100)=40。
    assert!((lost(&after, DamageType::Physical, "shared_guard") - 200.0).abs() < EPS);
    assert!((lost(&after, DamageType::Fire, "guard") - 100.0).abs() < EPS);
    assert!((lost(&after, DamageType::Fire, "shared_guard") - 40.0).abs() < EPS);
    assert!((after.pools.guard_shared - 60.0).abs() < EPS);
    assert_eq!(after.pools.guard_by_type[DamageType::Fire as usize], 0.0);
    // life = 5000 − 800（phys 余）− 160（fire 余）。
    assert!((after.pools.life - 4040.0).abs() < EPS);
}

/// ward + bypass（:569-573）与 WardNotBreak 返还（:509/:666）。
/// 手算：ward 500、bypass 40% → 吸 min(1000×0.6, 500)=500、rem 500；
/// 击后 ward 归零（破盾）；WardNotBreak 时返还原值 500。
#[test]
fn ward_bypass_and_not_break_restore() {
    // Arrange
    let pools = PoolState {
        life: 2000.0,
        ward: 500.0,
        ..Default::default()
    };
    let mut ctx = ctx_with_life(2000.0);
    ctx.ward_bypass = 40.0;

    // Act：默认破盾。
    let after = reduce_pools(&pools, &phys(1000.0), &ctx);
    // Act：WardNotBreak 返还。
    let mut ctx_nb = ctx.clone();
    ctx_nb.ward_not_break = true;
    let after_nb = reduce_pools(&pools, &phys(1000.0), &ctx_nb);

    // Assert
    assert!((lost(&after, DamageType::Physical, "ward") - 500.0).abs() < EPS);
    assert!((after.pools.life - 1500.0).abs() < EPS);
    assert_eq!(after.pools.ward, 0.0);
    assert_eq!(after_nb.pools.ward, 500.0);
    assert!((after_nb.pools.life - 1500.0).abs() < EPS);
}

/// EternalLife + bypass（:587-594）：bypass 部分整体免除、不穿透到生命。
/// 手算：ES 1000、bypass 20%、hit 800：temp = min(800, 1000/0.8) = 800；
/// ES −= 800×0.8 = 640 → 360；rem = 0 → life 不动；免除 = 800×0.2 = 160。
#[test]
fn eternal_life_prevents_bypass_portion() {
    // Arrange
    let pools = PoolState {
        life: 500.0,
        energy_shield: 1000.0,
        ..Default::default()
    };
    let mut ctx = ctx_with_life(500.0);
    ctx.eternal_life = true;
    ctx.es_bypass_by_type = [20.0; 5];

    // Act
    let after = reduce_pools(&pools, &phys(800.0), &ctx);

    // Assert
    assert!((after.pools.energy_shield - 360.0).abs() < EPS);
    assert!((after.pools.life - 500.0).abs() < EPS);
    assert!((lost(&after, DamageType::Physical, "energy_shield") - 640.0).abs() < EPS);
    assert!((lost(&after, DamageType::Physical, "eternal_life_prevented") - 160.0).abs() < EPS);
}

/// allies 先扣层（:525-530）：按 percent 比例分担、cap 于层余量，**不计入 recoupable**
/// （:531）。手算：层余量 300、50% → 吸 min(1000×0.5, 300)=300、rem 700；
/// recoupable = 700（仅 allies 后的部分）。
#[test]
fn ally_layer_absorbs_before_recoup() {
    // Arrange
    let pools = PoolState {
        life: 2000.0,
        allies: vec![AllyLayer {
            id: "frostShield",
            remaining: 300.0,
            mitigation_pct: 50.0,
            damage_type: None,
        }],
        ..Default::default()
    };

    // Act
    let after = reduce_pools(&pools, &phys(1000.0), &ctx_with_life(2000.0));

    // Assert
    assert!((after.recoupable_by_type[DamageType::Physical as usize] - 700.0).abs() < EPS);
    assert_eq!(after.pools.allies[0].remaining, 0.0);
    assert!((after.pools.life - 1300.0).abs() < EPS);
    assert!((lost(&after, DamageType::Physical, "frostShield") - 300.0).abs() < EPS);
}

/// aegis 扣减顺序（per-type :539 → sharedElemental 仅元素 :545 → shared :551）。
/// 手算：fire 500 依次吃 fire aegis 100、sharedElemental 150、shared 200 → rem 50。
#[test]
fn aegis_layers_in_order() {
    // Arrange
    let mut pools = PoolState {
        life: 1000.0,
        aegis_shared: 200.0,
        aegis_shared_elemental: 150.0,
        ..Default::default()
    };
    pools.aegis_by_type[DamageType::Fire as usize] = 100.0;
    let hit = TypedDamage {
        fire: 500.0,
        ..Default::default()
    };

    // Act
    let after = reduce_pools(&pools, &hit, &ctx_with_life(1000.0));

    // Assert
    assert!((lost(&after, DamageType::Fire, "aegis") - 100.0).abs() < EPS);
    assert!((lost(&after, DamageType::Fire, "shared_elemental_aegis") - 150.0).abs() < EPS);
    assert!((lost(&after, DamageType::Fire, "shared_aegis") - 200.0).abs() < EPS);
    assert!((after.pools.life - 950.0).abs() < EPS);
    // 物理不能吃 sharedElemental（:545 isElemental 门）。
    let phys_after = reduce_pools(
        &PoolState {
            life: 1000.0,
            aegis_shared_elemental: 150.0,
            ..Default::default()
        },
        &phys(100.0),
        &ctx_with_life(1000.0),
    );
    assert_eq!(phys_after.pools.aegis_shared_elemental, 150.0);
}

/// ES 段逆序遍历（:578 `for i=#dmgTypeList,1,-1` → Chaos 先）。
/// 手算：ES 500、fire 400 + chaos 300：chaos 先（双倍）耗尽 ES（挡 250、耗 500）、
/// rem chaos 50 → life；fire 无 ES 可吃，400 全进 life → life 2000−450 = 1550。
/// 若误为正序（fire 先），fire 吃 400 ES、chaos 双倍只能挡 50 → life 1650 ≠ 本期望。
#[test]
fn es_consumed_in_reverse_type_order_chaos_first() {
    // Arrange
    let pools = PoolState {
        life: 2000.0,
        energy_shield: 500.0,
        ..Default::default()
    };
    let hit = TypedDamage {
        fire: 400.0,
        chaos: 300.0,
        ..Default::default()
    };

    // Act
    let after = reduce_pools(&pools, &hit, &ctx_with_life(2000.0));

    // Assert
    assert!((lost(&after, DamageType::Chaos, "energy_shield") - 500.0).abs() < EPS);
    assert_eq!(lost(&after, DamageType::Fire, "energy_shield"), 0.0);
    assert!((after.pools.life - 1550.0).abs() < EPS);
}

// ---------------------------------------------------------------------------
// loss-prevention 分段（:611-651）
// ---------------------------------------------------------------------------

/// 仅 above-half 防止（:643-647 else 分支）。手算：prev 20%、life 1000/1000、
/// hit 400：转入延迟损失 400×0.2 = 80、生命实扣 320 → life 680。
#[test]
fn loss_prevention_above_half_only() {
    // Arrange
    let mut ctx = ctx_with_life(1000.0);
    ctx.prevented_life_loss = 20.0;

    // Act
    let after = reduce_pools(&life_only(1000.0), &phys(400.0), &ctx);

    // Assert
    assert!((after.pools.life - 680.0).abs() < EPS);
    assert!((after.pools.life_loss_lost_over_time - 80.0).abs() < EPS);
    assert!((lost(&after, DamageType::Physical, "life_loss_prevented") - 80.0).abs() < EPS);
}

/// 仅 below-half 防止（:623-641 分段分支）。手算：belowHalf 50%、life 1000/1000、
/// hit 800：半血以上段 damageToSplit = min(800, 500) = 500 全额实扣 → life 500、
/// rem 300；入低血段：unspecific = 300×0 = 0、specific = 300×0.5 = 150 →
/// 延迟 150、rem 150 → life 350。
#[test]
fn loss_prevention_below_half_segments() {
    // Arrange
    let mut ctx = ctx_with_life(1000.0);
    ctx.life_loss_below_half_prevented = 50.0;

    // Act
    let after = reduce_pools(&life_only(1000.0), &phys(800.0), &ctx);

    // Assert
    assert!((after.pools.life - 350.0).abs() < EPS);
    assert_eq!(after.pools.life_loss_lost_over_time, 0.0);
    assert!((after.pools.life_below_half_loss_lost_over_time - 150.0).abs() < EPS);
}

/// 双段叠加 + 生命可承上限的 overkill 前置（:617-621）。手算：prev 20%、
/// belowHalf 50%、life 800/1000、hit 2000：
/// poolAboveLow = 300/0.8 = 375；可承 = 375 + 500/0.5/0.8 = 1625 < 2000 →
/// overkill = 375、rem = 1625；split = 375 → 实扣 300（life 500）、延迟 75；
/// 低血段：unspecific = 1250×0.2 = 250（延迟累计 325）、rem 1000；
/// specific = 1000×0.5 = 500（低血延迟 500）、rem 500 → life 0。
#[test]
fn loss_prevention_both_segments_with_overkill_clamp() {
    // Arrange
    let mut ctx = ctx_with_life(1000.0);
    ctx.prevented_life_loss = 20.0;
    ctx.life_loss_below_half_prevented = 50.0;

    // Act
    let after = reduce_pools(&life_only(800.0), &phys(2000.0), &ctx);

    // Assert
    assert_eq!(after.pools.life, 0.0);
    assert!((after.pools.life_loss_lost_over_time - 325.0).abs() < EPS);
    assert!((after.pools.life_below_half_loss_lost_over_time - 500.0).abs() < EPS);
    assert!((after.overkill - 375.0).abs() < EPS);
    assert_eq!(after.hit_pool_remaining, 0.0);
}

// ---------------------------------------------------------------------------
// max-hit TotalHitPool 池层（:2942-2960 基底 + :3540-3596 扩展）
// ---------------------------------------------------------------------------

/// ward bypass 的池扩展（:3544-3553）。手算：base 1000、ward 600、bypass 40%：
/// protected = 600/0.6×0.4 = 400；pool = max(1000−400,0) + min(1000,400)/0.4 =
/// 600 + 1000 = 1600；bypass=0 时平铺相加 1000+600 = 1600（巧合同值，再以
/// ward 200 区分：bypass 路径 protected = 133.33 → 866.67+333.33 = 1200 vs 平铺 1200——
/// 改用池小于 protected 的形状：base 300、ward 600、bypass 40% → protected 400 →
/// 0 + 300/0.4 = 750）。
#[test]
fn extend_pool_ward_bypass_vs_flat() {
    // Arrange
    let pools = PoolState {
        ward: 600.0,
        ..Default::default()
    };
    let ctx = PoolCtx {
        ward_bypass: 40.0,
        ..Default::default()
    };

    // Act / Assert：bypass 嵌套（池 < protected 段：全段放大 1/bypass）。
    let extended = extend_total_hit_pool(300.0, DamageType::Physical, &pools, &ctx);
    assert!((extended - 750.0).abs() < EPS);
    // bypass = 0 → 平铺相加（:3552）。
    let flat = extend_total_hit_pool(300.0, DamageType::Physical, &pools, &PoolCtx::default());
    assert!((flat - 900.0).abs() < EPS);
}

/// guard 池扩展只认 shared（vendor :3557 Lua 优先级求值语义）+ rate≥100 平铺
/// （:3560-3561）。手算：shared rate 20%、池 300 → protected = 300/0.2×0.8 = 1200；
/// base 1000 < 1200 → pool = 1000/0.8 = 1250；per-type guard 不参与。
#[test]
fn extend_pool_guard_shared_only() {
    // Arrange
    let mut pools = PoolState {
        guard_shared: 300.0,
        guard_shared_rate: 20.0,
        ..Default::default()
    };
    pools.guard_by_type[DamageType::Physical as usize] = 999.0;
    pools.guard_rate_by_type[DamageType::Physical as usize] = 50.0;
    let ctx = PoolCtx::default();

    // Act
    let extended = extend_total_hit_pool(1000.0, DamageType::Physical, &pools, &ctx);

    // Assert
    assert!((extended - 1250.0).abs() < EPS);
    // rate ≥ 100 → 平铺相加。
    let pools_full = PoolState {
        guard_shared: 300.0,
        guard_shared_rate: 100.0,
        ..Default::default()
    };
    let flat = extend_total_hit_pool(1000.0, DamageType::Physical, &pools_full, &ctx);
    assert!((flat - 1300.0).abs() < EPS);
}

/// aegis 池扩展取最强口径（:3555：max(perType, shared, 元素时 perType+sharedElemental)）。
/// 手算：fire perType 100 + sharedElemental 150 = 250 > shared 200 → +250。
#[test]
fn extend_pool_aegis_takes_strongest() {
    // Arrange
    let mut pools = PoolState {
        aegis_shared: 200.0,
        aegis_shared_elemental: 150.0,
        ..Default::default()
    };
    pools.aegis_by_type[DamageType::Fire as usize] = 100.0;
    let ctx = PoolCtx::default();

    // Act / Assert
    let fire = extend_total_hit_pool(1000.0, DamageType::Fire, &pools, &ctx);
    assert!((fire - 1250.0).abs() < EPS);
    // 物理无元素口径 → max(0, 200) = 200。
    let physical = extend_total_hit_pool(1000.0, DamageType::Physical, &pools, &ctx);
    assert!((physical - 1200.0).abs() < EPS);
}

// ---------------------------------------------------------------------------
// pob2-oracle 对拍（中间值锚定真实 build）
// ---------------------------------------------------------------------------

/// sorceress-stormweaver-comet（MoM 系）oracle 中间值对拍。
/// oracle mainOutput（2026-06-11）：Life=1581 / ManaUnreserved=2367 /
/// EnergyShieldRecoveryCap=987 / Ward=890 / sharedMindOverMatter=100 /
/// 全类型 EnergyShieldBypass=0 / sharedMoMHitPool=3948 /
/// PhysicalTotalHitPool=5825 / ChaosTotalHitPool=5331.5（chaos ES 双倍折半）。
#[test]
fn oracle_stormweaver_mom_pool_chain() {
    // Arrange：池整备输入按 oracle 口径。
    let base = PoolBaseStats {
        max_life: 1581.0,
        life_recoverable: 1581.0,
        mana_unreserved: 2367.0,
        energy_shield_recovery_cap: 987.0,
        ward: 890.0,
    };
    let ctx = PoolCtx {
        max_life: 1581.0,
        mom_shared: 100.0,
        ..Default::default()
    };

    // Act：MoM 池整备（:2726-2820）。
    let mom = mom_hit_pools(&ctx, &base);

    // Assert：sharedMoMHitPool = 1581 + 2367 = 3948（oracle 同值）。
    assert!((mom.shared_hit_pool - 3948.0).abs() < EPS);
    for idx in 0..5 {
        assert!((mom.hit_pool_by_type[idx] - 3948.0).abs() < EPS);
    }

    // Act：TotalHitPool 基底（:2942-2960）+ ward 扩展（:3540-3553）。
    let pools = PoolState {
        life: 1581.0,
        mana: 2367.0,
        energy_shield: 987.0,
        ward: 890.0,
        ..Default::default()
    };
    let phys_base = total_hit_pool_base(DamageType::Physical, 3948.0, 987.0, &ctx);
    let phys_total = extend_total_hit_pool(phys_base, DamageType::Physical, &pools, &ctx);
    let chaos_base = total_hit_pool_base(DamageType::Chaos, 3948.0, 987.0, &ctx);
    let chaos_total = extend_total_hit_pool(chaos_base, DamageType::Chaos, &pools, &ctx);

    // Assert：oracle PhysicalTotalHitPool=5825（=3948+987+890）、
    // ChaosTotalHitPool=5331.5（=3948+987/2+890）。
    assert!((phys_base - 4935.0).abs() < EPS);
    assert!((phys_total - 5825.0).abs() < EPS);
    assert!((chaos_total - 5331.5).abs() < EPS);

    // Act：恰好 TotalHitPool 大小的物理击中 → 状态机应精确清空全部池。
    let after = reduce_pools(&pools, &phys(5825.0), &ctx);

    // Assert：ward 890 → ES 987 → mana 2367（MoM 100）→ life 1581，零溢出。
    assert!((lost(&after, DamageType::Physical, "ward") - 890.0).abs() < EPS);
    assert!((lost(&after, DamageType::Physical, "energy_shield") - 987.0).abs() < EPS);
    assert!((lost(&after, DamageType::Physical, "mana") - 2367.0).abs() < EPS);
    assert!((lost(&after, DamageType::Physical, "life") - 1581.0).abs() < EPS);
    assert!(after.pools.life.abs() < 1e-6);
    assert!(after.overkill.abs() < 1e-6);
    assert_eq!(after.hit_pool_remaining, 0.0);
}

/// witch-abyssal-lich-detonate-dead（EternalLife + 全类型 bypass 27）oracle 对拍。
/// oracle mainOutput（2026-06-11）：Life=1694 / ManaUnreserved=792 /
/// EnergyShieldRecoveryCap=11512 / Ward=0 / sharedMindOverMatter=0 /
/// 全类型 EnergyShieldBypass=27 / sharedMoMHitPool=1694 /
/// PhysicalTotalHitPool=17463.86301（=1694+11512/0.73）/
/// ChaosTotalHitPool=9578.931507（chaos 双倍：11512/0.73/2）。
#[test]
fn oracle_lich_eternal_life_pool_chain() {
    // Arrange：bypass 经 build_pool_ctx 聚合（BASE 27），EternalLife 经 keystone 注入。
    let mut db = ModDb::new();
    db.add_list([
        Modifier::flag("EternalLife"),
        Modifier::number("PhysicalEnergyShieldBypass", ModType::Base, 27.0),
        Modifier::number("FireEnergyShieldBypass", ModType::Base, 27.0),
        Modifier::number("ColdEnergyShieldBypass", ModType::Base, 27.0),
        Modifier::number("LightningEnergyShieldBypass", ModType::Base, 27.0),
        Modifier::number("ChaosEnergyShieldBypass", ModType::Base, 27.0),
    ]);
    let cfg = CalcConfig::new();
    let keystones = DefenceKeystones::from_db(&db, &cfg);
    let base = PoolBaseStats {
        max_life: 1694.0,
        life_recoverable: 1694.0,
        mana_unreserved: 792.0,
        energy_shield_recovery_cap: 11512.0,
        ward: 0.0,
    };

    // Act：整备。
    let ctx = build_pool_ctx(&db, &cfg, &keystones, &base);
    let pools = build_pool_state(&db, &cfg, &base);
    let mom = mom_hit_pools(&ctx, &base);

    // Assert：整备中间值对 oracle。
    assert!(ctx.eternal_life);
    assert_eq!(ctx.es_bypass_by_type, [27.0; 5]);
    assert!((mom.shared_hit_pool - 1694.0).abs() < EPS); // 无 MoM → 纯生命池
    assert_eq!(pools.energy_shield, 11512.0);

    // Act：TotalHitPool 基底（EternalLife 分支 :2948-2950）。
    let phys_total = total_hit_pool_base(DamageType::Physical, 1694.0, 11512.0, &ctx);
    let chaos_total = total_hit_pool_base(DamageType::Chaos, 1694.0, 11512.0, &ctx);

    // Assert：oracle PhysicalTotalHitPool=17463.86301 / ChaosTotalHitPool=9578.931507
    // （= 1694 + 11512/0.73[/2]，oracle 输出 5 位小数精度）。
    assert!((phys_total - (1694.0 + 11512.0 / 0.73)).abs() < 1e-9);
    assert!((chaos_total - (1694.0 + 11512.0 / 0.73 / 2.0)).abs() < 1e-9);
    assert!((phys_total - 17463.86301).abs() < 1e-4);
    assert!((chaos_total - 9578.931507).abs() < 1e-4);

    // Act：恰好池量的物理击中 → EternalLife 分支精确清空（bypass 部分被免除）。
    let after = reduce_pools(&pools, &phys(phys_total), &ctx);

    // Assert：ES 实扣 11512、免除 15769.863×0.27 = 4257.863、life 实扣 1694。
    assert!(after.pools.energy_shield.abs() < 1e-6);
    assert!(after.pools.life.abs() < 1e-6);
    assert!(after.overkill.abs() < 1e-6);
    assert!((lost(&after, DamageType::Physical, "energy_shield") - 11512.0).abs() < 1e-6);
    assert!(
        (lost(&after, DamageType::Physical, "eternal_life_prevented") - (11512.0 / 0.73 * 0.27))
            .abs()
            < 1e-6
    );
    assert!((lost(&after, DamageType::Physical, "life") - 1694.0).abs() < 1e-6);
}
