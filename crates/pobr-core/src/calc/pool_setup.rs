//! Pool setup (13-G3) -- reads bypass / MoM / guard / aegis etc. from the
//! ModDb, resolves them into [`PoolCtx`] / [`PoolState`], and produces the
//! MoM/EB hit pools (`<X>MoMHitPool`).
//!
//! Vendor mirror (`vendor/PathOfBuilding-PoE2/src/Modules/CalcDefence.lua`, line numbers verified 2026-06-11):
//! - Per-type ES bypass: :2707-2722 (`UnblockedDamageDoesBypassES` → 100;
//!   otherwise Override or Σ BASE; clamped 0-100; MinimumBypass = the min across all types);
//! - MoM / EB pool setup: :2726-2820 (shared/per-type
//!   `DamageTakenFromManaBeforeLife`; under EB, ES nests to protect Mana per
//!   bypass via the manaProtected formula; poolProtected folding);
//! - Guard: :2821-2856 (rate = `min(Σ GuardAbsorbRate, 100)`, pool =
//!   `GuardAbsorbLimit` under calcLib.val semantics);
//! - Aegis: :2858-2881 (`modDB:Max` takes the single strongest source);
//! - Loss prevention: :2662-2665 (`min(Σ LifeLossPrevented, 100)` and `Σ LifeLossBelowHalfPrevented`).
//!
//! Design constraint: setup is kept separate from evaluation -- this file is
//! the **only** side that reads the ModDb; `pool_damage.rs`'s state machine
//! only consumes the resolved value types; this file likewise never writes to `Env`.

use crate::calc::pool_damage::{
    AllyLayer, PoolCtx, PoolState, apply_protected_layer, life_hit_pool_with_loss_prevention,
    pool_protected,
};
use crate::rules::DefenceKeystones;
use crate::{CalcConfig, ModDb};
use pobr_data::constants::DamageType;
use pobr_data::prelude::*;

/// Vendor's type names (the per-type ModName prefix), indexed by [`DamageType`]'s enum order.
const TYPE_NAMES: [&str; 5] = ["Physical", "Fire", "Cold", "Lightning", "Chaos"];

/// The "output side" inputs for pool setup (supplied by perform/Track F from
/// the existing OutputTable values; this module doesn't recompute the base Life/Mana/ES values).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PoolBaseStats {
    /// Maximum life (vendor `output.Life`, the half-life baseline).
    pub max_life: f64,
    /// Recoverable life (vendor `output.LifeRecoverable`, the EHP life pool value).
    pub life_recoverable: f64,
    /// Unreserved mana (vendor `output.ManaUnreserved`).
    pub mana_unreserved: f64,
    /// ES recovery cap (vendor `output.EnergyShieldRecoveryCap`).
    pub energy_shield_recovery_cap: f64,
    /// Ward (vendor `output.Ward`).
    pub ward: f64,
}

