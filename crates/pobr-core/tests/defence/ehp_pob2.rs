//! (F-1): integration tests for the new PoB2-parity EHP pipeline.
//!
//! Vendor reference: `vendor/PathOfBuilding-PoE2/src/Modules/CalcDefence.lua`
//! - `numberOfHitsToDie`: :2979-3145 (loop pool drain + recursive acceleration +
//!   overkill fractional fold);
//! - enemy incoming damage placeholder: ConfigOptions.lua:1982-1996;
//! - new max hit: :3540-3697 (TotalHitPool pool-expansion layer + quadratic solve +
//!   smoothing);
//! - not-hit layer: :2015-2026; mitigation layer: :3155-3247; TotalEHP: :3322.
//!
//! After the F-3 parity switchover: canonical `total_ehp`/`*_max_hit` hold the new
//! parity values (`*_pob2` is a same-value alias); the old lowest-max-hit figure is
//! kept in `total_ehp_lowest_max_hit`.

use pobr_core::calc::actor::{Actor, ActorBaseStats};
use pobr_core::calc::env::Env;
use pobr_core::calc::perform::perform;
use pobr_core::calc::setup_env::setup_enemy;
use pobr_core::calc::{
    EhpLoopParams, GainWhenHit, MaxHitInputs, MitigationCtx, PoolCtx, PoolState, TypedDamage,
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

// Game stat reserve_life_instead_of_loss_from_damage_for_x_ms, current
// Widow's Reign: damage reserves Life until 3 seconds without damage to Life.
// No test below advances that timer or automatically disables a skill.
fn widow_mods() -> Vec<Modifier> {
    let mods =
        crate::support::parse_mod("Life that would be lost by taking Damage is instead Reserved")
            .unwrap()
            .mods;
    assert!(
        mods.iter()
            .any(|m| m.name.as_str() == "DamageInsteadReservesLife")
    );
    mods
}

fn widow_pools(life: f64) -> (PoolState, PoolCtx) {
    let db = {
        let mut db = pobr_core::ModDb::new();
        db.add_list(widow_mods());
        db
    };
    let cfg = CalcConfig::default();
    let base = pobr_core::calc::PoolBaseStats {
        max_life: life,
        life_recoverable: life,
        ..Default::default()
    };
    let ctx = pobr_core::calc::build_pool_ctx(
        &db,
        &cfg,
        &pobr_core::rules::DefenceKeystones::default(),
        &base,
    );
    assert!(ctx.damage_instead_reserves_life);
    (
        pobr_core::calc::build_pool_state(&db, &cfg, &base, 0.0),
        ctx,
    )
}

#[test]
fn widow_damage_reservation_blocks_recovery_across_serial_hits() {
    let (pools, ctx) = widow_pools(1000.0);
    let hit = TypedDamage {
        physical: 250.0,
        ..Default::default()
    };
    let mut after = reduce_pools(&pools, &hit, &ctx);
    assert_eq!(after.pools.life, 750.0);
    assert_eq!(after.pools.life_reserved_by_damage, 250.0);
    assert_eq!(after.pools.life_recovery_cap(1000.0), 750.0);
    assert!(
        after
            .resources_lost
            .contains(&(DamageType::Physical, "life_reserved", 250.0))
    );
    assert!(
        !after
            .resources_lost
            .iter()
            .any(|(_, layer, _)| *layer == "life")
    );
    after.pools.recover_life(1000.0, 1000.0);
    assert_eq!(after.pools.life, 750.0);
    let next = reduce_pools(&after.pools, &hit, &ctx);
    assert_eq!(next.pools.life, 500.0);
    assert_eq!(next.pools.life_reserved_by_damage, 500.0);
    let quiet_snapshot = reduce_pools(&next.pools, &TypedDamage::default(), &ctx);
    assert_eq!(
        quiet_snapshot.pools, next.pools,
        "no hit does not imply elapsed expiry"
    );

    let params = EhpLoopParams {
        gain: GainWhenHit {
            life: 100.0,
            ..Default::default()
        },
        ..default_params()
    };
    assert_eq!(number_of_hits_to_die(&hit, &pools, &ctx, &params), 4.0);
    assert_eq!(
        number_of_hits_to_die(
            &TypedDamage {
                physical: 1.0,
                ..Default::default()
            },
            &pools,
            &ctx,
            &params,
        ),
        1000.0,
        "accelerated recovery must not expand reservation capacity"
    );
    let normal = PoolCtx {
        damage_instead_reserves_life: false,
        ..ctx
    };
    assert_eq!(number_of_hits_to_die(&hit, &pools, &normal, &params), 6.0);
    let normal_hit = reduce_pools(&pools, &hit, &normal);
    assert_eq!(normal_hit.pools.life_reserved_by_damage, 0.0);
    assert!(
        normal_hit
            .resources_lost
            .contains(&(DamageType::Physical, "life", 250.0))
    );
    assert_eq!(
        number_of_hits_to_die(&hit, &pools, &normal, &default_params()),
        4.0
    );
}

#[test]
fn widow_perform_ehp_does_not_count_block_recovery_as_released_reservation() {
    let run = |equipped: bool, recovery: f64| {
        let mut mods = if equipped { widow_mods() } else { vec![] };
        mods.extend([
            Modifier::number("ShieldBlockChance", ModType::Base, 50.0),
            Modifier::number("LifeOnBlock", ModType::Base, recovery),
        ]);
        let mut env = player_with(
            ActorBaseStats {
                life: 1000.0,
                ..Default::default()
            },
            mods,
        );
        env.enemy.mod_db.add_list([
            Modifier::number("EnemyPhysicalDamage", ModType::Base, 250.0),
            Modifier::flag("NeverCrit"),
        ]);
        perform(&mut env).unwrap();
        env.player.output
    };
    let ordinary = run(false, 100.0);
    let widow = run(true, 100.0);
    let no_recovery = run(true, 0.0);
    assert_eq!(widow.life_recoverable, ordinary.life_recoverable);
    assert_eq!(widow.physical_max_hit, ordinary.physical_max_hit);
    assert!(widow.total_ehp > 0.0);
    assert!(ordinary.total_ehp > widow.total_ehp);
    assert_eq!(widow.total_ehp, no_recovery.total_ehp);
    assert_eq!(
        widow.number_of_mitigated_hits,
        no_recovery.number_of_mitigated_hits
    );
}

#[test]
fn widow_reservation_is_distinct_from_ordinary_missing_life_and_death() {
    let (mut pools, ctx) = widow_pools(1000.0);
    pools.life = 400.0; // Life previously spent on costs, not damage reservation.
    pools.life_reserved_by_damage = 200.0;
    let hit = TypedDamage {
        physical: 300.0,
        ..Default::default()
    };
    let mut after = reduce_pools(&pools, &hit, &ctx);
    assert_eq!(after.pools.life_reserved_by_damage, 500.0);
    assert_eq!(
        after.pools.life, 400.0,
        "reservation only clamps current Life"
    );
    after.pools.recover_life(200.0, 1000.0);
    assert_eq!(
        after.pools.life, 500.0,
        "ordinary missing Life remains recoverable"
    );
    let mut lethal = reduce_pools(
        &after.pools,
        &TypedDamage {
            physical: 600.0,
            ..Default::default()
        },
        &ctx,
    );
    assert_eq!(lethal.pools.life, 0.0);
    assert_eq!(lethal.pools.life_reserved_by_damage, 1000.0);
    assert_eq!(lethal.overkill, 100.0);
    lethal.pools.recover_life(1000.0, 1000.0);
    assert_eq!(
        lethal.pools.life, 0.0,
        "recovery cannot revive a fully reserved pool"
    );
}

#[test]
fn widow_keeps_skill_and_permanent_reservations_and_maximum_life_distinct() {
    for equipped in [false, true] {
        let mut mods = if equipped { widow_mods() } else { vec![] };
        mods.extend([
            Modifier::number("LifeReservedPercent", ModType::Inc, 20.0),
            Modifier::number("LifeReservationEfficiency", ModType::Inc, 100.0),
            Modifier::number("ExtraLifeReserved", ModType::Base, 25.0),
        ]);
        let mut env = player_with(
            ActorBaseStats {
                life: 1000.0,
                ..Default::default()
            },
            mods,
        );
        perform(&mut env).unwrap();
        let out = &env.player.output;
        assert_eq!(out.life, 1000.0);
        // Skill reservation = 200/2; permanent reservation = 250, unscaled.
        assert_eq!(out.life_reserved, 350.0);
        assert_eq!(out.life_unreserved, 650.0);
        assert_eq!(out.life_recoverable, 650.0);
        assert_eq!(out.physical_max_hit, 650.0);
        assert_eq!(out.ehp_terms.as_ref().unwrap().total_pool[0], 650.0);
        assert_eq!(out.limit_views.life_permanent_reserved, 25.0);
        assert_eq!(out.limit_views.life_cancellable_reservation, 75.0);

        if equipped {
            let (mut pools, mut ctx) = widow_pools(out.life);
            ctx.life_unreserved = out.life_unreserved;
            pools.life = out.life_recoverable;
            let after = reduce_pools(
                &pools,
                &TypedDamage {
                    physical: 200.0,
                    ..Default::default()
                },
                &ctx,
            );
            assert_eq!(
                after.pools.life_reserved_by_damage, 200.0,
                "efficiency is skill-only"
            );
            assert_eq!(after.pools.life_recovery_cap(out.life_unreserved), 450.0);
        }
    }
}

#[test]
fn widow_reserves_only_damage_after_energy_shield_and_mind_over_matter() {
    let (mut pools, mut ctx) = widow_pools(1000.0);
    pools.energy_shield = 300.0;
    pools.mana = 200.0;
    ctx.mom_shared = 50.0;
    let after = reduce_pools(
        &pools,
        &TypedDamage {
            physical: 500.0,
            ..Default::default()
        },
        &ctx,
    );
    assert_eq!(after.pools.energy_shield, 0.0);
    assert_eq!(after.pools.mana, 100.0);
    assert_eq!(after.pools.life, 900.0);
    assert_eq!(after.pools.life_reserved_by_damage, 100.0);
    let next = reduce_pools(
        &after.pools,
        &TypedDamage {
            physical: 500.0,
            ..Default::default()
        },
        &ctx,
    );
    assert_eq!(next.pools.mana, 0.0);
    assert_eq!(next.pools.life, 500.0);
    assert_eq!(next.pools.life_reserved_by_damage, 500.0);

    // PoE2 chaos consumes twice as much ES, rather than bypassing it.
    let chaos = reduce_pools(
        &pools,
        &TypedDamage {
            chaos: 200.0,
            ..Default::default()
        },
        &ctx,
    );
    assert_eq!(chaos.pools.energy_shield, 0.0);
    assert_eq!(chaos.pools.mana, 175.0);
    assert_eq!(chaos.pools.life_reserved_by_damage, 25.0);
}

#[test]
fn widow_chaos_inoculation_and_bypass_do_not_create_extra_life() {
    let mut mods = widow_mods();
    mods.extend(crate::support::parse_mod("Maximum Life is 1").unwrap().mods);
    let mut env = player_with(
        ActorBaseStats {
            life: 1000.0,
            energy_shield: 300.0,
            ..Default::default()
        },
        mods,
    );
    perform(&mut env).unwrap();
    let out = &env.player.output;
    assert_eq!(out.life, 1.0);
    assert_eq!(out.life_recoverable, 1.0);
    assert_eq!(out.physical_max_hit, 301.0);
    assert!(out.chaos_max_hit.is_infinite());

    let (mut pools, mut ctx) = widow_pools(1.0);
    pools.energy_shield = 300.0;
    let hit = TypedDamage {
        physical: 1.0,
        ..Default::default()
    };
    assert_eq!(
        reduce_pools(&pools, &hit, &ctx)
            .pools
            .life_reserved_by_damage,
        0.0
    );
    ctx.es_bypass_by_type[0] = 100.0;
    let after = reduce_pools(&pools, &hit, &ctx);
    assert_eq!(after.pools.energy_shield, 300.0);
    assert_eq!(after.pools.life_reserved_by_damage, 1.0);
    assert_eq!(after.pools.life, 0.0);
}

#[test]
fn widow_hit_only_defences_do_not_inflate_damage_over_time_pool() {
    let mut mods = widow_mods();
    mods.extend([
        Modifier::number("GuardAbsorbRate", ModType::Base, 100.0),
        Modifier::number("GuardAbsorbLimit", ModType::Base, 400.0),
    ]);
    let mut env = player_with(
        ActorBaseStats {
            life: 1000.0,
            ..Default::default()
        },
        mods,
    );
    perform(&mut env).unwrap();
    let out = &env.player.output;
    assert_eq!(out.physical_max_hit, 1400.0);
    assert_eq!(out.ehp_terms.as_ref().unwrap().total_pool[0], 1000.0);
    let (mut pools, ctx) = widow_pools(1000.0);
    pools.guard_shared = 400.0;
    pools.guard_shared_rate = 100.0;
    let after = reduce_pools(
        &pools,
        &TypedDamage {
            physical: 500.0,
            ..Default::default()
        },
        &ctx,
    );
    assert_eq!(after.pools.life_reserved_by_damage, 100.0);
    assert_eq!(
        after.recoupable_by_type[0], 500.0,
        "reservation still comes from damage taken"
    );
}

/// Naive per-hit path (no recursive acceleration): same semantics as the vendor's main
/// loop but with iterationMultiplier fixed at 1, still applying the overkill fractional
/// fold at the end. Serves as the comparison baseline for accelerated-path equivalence.
fn naive_hits_to_die(damage_in: &TypedDamage, pools_full: &PoolState, ctx: &PoolCtx) -> f64 {
    let per_hit = damage_in.total();
    if per_hit <= 0.0 {
        return f64::INFINITY;
    }
    let mut pool = pools_full.clone();
    let mut hits = 0.0;
    let mut last_overkill = 0.0;
    // The cap is far larger than what ehp_calc_max_iterations covers, ensuring the
    // naive path never gets truncated.
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

/// Recovery on block (vendor CalcDefence.lua:2144-2164 `GainWhenHit`): every survived
/// hit restores `LifeWhenHit`, capped at the full pool. Numbers from the PoB2 oracle
/// on the smith fixture with "Recover 500 Life when you Block" at 20.25% average
/// block: 3090 life, 132.68 per mitigated hit, 101.25 recovered per hit.
/// Hand calc: hit 1 leaves 3058.57 (the cap never binds afterwards), each later
/// hit nets −31.43, hit 95 leaves 104.15, hit 96 kills with 28.53 overkill:
/// 96 − 28.53 / 132.68 = 95.785 (oracle NumberOfMitigatedDamagingHits 95.787).
#[test]
fn hits_to_die_recovers_on_block_between_hits() {
    let pools = PoolState {
        life: 3090.0,
        ..Default::default()
    };
    let ctx = PoolCtx {
        max_life: 3090.0,
        ..Default::default()
    };
    let hit = TypedDamage {
        physical: 132.68,
        ..Default::default()
    };
    let params = EhpLoopParams {
        gain: GainWhenHit {
            life: 101.25,
            ..Default::default()
        },
        ..EhpLoopParams::from_constants(&RuntimeConstants::default(), true)
    };

    let hits = number_of_hits_to_die(&hit, &pools, &ctx, &params);

    assert!((hits - 95.785).abs() < 0.01, "hits = {hits}");
}

// Enemy incoming damage placeholder (ConfigOptions.lua:1982-1996)

/// Hand-computed pinned value: `monsterDamageTable[82] = 353.67` (monster_scaling.json).
/// Pinnacle (DPSMult = 8/4.4): round(353.67 × 1.5 × 8/4.4) = round(964.55) = 965;
/// chaos = round(965 / 2.5) = 386. None (1/4.4): round(120.57) = 121, chaos 48.
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

// numberOfHitsToDie (CalcDefence.lua:2979-3145)

/// Pure life-pool exact division: 1000 life / 100 per hit = 10 hits (overkill 0, no
/// fractional fold).
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

/// Overkill fractional fold (:3133-3135): 1000 life / 300 per hit → 4 hits overflow by
/// 200 → numHits = 4 − 200/300 = 3.3333… (hand-computed).
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

/// Zero incoming damage → ∞ (:2060-2064). Runic Ward is a finite pool spent
/// after life reaches 1 (:659-676), so a hit smaller than the ward is still lethal
/// eventually: 1000 life + 500 ward at 300 per hit dies on the fifth hit.
#[test]
fn hits_to_die_zero_damage_and_ward_pool() {
    // Arrange
    let pools = PoolState {
        life: 1000.0,
        ward: 500.0,
        ..Default::default()
    };
    let ctx = PoolCtx {
        max_life: 1000.0,
        ..Default::default()
    };

    // Act / Assert: zero incoming damage.
    let zero = TypedDamage::default();
    assert!(number_of_hits_to_die(&zero, &pools, &ctx, &default_params()).is_infinite());
    // Act / Assert: ward extends the pool to 1500, 300 per hit → exactly 5 hits.
    let small = TypedDamage {
        fire: 300.0,
        ..Default::default()
    };
    let hits = number_of_hits_to_die(&small, &pools, &ctx, &default_params());
    assert!((hits - 5.0).abs() < 1e-9, "hits = {hits}");
}

#[test]
fn ward_with_life_loss_prevention_absorbs_the_remaining_hit() {
    // Grasping Wounds prevents Life loss, not damage to Runic Ward. The
    // latter absorbs damage after Life reaches 1, including lethal excess.
    let pools = PoolState {
        life: 1000.0,
        ward: 500.0,
        ..Default::default()
    };
    let ctx = PoolCtx {
        max_life: 1000.0,
        prevented_life_loss: 25.0,
        ..Default::default()
    };
    let after = reduce_pools(
        &pools,
        &TypedDamage {
            physical: 1500.0,
            ..Default::default()
        },
        &ctx,
    );
    // 1332 damage costs 999 Life and defers 333; Ward takes the other 168.
    assert_eq!(after.pools.life, 1.0);
    assert_eq!(after.pools.ward, 332.0);
    assert_eq!(after.pools.life_loss_lost_over_time, 333.0);
    assert_eq!(after.overkill, 0.0);

    let mixed = reduce_pools(
        &pools,
        &TypedDamage {
            physical: 1000.0,
            fire: 500.0,
            ..Default::default()
        },
        &ctx,
    );
    assert!((mixed.pools.life - after.pools.life).abs() < 1e-9);
    assert!((mixed.pools.ward - after.pools.ward).abs() < 1e-9);
    assert_eq!(mixed.pools.life_loss_lost_over_time, 333.0);
    assert_eq!(mixed.overkill, 0.0);

    let lethal = reduce_pools(
        &after.pools,
        &TypedDamage {
            physical: 400.0,
            ..Default::default()
        },
        &ctx,
    );
    assert_eq!(lethal.pools.ward, 0.0);
    assert_eq!(lethal.pools.life, 0.0);
    assert!((lethal.overkill - (400.0 - 332.0 - 1.0 / 0.75)).abs() < 1e-9);
    assert!((lethal.pools.life_loss_lost_over_time - 1000.0 / 3.0).abs() < 1e-9);

    let hit = TypedDamage {
        physical: 100.0,
        ..Default::default()
    };
    let params = EhpLoopParams::from_constants(&RuntimeConstants::default(), true);
    let hits = number_of_hits_to_die(&hit, &pools, &ctx, &params);
    assert!(
        (hits - (1000.0 / 0.75 + 500.0) / 100.0).abs() < 1e-9,
        "{hits}"
    );
}

#[test]
fn ward_with_life_loss_prevention_respects_half_life_and_bypass() {
    let pools = PoolState {
        life: 750.0,
        ward: 500.0,
        ..Default::default()
    };
    let ctx = PoolCtx {
        max_life: 1000.0,
        life_loss_below_half_prevented: 50.0,
        ..Default::default()
    };
    let hit = TypedDamage {
        physical: 1500.0,
        ..Default::default()
    };
    let after = reduce_pools(&pools, &hit, &ctx);
    // 250 damage reaches half Life, 998 more reaches 1; Ward takes 252.
    assert_eq!(after.pools.life, 1.0);
    assert_eq!(after.pools.ward, 248.0);
    assert_eq!(after.pools.life_below_half_loss_lost_over_time, 499.0);
    assert_eq!(after.overkill, 0.0);

    let bypass = reduce_pools(
        &pools,
        &hit,
        &PoolCtx {
            ward_bypass: 100.0,
            ..ctx.clone()
        },
    );
    assert_eq!(bypass.pools.life, 0.0);
    assert_eq!(bypass.pools.ward, 500.0);
    assert_eq!(bypass.overkill, 250.0);

    let fractional = reduce_pools(
        &PoolState {
            life: 0.5,
            ward: 100.0,
            ..Default::default()
        },
        &TypedDamage {
            physical: 10.0,
            ..Default::default()
        },
        &ctx,
    );
    assert_eq!(fractional.pools.life, 0.5, "damage must not restore Life");
    assert_eq!(fractional.pools.ward, 90.0);
    assert_eq!(fractional.pools.life_below_half_loss_lost_over_time, 0.0);
}

/// Accelerated path vs. naive per-hit path equivalence (Track F test plan; acceleration
/// is just a step-skipping optimization, so results must match for pool shapes that
/// drain linearly). Covers three pool shapes:
/// (1) life+ES (single damage type, layered linear drain); (2) MoM 30%; (3) guard
/// proportional absorption layer.
#[test]
fn accelerated_path_matches_naive_per_hit_path() {
    let params = default_params();

    // (1) life + ES (single physical type: layered pool with linear drain, step-skip
    // and per-hit values are equal).
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

    // (2) MoM 30% (:602-609).
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

    // (3) shared guard 20% proportional absorption (:563-568).
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

/// Bounds for the step-skip **approximation** with mixed types + double ES drain: the
/// vendor's acceleration merges multiple hits into one big hit (:3105-3119), and chaos's
/// double ES drain (:582) means the "chaos hits ES first within a hit" ordering changes
/// once merged — the vendor itself accepts this approximation (PoB2 parity treats the
/// accelerated value as authoritative). Pins: deviation from the naive path is bounded
/// (<10%), and the accelerated value never overestimates survivability (never exceeds
/// the naive value + ε).
#[test]
fn accelerated_path_bounded_deviation_on_mixed_chaos_es() {
    // Arrange: chaos double ES drain + physical mix (the worst-case shape for
    // step-skipping across the ES→life boundary).
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

    // Assert: hand-computed vendor semantics give fast = 10.9667, naive = 11.7333
    // (deviation ~6.5%).
    assert!(
        (fast - naive).abs() / naive < 0.10,
        "fast {fast} vs naive {naive}"
    );
    assert!(
        fast <= naive + 1e-9,
        "the fast path should not overestimate survivability: {fast} > {naive}"
    );
}

// taken_hit_per_type (panel path :2171-2444)

/// Neutral snapshot identity: identity shift, multiplier 1 → TakenHit = raw incoming
/// damage, panel DR = 0.
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

/// Resistance + armour mitigation (:2402-2409/:2442): fire resist 75%, physical
/// effArmour 2000 vs. 1000 incoming → armour DR = round(2000/(2000+10×1000)×100) =
/// round(16.67) = **17%** (vendor `calcs.armourReduction` rounds, Common.lua
/// round=floor(x+0.5)) → physical taken = 1000 × (1−0.17) = 830; fire taken = 500 × 0.25 = 125.
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

// max_hit_pob2 (:3540-3697)

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

/// Closed-form branch with no armour, full conversion (:3614-3617): pool 1000, fire
/// resist 75% (resMult 0.25) → max hit = 1000 / 0.25 = 4000 (hand-computed).
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

/// Self-consistency of the physical quadratic branch (:3620-3641): the solved RAW,
/// substituted back through the takenHit chain, should exactly equal TotalHitPool
/// (the self-consistency condition given that armour DR varies with hit size; floor
/// introduces <1 error).
#[test]
fn max_hit_quadratic_is_self_consistent_with_taken_hit() {
    // Arrange: effArmour 3000, pool 2000, no flat DR/overwhelm.
    let mut mit = MitigationCtx::default();
    mit.effective_applied_armour[0] = 3000.0;
    let pools = PoolState::default();
    let ctx = PoolCtx::default();
    let inputs = max_hit_inputs(&mit, &pools, &ctx, [2000.0; 5]);

    // Act
    let raw = max_hit_pob2(DamageType::Physical, &inputs, 1.0);

    // Assert: substituting back, taken = RAW × (1 − armour/(armour + 10×RAW)) should
    // ≈ pool (floor tolerance ≤ 2).
    let armour_dr = 3000.0 / (3000.0 + 10.0 * raw);
    let taken_back = raw * (1.0 - armour_dr);
    assert!(
        (taken_back - 2000.0).abs() < 2.0,
        "raw = {raw}, taken_back = {taken_back}"
    );
}

/// Taken multiplier 0 (equivalent to ChaosDamageTaken MORE −100) → immune, max hit = ∞.
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

// not-hit layer (:2015-2026)

/// Four-way NotHit composition: evade 30% + avoidAll 20% →
/// melee NotHit = 100 − 0.7 × 0.8 × 100 = 44% (hand-computed);
/// projectile also multiplies by avoidProj 10% → 100 − 0.7 × 0.8 × 0.9 × 100 = 49.6%.
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

// End to end: perform dual-run invariant + new field outputs

/// End to end (setup_enemy injects the placeholder → perform): all new fields are
/// produced, and the F-3 switchover invariant holds — canonical `total_ehp` ==
/// `total_ehp_pob2`.
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

    // Assert: pool-level fields.
    assert_eq!(out.life_recoverable, 2000.0);
    assert_eq!(out.energy_shield_recovery_cap, 0.0);
    // Enemy incoming damage (Pinnacle@82 placeholder: 965×4 + 386 = 4246, hand-computed
    // from the pinned values above).
    assert_eq!(out.total_enemy_damage_in, 965.0 * 4.0 + 386.0);
    // Hits-to-die / new-parity EHP / new max hit are all produced as positive, finite values.
    assert!(out.number_of_damaging_hits.is_finite() && out.number_of_damaging_hits > 0.0);
    assert!(out.number_of_mitigated_hits >= out.number_of_damaging_hits - 1e-9);
    assert!(out.total_ehp_pob2 > 0.0 && out.total_ehp_pob2.is_finite());
    assert!(out.physical_max_hit_pob2 > 0.0);
    assert!(out.chaos_max_hit_pob2 > 0.0);
    // Panel physical damage reduction (armour 1500 vs. incoming → DR > 0).
    assert!(out.physical_damage_reduction > 0.0);

    // F-3 parity switchover: canonical `total_ehp` = the new parity value
    // (`total_ehp_pob2` remains a same-value alias); the old lowest-max-hit figure
    // is kept in `total_ehp_lowest_max_hit` (not removed).
    assert_eq!(out.total_ehp, out.total_ehp_pob2);
    assert_eq!(out.physical_max_hit, out.physical_max_hit_pob2);
    assert_eq!(out.chaos_max_hit, out.chaos_max_hit_pob2);
    assert!(out.total_ehp > 0.0);
    assert!(out.total_ehp_lowest_max_hit > 0.0);
}

/// Bare Env (no setup_enemy → no incoming damage placeholder): the new pipeline
/// short-circuits neutrally — hits-to-die is ∞, `total_ehp_pob2 = 0`, and the old
/// figures are still produced as usual (backward compatible).
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
    // F-3 parity switchover: canonical total_ehp = the new parity value → 0 (neutral)
    // when there's no incoming damage; the old lowest-max-hit figure is kept in the
    // extra field (backward compatible).
    assert_eq!(out.total_ehp, 0.0);
    assert!(out.total_ehp_lowest_max_hit > 0.0);
    // The new max-hit pipeline is mathematically equivalent to the old self-consistent
    // iterative solve under neutral input (F-2 report §3.1).
    assert_eq!(out.fire_max_hit, 1000.0);
}

/// MoM build end to end: 30% MoM (DamageTakenFromManaBeforeLife BASE 30) should
/// significantly raise hits-to-die and the new-parity EHP (the mana pool is folded in).
#[test]
fn perform_mom_increases_pob2_ehp() {
    // Arrange: same base stats, run once with and once without the MoM mod.
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

    // Assert: MoM raises hits-to-die and the new-parity EHP (PoB2 :602-609 / :2726-2771).
    assert!(
        mom.player.output.number_of_damaging_hits > plain.player.output.number_of_damaging_hits
    );
    assert!(mom.player.output.total_ehp_pob2 > plain.player.output.total_ehp_pob2);
}
