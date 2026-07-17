//! 池整备（M2 Track A，13-G3）——从 ModDb 读出 bypass / MoM / guard / aegis 等，
//! 固化为 [`PoolCtx`] / [`PoolState`]，并产出 MoM/EB 命中池（`<X>MoMHitPool`）。
//!
//! vendor 对照（`vendor/PathOfBuilding-PoE2/src/Modules/CalcDefence.lua`，行号
//! 2026-06-11 核实）：
//! - ES bypass per-type：:2707-2722（`UnblockedDamageDoesBypassES` → 100；否则
//!   Override 或 Σ BASE；clamp 0-100；MinimumBypass = 全类型取 min）；
//! - MoM / EB 池整备：:2726-2820（shared/per-type `DamageTakenFromManaBeforeLife`、
//!   EB 时 ES 按 bypass 嵌套保护 Mana 的 manaProtected 公式、poolProtected 折算）；
//! - Guard：:2821-2856（rate = `min(Σ GuardAbsorbRate, 100)`、池 = calcLib.val
//!   语义的 `GuardAbsorbLimit`）；
//! - Aegis：:2858-2881（`modDB:Max` 取最强单源）；
//! - loss-prevention：:2662-2665（`min(Σ LifeLossPrevented, 100)` 与
//!   `Σ LifeLossBelowHalfPrevented`）。
//!
//! 设计约束（蓝图 §0 约束 2）：整备与求值分离——本文件是**唯一**读 ModDb 的一侧，
//! `pool_damage.rs` 状态机只消费固化后的值类型；本文件同样不写 `Env`。

use crate::calc::pool_damage::{
    AllyLayer, PoolCtx, PoolState, apply_protected_layer, life_hit_pool_with_loss_prevention,
    pool_protected,
};
use crate::rules::DefenceKeystones;
use crate::{CalcConfig, ModDb};
use pobr_data::constants::DamageType;
use pobr_data::prelude::*;

/// vendor 类型名（per-type ModName 前缀），下标 = [`DamageType`] 枚举序。
const TYPE_NAMES: [&str; 5] = ["Physical", "Fire", "Cold", "Lightning", "Chaos"];

/// 池整备的「output 侧」输入（由 perform/Track F 从既有 OutputTable 口径提供，
/// 本模块不重复计算 Life/Mana/ES 本值）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PoolBaseStats {
    /// 最大生命（vendor `output.Life`，half-life 基准）。
    pub max_life: f64,
    /// 可恢复生命（vendor `output.LifeRecoverable`，EHP 生命池口径）。
    pub life_recoverable: f64,
    /// 未预留法力（vendor `output.ManaUnreserved`）。
    pub mana_unreserved: f64,
    /// ES 恢复上限（vendor `output.EnergyShieldRecoveryCap`）。
    pub energy_shield_recovery_cap: f64,
    /// 结界（vendor `output.Ward`）。
    pub ward: f64,
}