/// Builds the damage-pool-deduction context (CalcDefence.lua:2707-2722 / :2728 / :2773 / :2662-2665 / :572).
///
/// Keystone flags (EternalLife / EB / WardNotBreak) are injected via
/// [`DefenceKeystones`]; `ChaosNotDoubleESDamage` is not a registry field
/// (not a keystone switch), so it reads the flag directly, matching vendor `:582`.
pub fn build_pool_ctx(
    db: &ModDb,
    cfg: &CalcConfig,
    keystones: &DefenceKeystones,
    base: &PoolBaseStats,
) -> PoolCtx {
    // Per-type ES bypass (:2710-2721): UnblockedDamageDoesBypassES → 100 for
    // all types; otherwise Override takes priority, falling back to Σ BASE; clamped 0-100.
    let unblocked_bypass = db.flag(cfg, ModName::from("UnblockedDamageDoesBypassES"));
    let mut es_bypass_by_type = [0.0_f64; 5];
    for (idx, type_name) in TYPE_NAMES.iter().enumerate() {
        let name = ModName::from(format!("{type_name}EnergyShieldBypass"));
        es_bypass_by_type[idx] = if unblocked_bypass {
            100.0
        } else {
            db.override_(cfg, name.clone())
                .unwrap_or_else(|| db.sum(ModType::Base, cfg, &[name]))
        }
        .clamp(0.0, 100.0);
    }
    // MoM shared (:2728) and per-type (:2773, capped at 100−shared).
    let mom_shared = db
        .sum(
            ModType::Base,
            cfg,
            &[ModName::from("DamageTakenFromManaBeforeLife")],
        )
        .min(100.0);
    let mut mom_by_type = [0.0_f64; 5];
    for (idx, type_name) in TYPE_NAMES.iter().enumerate() {
        mom_by_type[idx] = db
            .sum(
                ModType::Base,
                cfg,
                &[ModName::from(format!(
                    "{type_name}DamageTakenFromManaBeforeLife"
                ))],
            )
            .min(100.0 - mom_shared);
    }
    PoolCtx {
        max_life: base.max_life,
        es_bypass_by_type,
        mom_shared,
        mom_by_type,
        // :572 `Σ BASE WardBypass` (vendor doesn't clamp this, kept as the raw value).
        ward_bypass: db.sum(ModType::Base, cfg, &[ModName::from("WardBypass")]),
        eternal_life: keystones.eternal_life,
        eb: keystones.energy_shield_protects_mana,
        chaos_not_double_es: db.flag(cfg, ModName::from("ChaosNotDoubleESDamage")),
        ward_not_break: keystones.ward_not_break,
        // :2662 `min(Σ LifeLossPrevented, 100)`; :2664 belowHalf is the raw BASE sum.
        prevented_life_loss: db
            .sum(ModType::Base, cfg, &[ModName::from("LifeLossPrevented")])
            .min(100.0),
        life_loss_below_half_prevented: db.sum(
            ModType::Base,
            cfg,
            &[ModName::from("LifeLossBelowHalfPrevented")],
        ),
    }
}

/// Builds the pre-deduction pool snapshot (CalcDefence.lua:2821-2881, plus
/// the output-read semantics from reducePoolsByDamage's entry point at :490-516).
///
/// The allies section currently only wires up the companion layer
/// (`TakenFromCompanionBeforeYou` + `TotalCompanionLife`, #12); other
/// before-you layers like frost shield / spectre / totem / soul link have no
/// source in vendor either without them equipped, and the structure is left
/// in place for future additions.
pub fn build_pool_state(db: &ModDb, cfg: &CalcConfig, base: &PoolBaseStats) -> PoolState {
    // Aegis (:2860-2877): modDB:Max takes the single strongest source (not a sum).
    let aegis_shared = db.max_of(ModType::Base, cfg, &[ModName::from("AegisValue")]);
    let aegis_shared_elemental =
        db.max_of(ModType::Base, cfg, &[ModName::from("ElementalAegisValue")]);
    let mut aegis_by_type = [0.0_f64; 5];
    for (idx, type_name) in TYPE_NAMES.iter().enumerate() {
        aegis_by_type[idx] = db.max_of(
            ModType::Base,
            cfg,
            &[ModName::from(format!("{type_name}AegisValue"))],
        );
    }
    // Guard (:2823-2845): rate = min(Σ BASE, 100); pool = calcLib.val(GuardAbsorbLimit)
    // (only evaluated when rate > 0, since vendor only writes output in that branch).
    let guard_shared_rate = db
        .sum(ModType::Base, cfg, &[ModName::from("GuardAbsorbRate")])
        .min(100.0);
    let guard_shared = if guard_shared_rate > 0.0 {
        calc_val(db, cfg, "GuardAbsorbLimit")
    } else {
        0.0
    };
    let mut guard_rate_by_type = [0.0_f64; 5];
    let mut guard_by_type = [0.0_f64; 5];
    for (idx, type_name) in TYPE_NAMES.iter().enumerate() {
        let rate = db
            .sum(
                ModType::Base,
                cfg,
                &[ModName::from(format!("{type_name}GuardAbsorbRate"))],
            )
            .min(100.0);
        guard_rate_by_type[idx] = rate;
        guard_by_type[idx] = if rate > 0.0 {
            calc_val(db, cfg, &format!("{type_name}GuardAbsorbLimit"))
        } else {
            0.0
        };
    }
    // Companion before-you layer (#12; setup at CalcDefence.lua:2961-2965 +
    // :493-495/:3087-3088's EHP table + :3656-3663's max-hit): when
    // `TakenFromCompanionBeforeYou` ≠ 0, takes `TotalCompanionLife` (config
    // Override takes priority, otherwise the BASE sum injected by perform).
    // The deflected term (`TakenFromCompanionBeforeYouFromDeflected ×
    // DeflectChance`, :2962-2963) is not modeled -- the 18-build corpus has
    // no source for it (the wolf-pack oracle's pinned mitigation is exactly
    // 10, i.e. the pure-hits term); add it here when a source shows up.
    let mut allies = Vec::new();
    let companion_rate = db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("TakenFromCompanionBeforeYou")],
    );
    if companion_rate != 0.0 {
        let companion_life_name = ModName::from("TotalCompanionLife");
        let companion_life = db
            .override_(cfg, companion_life_name.clone())
            .unwrap_or_else(|| db.sum(ModType::Base, cfg, &[companion_life_name]));
        if companion_life > 0.0 {
            allies.push(AllyLayer {
                id: "companion",
                remaining: companion_life,
                mitigation_pct: companion_rate,
                damage_type: None,
            });
        }
    }
    PoolState {
        allies,
        aegis_shared,
        aegis_shared_elemental,
        aegis_by_type,
        guard_shared,
        guard_shared_rate,
        guard_by_type,
        guard_rate_by_type,
        ward: base.ward,
        energy_shield: base.energy_shield_recovery_cap,
        mana: base.mana_unreserved,
        life: base.life_recoverable,
        life_loss_lost_over_time: 0.0,
        life_below_half_loss_lost_over_time: 0.0,
    }
}

/// Equivalent to vendor's `calcLib.val` (CalcTools.lua:32-39):
/// `Σ BASE × (1 + Σ INC/100) × Π more` (returns 0 directly when BASE is 0, without applying the factors).
fn calc_val(db: &ModDb, cfg: &CalcConfig, name: &str) -> f64 {
    let names = [ModName::from(name)];
    let base = db.sum(ModType::Base, cfg, &names);
    if base == 0.0 {
        return 0.0;
    }
    base * (1.0 + db.sum(ModType::Inc, cfg, &names) / 100.0) * db.more(cfg, &names)
}

/// Minimum ES bypass across all types (vendor `output.MinimumBypass`,
/// :2709/:2721 -- used by shared MoM's EB nesting).
pub fn minimum_es_bypass(ctx: &PoolCtx) -> f64 {
    ctx.es_bypass_by_type
        .iter()
        .copied()
        .fold(100.0_f64, f64::min)
}

/// The output of MoM / EB hit pool setup (vendor `sharedManaEffectiveLife` /
/// `sharedMoMHitPool` / `<X>ManaEffectiveLife` / `<X>MoMHitPool`, :2726-2820).
///
/// `hit_pool_by_type` is the MoM base for max-hit's TotalHitPool (the
/// starting point at :2945); `effective_life_by_type` is the base for TotalPool (the panel view).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MomHitPools {
    pub shared_effective_life: f64,
    pub shared_hit_pool: f64,
    pub effective_life_by_type: [f64; 5],
    pub hit_pool_by_type: [f64; 5],
}