/// 构造扣池上下文（CalcDefence.lua:2707-2722 / :2728 / :2773 / :2662-2665 / :572）。
///
/// keystone flag（EternalLife / EB / WardNotBreak）经 [`DefenceKeystones`] 注入
/// （蓝图 §3.3 契约 2——不散读 keystone flag）；`ChaosNotDoubleESDamage` 非注册表
/// 字段（非 keystone 开关），按 vendor :582 直接读 flag。
pub fn build_pool_ctx(
    db: &ModDb,
    cfg: &CalcConfig,
    keystones: &DefenceKeystones,
    base: &PoolBaseStats,
) -> PoolCtx {
    // ES bypass per-type（:2710-2721）：UnblockedDamageDoesBypassES → 全类型 100；
    // 否则 Override 优先、Σ BASE 兜底；clamp 0-100。
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
    // MoM shared（:2728）与 per-type（:2773，cap 于 100−shared）。
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
        // :572 `Σ BASE WardBypass`（vendor 不 clamp，保持原值）。
        ward_bypass: db.sum(ModType::Base, cfg, &[ModName::from("WardBypass")]),
        eternal_life: keystones.eternal_life,
        eb: keystones.energy_shield_protects_mana,
        chaos_not_double_es: db.flag(cfg, ModName::from("ChaosNotDoubleESDamage")),
        ward_not_break: keystones.ward_not_break,
        // :2662 `min(Σ LifeLossPrevented, 100)`；:2664 belowHalf 原始 BASE 和。
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

/// 构造扣池前的池快照（CalcDefence.lua:2821-2881 + reducePoolsByDamage 入口
/// :490-516 的 output 读出口径）。
///
/// allies 段目前只接 companion 层（`TakenFromCompanionBeforeYou` +
/// `TotalCompanionLife`，#12）；frost shield / spectre / totem / soul link 等
/// 其余先扣层无来源时 vendor 同样为空，结构保留给后续接入。
pub fn build_pool_state(db: &ModDb, cfg: &CalcConfig, base: &PoolBaseStats) -> PoolState {
    // Aegis（:2860-2877）：modDB:Max 取最强单源（非求和）。
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
    // Guard（:2823-2845）：rate = min(Σ BASE, 100)；池 = calcLib.val(GuardAbsorbLimit)
    // （rate > 0 才求值，vendor 仅在该分支写 output）。
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
    // 伴侣先扣层（#12；CalcDefence.lua:2961-2965 整备 + :493-495/:3087-3088 EHP
    // 表 + :3656-3663 max-hit）：`TakenFromCompanionBeforeYou` ≠ 0 时取
    // `TotalCompanionLife`（config Override 优先，否则 perform 注入的 BASE 和）。
    // deflected 项（`TakenFromCompanionBeforeYouFromDeflected × DeflectChance`，
    // :2962-2963）未建模——18-build 语料无来源（wolf-pack oracle 钉值 mitigation
    // 恰为 10 = 纯 hits 项），接入时需在此叠加。
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

/// vendor `calcLib.val`（CalcTools.lua:32-39）等价：`Σ BASE × (1 + Σ INC/100) × Π more`
/// （BASE 为 0 时直接 0，不施放乘区）。
fn calc_val(db: &ModDb, cfg: &CalcConfig, name: &str) -> f64 {
    let names = [ModName::from(name)];
    let base = db.sum(ModType::Base, cfg, &names);
    if base == 0.0 {
        return 0.0;
    }
    base * (1.0 + db.sum(ModType::Inc, cfg, &names) / 100.0) * db.more(cfg, &names)
}

/// 全类型最小 ES bypass（vendor `output.MinimumBypass`，:2709/:2721——shared MoM 的
/// EB 嵌套用它）。
pub fn minimum_es_bypass(ctx: &PoolCtx) -> f64 {
    ctx.es_bypass_by_type
        .iter()
        .copied()
        .fold(100.0_f64, f64::min)
}

/// MoM / EB 命中池整备产物（vendor `sharedManaEffectiveLife` / `sharedMoMHitPool` /
/// `<X>ManaEffectiveLife` / `<X>MoMHitPool`，:2726-2820）。
///
/// `hit_pool_by_type` 即 max-hit TotalHitPool 的 MoM 基底（:2945 起点）；
/// `effective_life_by_type` 为 TotalPool（panel 口径）基底。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MomHitPools {
    pub shared_effective_life: f64,
    pub shared_hit_pool: f64,
    pub effective_life_by_type: [f64; 5],
    pub hit_pool_by_type: [f64; 5],
}

/// MoM / EB 池整备（CalcDefence.lua:2726-2820 逐行）。
///
/// - shared 段（:2728-2771）：MoM>0 时 sourcePool = `max(ManaUnreserved, 0)`；EB 且
///   MinimumBypass<100 时 mana 先被 ES 嵌套保护（bypass>0 走 :2738-2740 manaProtected
///   公式，bypass=0 直接 +EScap）；再按 poolProtected 折进生命池。
/// - per-type 段（:2772-2819）：`<X>MindOverMatter > 0` 或「该类型 bypass 高于
///   MinimumBypass 且 shared MoM > 0」时独立重算（EB 的 manaProtected 用 :2785 的
///   per-type 公式），否则回落 shared 值。
pub fn mom_hit_pools(ctx: &PoolCtx, base: &PoolBaseStats) -> MomHitPools {
    // :2674 LifeHitPool（整备期以满 recoverable 生命求值）。
    let life_hit_pool = life_hit_pool_with_loss_prevention(
        base.life_recoverable,
        base.max_life,
        ctx.prevented_life_loss,
        ctx.life_loss_below_half_prevented,
    );
    let minimum_bypass = minimum_es_bypass(ctx);

    // ---- shared 段（:2728-2771）----
    let (shared_effective_life, shared_hit_pool) = if ctx.mom_shared > 0.0 {
        let mut source_pool = base.mana_unreserved.max(0.0);
        let mut source_hit_pool = source_pool;
        let es_bypass = minimum_bypass / 100.0;
        if ctx.eb && es_bypass < 1.0 {
            if es_bypass > 0.0 {
                // :2738 manaProtected = min(mana/bypass − mana, EScap)。
                let mana_protected =
                    (source_pool / es_bypass - source_pool).min(base.energy_shield_recovery_cap);
                // :2739-2740（注意下界是 −life：mana 不足时 ES 直接顶到生命侧）。
                source_pool = (source_pool - mana_protected).max(-base.life_recoverable)
                    + (source_pool + base.life_recoverable).min(mana_protected) / es_bypass;
                source_hit_pool = (source_hit_pool - mana_protected).max(-life_hit_pool)
                    + (source_hit_pool + life_hit_pool).min(mana_protected) / es_bypass;
            } else {
                // :2742-2743 bypass=0：ES 全额并入 mana 池。
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
        // :2768-2770 无 shared MoM：直接生命池。
        (base.life_recoverable, life_hit_pool)
    };

    // ---- per-type 段（:2772-2819）----
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
        // :2774 独立重算条件。
        if mom_type > 0.0 || (bypass_pct > minimum_bypass && ctx.mom_shared > 0.0) {
            let mind_over_matter = mom_type + ctx.mom_shared;
            let mut source_pool = base.mana_unreserved.max(0.0);
            let mut source_hit_pool = source_pool;
            if ctx.eb && bypass_pct < 100.0 {
                if bypass_pct > 0.0 {
                    let es_bypass = bypass_pct / 100.0;
                    // :2785 per-type manaProtected = EScap/(1−bypass)×bypass
                    // （与 shared 段 :2738 的公式不同，按 vendor 原样保留）。
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
            // :2816-2817 回落 shared。
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

/// MoM 折算公共尾段（:2746-2755 / :2793-2802）：poolProtected 后并入生命池；
/// MoM ≥ 100 时直接平铺相加。返回 (effective_life, hit_pool)。
fn mom_fold(
    mind_over_matter_pct: f64,
    source_pool: f64,
    source_hit_pool: f64,
    life_recoverable: f64,
    life_hit_pool: f64,
) -> (f64, f64) {
    if mind_over_matter_pct >= 100.0 {
        // :2748-2751 / :2795-2798。
        (
            life_recoverable + source_pool,
            life_hit_pool + source_hit_pool,
        )
    } else {
        let rate = mind_over_matter_pct / 100.0;
        let protected = pool_protected(source_pool, rate);
        let hit_protected = pool_protected(source_hit_pool, rate);
        // :2753-2754 / :2800-2801。
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

    /// 空 ModDb → 中性 ctx（bypass/MoM/loss-prevention 全 0，flag 全关）。
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

    /// ES bypass 聚合（CalcDefence.lua:2715）：Override 优先于 Σ BASE；clamp 0-100
    /// （:2720）；`UnblockedDamageDoesBypassES` flag → 全类型 100（:2711-2712）。
    #[test]
    fn build_pool_ctx_es_bypass_override_and_clamp() {
        // Arrange：Fire BASE 30+20=50；Cold Override 80 胜过 BASE 10；Chaos BASE 150 → clamp 100。
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

        // UnblockedDamageDoesBypassES → 全类型 100。
        let mut db2 = ModDb::new();
        db2.add_list([Modifier::flag("UnblockedDamageDoesBypassES")]);
        let ctx2 = build_pool_ctx(&db2, &cfg, &ks, &base_stats());
        assert_eq!(ctx2.es_bypass_by_type, [100.0; 5]);
    }

    /// MoM 聚合（:2728/:2773）：shared cap 100；per-type cap 于 100−shared。
    #[test]
    fn build_pool_ctx_mom_caps() {
        // Arrange：shared 60+70=130 → 100 后 per-type cap 0；shared 30 时 Fire 90 → cap 70。
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

    /// keystone 注入（蓝图 §3.3 契约 2）+ ChaosNotDoubleESDamage 直读 +
    /// loss-prevention 聚合（:2662 cap 100 / :2664 原始和）。
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
        assert_eq!(ctx.prevented_life_loss, 100.0); // cap 100
        assert_eq!(ctx.life_loss_below_half_prevented, 50.0);
    }

    /// 池快照整备：aegis 取最强单源（modDB:Max，:2860/:2870）、guard 的
    /// calcLib.val 语义（BASE×(1+INC/100)×more，CalcTools.lua:32-39）、
    /// base 数值透传。
    #[test]
    fn build_pool_state_aegis_max_and_guard_val() {
        // Arrange：sharedAegis 两源 400/700 → 取 700；guard rate 25+10=35、
        // limit 200 × (1+50/100) = 300。
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
        // rate=0 的 per-type guard 不求值（vendor 仅 rate>0 写 output）。
        assert_eq!(state.guard_by_type, [0.0; 5]);
        // base 透传。
        assert_eq!(state.life, 1000.0);
        assert_eq!(state.mana, 500.0);
        assert_eq!(state.energy_shield, 600.0);
    }

    /// （#12）companion 先扣层整备（:2961-2965）：mitigation ≠ 0 且生命 > 0 时
    /// 产出 allies 层；Override 优先于 BASE 求和（config `TotalCompanionLife`
    /// 覆盖通道）；mitigation = 0 或生命 = 0 → allies 空。
    #[test]
    fn build_pool_state_companion_ally_layer() {
        let cfg = CalcConfig::new();
        // 无 mitigation → 空。
        assert!(
            build_pool_state(&ModDb::new(), &cfg, &base_stats())
                .allies
                .is_empty()
        );
        // wolf-pack 钉值形态：10% + 2262（oracle TotalCompanionLife）。
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
        // Override 优先。
        let mut db2 = ModDb::new();
        db2.add_list([
            Modifier::number("TakenFromCompanionBeforeYou", ModType::Base, 10.0),
            Modifier::number("TotalCompanionLife", ModType::Base, 2262.0),
            Modifier::number("TotalCompanionLife", ModType::Override, 500.0),
        ]);
        let state2 = build_pool_state(&db2, &cfg, &base_stats());
        assert_eq!(state2.allies[0].remaining, 500.0);
        // mitigation 有、生命 0 → 空（vendor :3656 `TotalCompanionLife > 0` 门控）。
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

    /// MinimumBypass = 全类型 min（:2709-2721）。
    #[test]
    fn minimum_es_bypass_takes_min() {
        let ctx = PoolCtx {
            es_bypass_by_type: [27.0, 27.0, 30.0, 27.0, 100.0],
            ..Default::default()
        };
        assert_eq!(minimum_es_bypass(&ctx), 27.0);
        assert_eq!(minimum_es_bypass(&PoolCtx::default()), 0.0);
    }

    /// MoM 30% 共享、无 EB（:2746-2754）：手算
    /// sourcePool = 500；protected = 500/0.3×0.7 = 1166.67；
    /// effective = max(1000−1166.67, 0) + min(1000, 1166.67)/0.7 = 1428.57…。
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
        // 无 per-type MoM → 全类型回落 shared（:2816-2817）。
        for idx in 0..5 {
            assert!((pools.hit_pool_by_type[idx] - expected).abs() < 1e-9);
        }
    }

    /// MoM ≥ 100（:2748-2751）：直接 life + mana 平铺相加（mana 不足以「保护」
    /// 比例，全池吃完为止）。手算 1000 + 500 = 1500。
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

    /// EB + bypass 嵌套（:2735-2740 + :2746-2754）：手算
    /// mana=1000, ES=600, life=1000, bypass=30, MoM=50：
    /// manaProtected = min(1000/0.3 − 1000, 600) = 600；
    /// sourceHitPool = max(1000−600, −1000) + min(1000+1000, 600)/0.3 = 400 + 2000 = 2400；
    /// hitPoolProtected = 2400/0.5×0.5 = 2400；
    /// sharedMoMHitPool = max(1000−2400, 0) + min(1000, 2400)/0.5 = 2000。
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

    /// EB + bypass=0（:2742-2743）：ES 全额并入 mana 池后再 MoM 折算。
    /// 手算：sourcePool = 500+600 = 1100；MoM 100 → hitPool = 1000+1100 = 2100。
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

    /// per-type 独立重算条件（:2774）：Fire 30% 单类型 MoM → 仅 Fire 偏离 shared。
    /// 手算 Fire：protected = 500/0.3×0.7 = 1166.67 → hitPool = 1000/0.7 = 1428.57。
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
        // 其余类型 = shared = 纯生命池。
        assert_eq!(
            pools.hit_pool_by_type[DamageType::Physical as usize],
            1000.0
        );
        assert_eq!(pools.shared_hit_pool, 1000.0);
    }
}