/// MoM / EB pool setup (line-by-line mirror of CalcDefence.lua:2726-2820).
///
/// - Shared section (:2728-2771): when MoM>0, sourcePool = `max(ManaUnreserved, 0)`;
///   under EB with MinimumBypass<100, mana is first nested-protected by ES
///   (bypass>0 takes the :2738-2740 manaProtected formula, bypass=0 adds the
///   ES cap directly); then folded into the life pool via poolProtected.
/// - Per-type section (:2772-2819): recalculated independently when
///   `<X>MindOverMatter > 0` or "this type's bypass exceeds MinimumBypass and
///   shared MoM > 0" (EB's manaProtected uses the per-type formula at
///   :2785), otherwise falls back to the shared value.
pub fn mom_hit_pools(ctx: &PoolCtx, base: &PoolBaseStats) -> MomHitPools {
    // :2674 LifeHitPool (evaluated with the full recoverable life during setup).
    let life_hit_pool = life_hit_pool_with_loss_prevention(
        base.life_recoverable,
        base.max_life,
        ctx.prevented_life_loss,
        ctx.life_loss_below_half_prevented,
    );
    let minimum_bypass = minimum_es_bypass(ctx);

    // Shared section (:2728-2771)
    let (shared_effective_life, shared_hit_pool) = if ctx.mom_shared > 0.0 {
        let mut source_pool = base.mana_unreserved.max(0.0);
        let mut source_hit_pool = source_pool;
        let es_bypass = minimum_bypass / 100.0;
        if ctx.eb && es_bypass < 1.0 {
            if es_bypass > 0.0 {
                // :2738 manaProtected = min(mana/bypass − mana, EScap).
                let mana_protected =
                    (source_pool / es_bypass - source_pool).min(base.energy_shield_recovery_cap);
                // :2739-2740 (note the lower bound is −life: when mana runs short, ES tops it up straight into the life side).
                source_pool = (source_pool - mana_protected).max(-base.life_recoverable)
                    + (source_pool + base.life_recoverable).min(mana_protected) / es_bypass;
                source_hit_pool = (source_hit_pool - mana_protected).max(-life_hit_pool)
                    + (source_hit_pool + life_hit_pool).min(mana_protected) / es_bypass;
            } else {
                // :2742-2743 bypass=0: ES is merged into the mana pool in full.
                source_pool += base.energy_shield_recovery_cap;
                source_hit_pool = source_pool;
            }
        }
        mom_fold(
            ctx.mom_shared,
            source_pool,
            source_hit_pool,
            base.life_recoverable,
            life_hit_pool,
        )
    } else {
        // :2768-2770 no shared MoM: goes straight to the life pool.
        (base.life_recoverable, life_hit_pool)
    };

    // Per-type section (:2772-2819)
    let mut effective_life_by_type = [0.0_f64; 5];
    let mut hit_pool_by_type = [0.0_f64; 5];
    for dtype in [
        DamageType::Physical,
        DamageType::Fire,
        DamageType::Cold,
        DamageType::Lightning,
        DamageType::Chaos,
    ] {
        let idx = dtype as usize;
        let mom_type = ctx.mom_by_type[idx];
        let bypass_pct = ctx.es_bypass_by_type[idx];
        // :2774 the condition for independent recalculation.
        if mom_type > 0.0 || (bypass_pct > minimum_bypass && ctx.mom_shared > 0.0) {
            let mind_over_matter = mom_type + ctx.mom_shared;
            let mut source_pool = base.mana_unreserved.max(0.0);
            let mut source_hit_pool = source_pool;
            if ctx.eb && bypass_pct < 100.0 {
                if bypass_pct > 0.0 {
                    let es_bypass = bypass_pct / 100.0;
                    // :2785 the per-type manaProtected formula = EScap/(1−bypass)×bypass
                    // (differs from the shared section's formula at :2738; kept verbatim per vendor).
                    let mana_protected =
                        base.energy_shield_recovery_cap / (1.0 - es_bypass) * es_bypass;
                    source_pool = (source_pool - mana_protected).max(-base.life_recoverable)
                        + (source_pool + base.life_recoverable).min(mana_protected) / es_bypass;
                    source_hit_pool = (source_hit_pool - mana_protected).max(-life_hit_pool)
                        + (source_hit_pool + life_hit_pool).min(mana_protected) / es_bypass;
                } else {
                    source_pool += base.energy_shield_recovery_cap;
                    source_hit_pool = source_pool;
                }
            }
            let (eff, hit) = mom_fold(
                mind_over_matter,
                source_pool,
                source_hit_pool,
                base.life_recoverable,
                life_hit_pool,
            );
            effective_life_by_type[idx] = eff;
            hit_pool_by_type[idx] = hit;
        } else {
            // :2816-2817 falls back to shared.
            effective_life_by_type[idx] = shared_effective_life;
            hit_pool_by_type[idx] = shared_hit_pool;
        }
    }
    MomHitPools {
        shared_effective_life,
        shared_hit_pool,
        effective_life_by_type,
        hit_pool_by_type,
    }
}

/// The shared MoM folding tail (:2746-2755 / :2793-2802): folds into the
/// life pool via poolProtected; when MoM ≥ 100, adds the pools flat instead. Returns (effective_life, hit_pool).
fn mom_fold(
    mind_over_matter_pct: f64,
    source_pool: f64,
    source_hit_pool: f64,
    life_recoverable: f64,
    life_hit_pool: f64,
) -> (f64, f64) {
    if mind_over_matter_pct >= 100.0 {
        // :2748-2751 / :2795-2798.
        (
            life_recoverable + source_pool,
            life_hit_pool + source_hit_pool,
        )
    } else {
        let rate = mind_over_matter_pct / 100.0;
        let protected = pool_protected(source_pool, rate);
        let hit_protected = pool_protected(source_hit_pool, rate);
        // :2753-2754 / :2800-2801.
        (
            apply_protected_layer(life_recoverable, protected, 1.0 - rate),
            apply_protected_layer(life_hit_pool, hit_protected, 1.0 - rate),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Modifier;

    fn base_stats() -> PoolBaseStats {
        PoolBaseStats {
            max_life: 1000.0,
            life_recoverable: 1000.0,
            mana_unreserved: 500.0,
            energy_shield_recovery_cap: 600.0,
            ward: 0.0,
        }
    }

    /// Empty ModDb → a neutral ctx (bypass/MoM/loss-prevention all 0, all flags off).
    #[test]
    fn build_pool_ctx_neutral_defaults() {
        // Arrange
        let db = ModDb::new();
        let cfg = CalcConfig::new();
        let ks = DefenceKeystones::default();

        // Act
        let ctx = build_pool_ctx(&db, &cfg, &ks, &base_stats());

        // Assert
        assert_eq!(ctx.es_bypass_by_type, [0.0; 5]);
        assert_eq!(ctx.mom_shared, 0.0);
        assert_eq!(ctx.ward_bypass, 0.0);
        assert!(!ctx.eternal_life && !ctx.eb && !ctx.ward_not_break);
        assert_eq!(ctx.max_life, 1000.0);
    }

    /// ES bypass aggregation (CalcDefence.lua:2715): Override takes priority
    /// over Σ BASE; clamped 0-100 (:2720); the `UnblockedDamageDoesBypassES`
    /// flag → 100 for all types (:2711-2712).
    #[test]
    fn build_pool_ctx_es_bypass_override_and_clamp() {
        // Arrange: Fire BASE 30+20=50; Cold Override 80 wins over BASE 10; Chaos BASE 150 → clamped to 100.
        let mut db = ModDb::new();
        db.add_list([
            Modifier::number("FireEnergyShieldBypass", ModType::Base, 30.0),
            Modifier::number("FireEnergyShieldBypass", ModType::Base, 20.0),
            Modifier::number("ColdEnergyShieldBypass", ModType::Base, 10.0),
            Modifier::number("ColdEnergyShieldBypass", ModType::Override, 80.0),
            Modifier::number("ChaosEnergyShieldBypass", ModType::Base, 150.0),
        ]);
        let cfg = CalcConfig::new();
        let ks = DefenceKeystones::default();

        // Act
        let ctx = build_pool_ctx(&db, &cfg, &ks, &base_stats());

        // Assert
        assert_eq!(ctx.es_bypass_by_type[DamageType::Fire as usize], 50.0);
        assert_eq!(ctx.es_bypass_by_type[DamageType::Cold as usize], 80.0);
        assert_eq!(ctx.es_bypass_by_type[DamageType::Chaos as usize], 100.0);
        assert_eq!(ctx.es_bypass_by_type[DamageType::Physical as usize], 0.0);

        // UnblockedDamageDoesBypassES → 100 for all types.
        let mut db2 = ModDb::new();
        db2.add_list([Modifier::flag("UnblockedDamageDoesBypassES")]);
        let ctx2 = build_pool_ctx(&db2, &cfg, &ks, &base_stats());
        assert_eq!(ctx2.es_bypass_by_type, [100.0; 5]);
    }

    /// MoM aggregation (:2728/:2773): shared capped at 100; per-type capped at 100−shared.
    #[test]
    fn build_pool_ctx_mom_caps() {
        // Arrange: shared 60+70=130 → capped to 100, so per-type cap is 0; shared 30 means Fire 90 → capped to 70.
        let mut db = ModDb::new();
        db.add_list([
            Modifier::number("DamageTakenFromManaBeforeLife", ModType::Base, 30.0),
            Modifier::number("FireDamageTakenFromManaBeforeLife", ModType::Base, 90.0),
        ]);
        let cfg = CalcConfig::new();
        let ks = DefenceKeystones::default();

        // Act
        let ctx = build_pool_ctx(&db, &cfg, &ks, &base_stats());

        // Assert
        assert_eq!(ctx.mom_shared, 30.0);
        assert_eq!(ctx.mom_by_type[DamageType::Fire as usize], 70.0);
    }

    /// Keystone injection + direct-read ChaosNotDoubleESDamage +
    /// loss-prevention aggregation (:2662 capped at 100 / :2664 raw sum).
    #[test]
    fn build_pool_ctx_flags_and_loss_prevention() {
        // Arrange
        let mut db = ModDb::new();
        db.add_list([
            Modifier::flag("ChaosNotDoubleESDamage"),
            Modifier::number("LifeLossPrevented", ModType::Base, 120.0),
            Modifier::number("LifeLossBelowHalfPrevented", ModType::Base, 50.0),
        ]);
        let cfg = CalcConfig::new();
        let ks = DefenceKeystones {
            eternal_life: true,
            energy_shield_protects_mana: true,
            ward_not_break: true,
            ..Default::default()
        };

        // Act
        let ctx = build_pool_ctx(&db, &cfg, &ks, &base_stats());

        // Assert
        assert!(ctx.eternal_life && ctx.eb && ctx.ward_not_break);
        assert!(ctx.chaos_not_double_es);
        assert_eq!(ctx.prevented_life_loss, 100.0); // capped at 100
        assert_eq!(ctx.life_loss_below_half_prevented, 50.0);
    }

    /// Pool snapshot setup: aegis takes the single strongest source
    /// (modDB:Max, :2860/:2870), guard follows calcLib.val semantics
    /// (BASE×(1+INC/100)×more, CalcTools.lua:32-39), base values pass through as-is.
    #[test]
    fn build_pool_state_aegis_max_and_guard_val() {
        // Arrange: sharedAegis has two sources 400/700 → takes 700; guard rate 25+10=35, limit 200 × (1+50/100) = 300.
        let mut db = ModDb::new();
        db.add_list([
            Modifier::number("AegisValue", ModType::Base, 400.0),
            Modifier::number("AegisValue", ModType::Base, 700.0),
            Modifier::number("FireAegisValue", ModType::Base, 250.0),
            Modifier::number("GuardAbsorbRate", ModType::Base, 25.0),
            Modifier::number("GuardAbsorbRate", ModType::Base, 10.0),
            Modifier::number("GuardAbsorbLimit", ModType::Base, 200.0),
            Modifier::number("GuardAbsorbLimit", ModType::Inc, 50.0),
        ]);
        let cfg = CalcConfig::new();

        // Act
        let state = build_pool_state(&db, &cfg, &base_stats());

        // Assert
        assert_eq!(state.aegis_shared, 700.0);
        assert_eq!(state.aegis_by_type[DamageType::Fire as usize], 250.0);
        assert_eq!(state.guard_shared_rate, 35.0);
        assert_eq!(state.guard_shared, 300.0);
        // Per-type guard with rate=0 isn't evaluated (vendor only writes output when rate>0).
        assert_eq!(state.guard_by_type, [0.0; 5]);
        // base values pass through.
        assert_eq!(state.life, 1000.0);
        assert_eq!(state.mana, 500.0);
        assert_eq!(state.energy_shield, 600.0);
    }

    /// (#12) Companion before-you layer setup (:2961-2965): produces an
    /// allies layer when mitigation ≠ 0 and life > 0; Override takes
    /// priority over the BASE sum (the config `TotalCompanionLife` override
    /// channel); mitigation = 0 or life = 0 → allies is empty.
    #[test]
    fn build_pool_state_companion_ally_layer() {
        let cfg = CalcConfig::new();
        // No mitigation → empty.
        assert!(
            build_pool_state(&ModDb::new(), &cfg, &base_stats())
                .allies
                .is_empty()
        );
        // wolf-pack's pinned shape: 10% + 2262 (oracle TotalCompanionLife).
        let mut db = ModDb::new();
        db.add_list([
            Modifier::number("TakenFromCompanionBeforeYou", ModType::Base, 10.0),
            Modifier::number("TotalCompanionLife", ModType::Base, 2262.0),
        ]);
        let state = build_pool_state(&db, &cfg, &base_stats());
        assert_eq!(state.allies.len(), 1);
        assert_eq!(state.allies[0].id, "companion");
        assert_eq!(state.allies[0].remaining, 2262.0);
        assert_eq!(state.allies[0].mitigation_pct, 10.0);
        assert_eq!(state.allies[0].damage_type, None);
        // Override takes priority.
        let mut db2 = ModDb::new();
        db2.add_list([
            Modifier::number("TakenFromCompanionBeforeYou", ModType::Base, 10.0),
            Modifier::number("TotalCompanionLife", ModType::Base, 2262.0),
            Modifier::number("TotalCompanionLife", ModType::Override, 500.0),
        ]);
        let state2 = build_pool_state(&db2, &cfg, &base_stats());
        assert_eq!(state2.allies[0].remaining, 500.0);
        // Mitigation present but life 0 → empty (gated by vendor :3656's `TotalCompanionLife > 0`).
        let mut db3 = ModDb::new();
        db3.add_list([Modifier::number(
            "TakenFromCompanionBeforeYou",
            ModType::Base,
            10.0,
        )]);
        assert!(
            build_pool_state(&db3, &cfg, &base_stats())
                .allies
                .is_empty()
        );
    }

    /// MinimumBypass = the min across all types (:2709-2721).
    #[test]
    fn minimum_es_bypass_takes_min() {
        let ctx = PoolCtx {
            es_bypass_by_type: [27.0, 27.0, 30.0, 27.0, 100.0],
            ..Default::default()
        };
        assert_eq!(minimum_es_bypass(&ctx), 27.0);
        assert_eq!(minimum_es_bypass(&PoolCtx::default()), 0.0);
    }

    /// MoM 30% shared, no EB (:2746-2754): hand-computed
    /// sourcePool = 500; protected = 500/0.3×0.7 = 1166.67;
    /// effective = max(1000−1166.67, 0) + min(1000, 1166.67)/0.7 = 1428.57….
    #[test]
    fn mom_hit_pools_shared_thirty_percent() {
        // Arrange
        let ctx = PoolCtx {
            max_life: 1000.0,
            mom_shared: 30.0,
            ..Default::default()
        };

        // Act
        let pools = mom_hit_pools(&ctx, &base_stats());

        // Assert
        let expected = 1000.0 / 0.7; // 1428.571…
        assert!((pools.shared_effective_life - expected).abs() < 1e-9);
        assert!((pools.shared_hit_pool - expected).abs() < 1e-9);
        // No per-type MoM → every type falls back to shared (:2816-2817).
        for idx in 0..5 {
            assert!((pools.hit_pool_by_type[idx] - expected).abs() < 1e-9);
        }
    }

    /// MoM ≥ 100 (:2748-2751): life + mana are added flat directly (there's
    /// no "protected" ratio for mana, the whole pool is consumed instead).
    /// Hand-computed: 1000 + 500 = 1500.
    #[test]
    fn mom_hit_pools_full_mom_adds_pools() {
        let ctx = PoolCtx {
            max_life: 1000.0,
            mom_shared: 100.0,
            ..Default::default()
        };
        let pools = mom_hit_pools(&ctx, &base_stats());
        assert_eq!(pools.shared_hit_pool, 1500.0);
        assert_eq!(pools.shared_effective_life, 1500.0);
    }

    /// EB + bypass nesting (:2735-2740 + :2746-2754): hand-computed with
    /// mana=1000, ES=600, life=1000, bypass=30, MoM=50:
    /// manaProtected = min(1000/0.3 − 1000, 600) = 600;
    /// sourceHitPool = max(1000−600, −1000) + min(1000+1000, 600)/0.3 = 400 + 2000 = 2400;
    /// hitPoolProtected = 2400/0.5×0.5 = 2400;
    /// sharedMoMHitPool = max(1000−2400, 0) + min(1000, 2400)/0.5 = 2000.
    #[test]
    fn mom_hit_pools_eb_bypass_nesting() {
        // Arrange
        let ctx = PoolCtx {
            max_life: 1000.0,
            mom_shared: 50.0,
            eb: true,
            es_bypass_by_type: [30.0; 5],
            ..Default::default()
        };
        let base = PoolBaseStats {
            max_life: 1000.0,
            life_recoverable: 1000.0,
            mana_unreserved: 1000.0,
            energy_shield_recovery_cap: 600.0,
            ward: 0.0,
        };

        // Act
        let pools = mom_hit_pools(&ctx, &base);

        // Assert
        assert!((pools.shared_hit_pool - 2000.0).abs() < 1e-9);
    }

    /// EB + bypass=0 (:2742-2743): ES is merged into the mana pool in full,
    /// then MoM folding runs. Hand-computed: sourcePool = 500+600 = 1100; MoM 100 → hitPool = 1000+1100 = 2100.
    #[test]
    fn mom_hit_pools_eb_no_bypass_merges_es() {
        let ctx = PoolCtx {
            max_life: 1000.0,
            mom_shared: 100.0,
            eb: true,
            ..Default::default()
        };
        let pools = mom_hit_pools(&ctx, &base_stats());
        assert_eq!(pools.shared_hit_pool, 2100.0);
    }

    /// The per-type independent-recalculation condition (:2774): a 30%
    /// single-type Fire MoM → only Fire deviates from shared.
    /// Hand-computed Fire: protected = 500/0.3×0.7 = 1166.67 → hitPool = 1000/0.7 = 1428.57.
    #[test]
    fn mom_hit_pools_per_type_recalc() {
        // Arrange
        let mut ctx = PoolCtx {
            max_life: 1000.0,
            ..Default::default()
        };
        ctx.mom_by_type[DamageType::Fire as usize] = 30.0;

        // Act
        let pools = mom_hit_pools(&ctx, &base_stats());

        // Assert
        let fire = pools.hit_pool_by_type[DamageType::Fire as usize];
        assert!((fire - 1000.0 / 0.7).abs() < 1e-9);
        // Every other type = shared = the pure life pool.
        assert_eq!(
            pools.hit_pool_by_type[DamageType::Physical as usize],
            1000.0
        );
        assert_eq!(pools.shared_hit_pool, 1000.0);
    }
}
