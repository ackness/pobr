//! Effective crit pipeline (resolve_crit) — mirrors PoB2 `CalcOffence.lua`'s crit section exactly.
//!
//! A single implementation serves both the panel (`calculate_minimal_vs_enemy`)
//! and attribution (`total_dps_traced`) call sites, eliminating logic drift
//! (gap: crit-resolve-refactor). The attribution path reuses `sum_traced` /
//! `more_factor_traced` to write BASE/INC/MORE/enemy contributions into
//! [`TraceGraph`] (gap: crit-traced-inc-more-untraced).
//!
//! Calculation order (must not be reordered; a literal mirror of
//! `CalcOffence.lua` L3681–3838):
//! 1. `crit_chance = (baseCrit + ΣBase + enemy.SelfCritChance) × (1 + (Σinc + enemy.IncSelfCritChance)/100) × Πmore`
//! 2. clamp to `[0, CritChanceCap]` (default 100%, overridable by `Override`/`Sum BASE`) → PreEffectiveCritChance
//! 3. under `mode_effective`, multiply by the hit-chance downgrade (the evasion re-roll)
//! 4. `Flag(CritChanceLucky)` → `1-(1-c)²`
//! 5. `Flag(BifurcateCrit)` → `1-(1-c)²` (records PreBifurcate for the extra crit damage term)
//! 6. `Flag(InevitableCriticalHits)` → sets 100% plus a geometric-series less-damage penalty
//! 7. crit damage: `NoCritMultiplier` → 1.0; otherwise
//!    `extra = (BASE_BONUS + ΣBase)/100 × (1+Σinc/100) × Πmore`,
//!    + an extra share for bifurcate's "both hits crit", + enemy SelfCritMultiplier,
//!    + the inevitable less-damage term, finally `1 + max(0, extra)`
//! 8. `crit_effect = (1-c) + c × crit_mult`
//!
//! Sources: agent-docs/critical-hits.md §PoB2 calculation implementation;
//!       devs/docs/architecture/12-combat-mechanics-architecture.md §4.3;
//!       PathOfBuilding-PoE2 `src/Modules/CalcOffence.lua`.

use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb, TraceGraph, TraceNodeId, TraceOperation};

use super::offence::more_factor_traced;
use super::round;

/// Default crit chance cap (percentage points). PoB2's `CritChanceCap` defaults to 100, overridable.
const DEFAULT_CRIT_CHANCE_CAP: f64 = 100.0;

/// Result of the effective crit pipeline. `chance` is the final crit chance
/// (after downgrade/lucky/bifurcate/inevitable, as a **fraction**, 0..1);
/// `multiplier` is the crit damage multiplier (e.g. 2.0 = crits deal double
/// damage); `effect` is the average crit effect `(1-c) + c×mult`, multiplied
/// directly onto the average hit. `pre_effective_chance` is the chance
/// (fraction) before the hit-chance downgrade but after the cap, used to
/// display overflow in breakdowns.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CritOutcome {
    pub chance: f64,
    pub multiplier: f64,
    pub effect: f64,
    pub pre_effective_chance: f64,
}

/// Pure numeric resolution for the crit pipeline (fraction in/out).
///
/// `hit_chance` is the hit chance (fraction 0..1, only used for the downgrade
/// under `mode_effective`). `base_crit` is the weapon/gem base crit chance
/// (fraction, e.g. 0.05 for unarmed).
///
/// Uses both the player and enemy modDBs: enemy `SelfCritChance`/
/// `SelfCritMultiplier` (crit weakness/marks) only apply under
/// `mode_effective` (matching PoB2).
pub fn resolve_crit(
    player: &ModDb,
    enemy: &ModDb,
    cfg: &CalcConfig,
    hit_chance: f64,
    base_crit: f64,
    mode_effective: bool,
) -> CritOutcome {
    resolve_crit_impl(
        player,
        enemy,
        cfg,
        hit_chance,
        base_crit,
        mode_effective,
        None,
    )
}

/// Numerically identical to [`resolve_crit`], but also writes BASE/INC/MORE/
/// enemy contributions into `trace`, returning `(outcome, crit_node)`:
/// `crit_node` is the [`TraceOperation::Chance`] node carrying `crit_effect`,
/// which the caller can `add_edge` onward to downstream DPS nodes.
pub fn resolve_crit_traced(
    player: &ModDb,
    enemy: &ModDb,
    cfg: &CalcConfig,
    hit_chance: f64,
    base_crit: f64,
    mode_effective: bool,
    trace: &mut TraceGraph,
) -> (CritOutcome, TraceNodeId) {
    let mut sink = TraceSink {
        trace,
        node_id: None,
    };
    let outcome = resolve_crit_impl(
        player,
        enemy,
        cfg,
        hit_chance,
        base_crit,
        mode_effective,
        Some(&mut sink),
    );
    let crit_node = sink
        .node_id
        .expect("resolve_crit_impl always sets the crit_effect node when tracing");
    (outcome, crit_node)
}

/// Trace output sink: the implementation connects every contribution edge
/// here, and at the end sets `node_id` to the node carrying `crit_effect`.
struct TraceSink<'a> {
    trace: &'a mut TraceGraph,
    node_id: Option<TraceNodeId>,
}

#[allow(clippy::too_many_arguments)]
fn resolve_crit_impl(
    player: &ModDb,
    enemy: &ModDb,
    cfg: &CalcConfig,
    hit_chance: f64,
    base_crit: f64,
    mode_effective: bool,
    sink: Option<&mut TraceSink<'_>>,
) -> CritOutcome {
    let crit_chance_names = [ModName::from("CriticalStrikeChance")];
    let self_crit_chance = [ModName::from("SelfCritChance")];

    // 0) Base crit from the underlying source (vendor CalcOffence.lua:3665-3676 baseCrit section)
    // vendor `baseCrit = critOverride or source.CritChance`: the **source's**
    // base crit (weapon base / spell skillData.critChance) is a separate
    // bucket from the mod bucket `Sum BASE CritChance`.
    // PoBR's equivalent: the source injects `SkillBaseCritChance` BASE via
    // the orchestration layer (distinct from the mod bucket
    // `CriticalStrikeChance`), plus the `base_crit` parameter (fraction, the
    // backward-compatible entry point).
    //
    // `baseCritOverride = Override(cfg, "CritChanceBase")` (:3667-3676) when
    // it hits, **replaces the source base directly** (the mod bucket is
    // unaffected). One source is the Blood Mage ascendancy "Sunder the
    // Flesh" ("Base Critical Hit Chance for Spells is 15%", ModParser.lua:5801 →
    // `mod("CritChanceBase","OVERRIDE",num,SkillType.Spell)`).
    let source_base_pct =
        base_crit * 100.0 + player.sum(ModType::Base, cfg, &[ModName::from("SkillBaseCritChance")]);
    let source_base_pct = player
        .override_(cfg, ModName::from("CritChanceBase"))
        .unwrap_or(source_base_pct);

    // 1) Base sum (including enemy crit weakness SelfCritChance, mode_effective only)
    let player_base = player.sum(ModType::Base, cfg, &crit_chance_names);
    let enemy_base = if mode_effective {
        enemy.sum(ModType::Base, cfg, &self_crit_chance)
    } else {
        0.0
    };
    let player_inc = player.sum(ModType::Inc, cfg, &crit_chance_names);
    let enemy_inc = if mode_effective {
        enemy.sum(ModType::Inc, cfg, &self_crit_chance)
    } else {
        0.0
    };
    let chance_more = player.more(cfg, &crit_chance_names);

    // The source base and mod contributions are both in percentage points;
    // do all arithmetic in percentage-point space (matching PoB2), converting
    // to a fraction only at the end.
    let base_pct = source_base_pct + player_base + enemy_base;
    let inc = player_inc + enemy_inc;
    let mut crit_pct = base_pct * (1.0 + inc / 100.0) * chance_more;

    // 2) cap (default 100, overridable by Override or Sum BASE)
    let cap = crit_chance_cap(player, cfg);
    crit_pct = crit_pct.min(cap);
    if base_pct > 0.0 {
        crit_pct = crit_pct.max(0.0);
    }
    let pre_effective_pct = crit_pct;

    // 3) mode_effective: hit chance downgrade (the evasion re-roll)
    if mode_effective {
        crit_pct *= hit_chance;
    }

    // 4) Lucky crit
    if mode_effective && player.flag(cfg, ModName::from("CritChanceLucky")) {
        let c = crit_pct / 100.0;
        crit_pct = (1.0 - (1.0 - c).powi(2)) * 100.0;
    }

    // 5) Bifurcate crit (records PreBifurcate for the extra crit damage term)
    let pre_bifurcate_pct = crit_pct;
    let bifurcate = mode_effective && player.flag(cfg, ModName::from("BifurcateCrit"));
    if bifurcate {
        let c = crit_pct / 100.0;
        crit_pct = (1.0 - (1.0 - c).powi(2)) * 100.0;
    }

    // 6) Inevitable crit: set to 100% plus a geometric-series less-damage penalty
    let mut inevitable_less_more: Option<f64> = None;
    let inevitable = mode_effective
        && player.flag(cfg, ModName::from("InevitableCriticalHits"))
        && crit_pct > 0.0;
    if inevitable {
        inevitable_less_more = Some(inevitable_less_crit_bonus(
            crit_pct,
            pre_bifurcate_pct,
            bifurcate,
        ));
        crit_pct = 100.0;
    }

    let crit_chance = round((crit_pct / 100.0).clamp(0.0, 1.0));
    let pre_effective_chance = round((pre_effective_pct / 100.0).clamp(0.0, 1.0));

    // 7) Crit damage
    let crit_multiplier = resolve_crit_multiplier(
        player,
        enemy,
        cfg,
        mode_effective,
        pre_bifurcate_pct,
        crit_pct,
        bifurcate,
        inevitable,
        inevitable_less_more,
    );

    // 8) Average crit effect
    let crit_effect = round(1.0 - crit_chance + crit_chance * crit_multiplier);

    if let Some(sink) = sink {
        record_trace(sink, player, enemy, cfg, mode_effective, crit_effect);
    }

    CritOutcome {
        chance: crit_chance,
        multiplier: crit_multiplier,
        effect: crit_effect,
        pre_effective_chance,
    }
}

/// Crit chance cap (percentage points). PoB2: `Override("CritChanceCap") or Sum("BASE","CritChanceCap")`, default 100.
fn crit_chance_cap(player: &ModDb, cfg: &CalcConfig) -> f64 {
    if let Some(override_cap) = player.override_(cfg, ModName::from("CritChanceCap")) {
        return override_cap;
    }
    let summed = player.sum(ModType::Base, cfg, &[ModName::from("CritChanceCap")]);
    if summed > 0.0 {
        summed
    } else {
        DEFAULT_CRIT_CHANCE_CAP
    }
}

/// Crit damage multiplier (`1 + max(0, extra)`).
///
/// `extra = (BASE_BONUS + ΣCriticalStrikeMultiplier BASE)/100 × (1+Σinc/100) × Πmore`,
/// then (mode_effective only) adds bifurcate's extra share, enemy SelfCritMultiplier, and the inevitable less-damage term.
///
/// Source: CalcOffence.lua L3781–3827.
#[allow(clippy::too_many_arguments)]
fn resolve_crit_multiplier(
    player: &ModDb,
    enemy: &ModDb,
    cfg: &CalcConfig,
    mode_effective: bool,
    pre_bifurcate_pct: f64,
    effective_crit_pct: f64,
    bifurcate: bool,
    inevitable: bool,
    inevitable_less_more: Option<f64>,
) -> f64 {
    // NoCritMultiplier: crit damage has no effect (CalcOffence.lua L3782).
    if player.flag(cfg, ModName::from("NoCritMultiplier")) {
        return 1.0;
    }

    let crit_multiplier_names = [ModName::from("CriticalStrikeMultiplier")];
    let base = player.sum(ModType::Base, cfg, &crit_multiplier_names);
    let inc = player.sum(ModType::Inc, cfg, &crit_multiplier_names);
    let mut more = player.more(cfg, &crit_multiplier_names);

    // The less-damage penalty folded in by inevitable crit becomes one extra MORE (PoB2 `NewMod("CritMultiplier","MORE",lessCritBonus)`).
    if let Some(less_more) = inevitable_less_more {
        more *= 1.0 + less_more / 100.0;
    }

    // An OVERRIDE hard-overrides the crit damage bonus (e.g. `Your Critical
    // Damage Bonus is 250%`): it wins over base/inc/more (PoB2's OVERRIDE
    // semantics), applied directly as the bonus percentage. Otherwise use
    // (100 + ΣBASE)×(1+inc)×more.
    //
    // Note: the OVERRIDE branch **deliberately** ignores the
    // `inevitable_less_more` folded into `more` above — OVERRIDE clamps to a
    // final value, and the inevitable-crit correction only appears under
    // `mode_effective` (the effective view); the panel view is unaffected.
    // The two co-occurring (an inevitable-crit build plus a "crit damage is
    // N%" keystone) is an edge case, currently resolved as the OVERRIDE final value.
    let mut extra =
        if let Some(ov) = player.override_(cfg, ModName::from("CriticalStrikeMultiplier")) {
            ov / 100.0
        } else {
            //  Player's base crit damage bonus now reads from the injected constants pack (fallback == old const, value unchanged).
            (cfg.constants.character().base_critical_hit_damage_bonus + base) / 100.0
                * (1.0 + inc / 100.0)
                * more
        };

    // Bifurcate: the conditional probability "given at least one crit, both
    // hits crit" adds an extra weighted share of crit damage
    // (CalcOffence.lua:3823-3846 `conditionalBifurcateChance =
    // (PreBifurcateCritChance²/100) / CritChance`, matches vendor 0.21/0.22),
    // mutually exclusive with inevitable (the inevitable path already folds
    // the 100%-crit case into the less-damage term).
    if mode_effective && bifurcate && !inevitable {
        let bifurcate_multi_chance = pre_bifurcate_pct.powi(2) / 100.0;
        let conditional = if effective_crit_pct > 0.0 {
            bifurcate_multi_chance / effective_crit_pct
        } else {
            0.0
        };
        extra += conditional * extra;
    }

    // Enemy SelfCritMultiplier (marks, etc.): BASE bonus × (1 + INC/100) (CalcOffence.lua L3814–3825).
    if mode_effective {
        let self_crit_mult = [ModName::from("SelfCritMultiplier")];
        let enemy_inc = 1.0 + enemy.sum(ModType::Inc, cfg, &self_crit_mult) / 100.0;
        extra += enemy.sum(ModType::Base, cfg, &self_crit_mult) / 100.0;
        extra *= enemy_inc;
    }

    round(1.0 + extra.max(0.0))
}

/// Inevitable crit's geometric-series less-damage penalty (folded into a
/// negative `CritMultiplier MORE`, percentage points).
///
/// A strict mirror of PoB2 `CalcOffence.lua` L3716–3733's 4-term truncation:
/// the probability that the Nth attempt (N=1..4) is the first crit,
/// `c·(1-c)^(N-1)`, gets `{100,70,40,10}%` tier crit damage; the expected
/// multiplier `critBonusMultiplier` is folded into
/// `lessCritBonus = round((1-critBonusMultiplier)·-100)`. With bifurcate, the
/// final tier can crit twice, multiplied by `2·PreBifurcate/PostBifurcate`.
///
/// `crit_pct` / `pre_bifurcate_pct` are both percentage points.
fn inevitable_less_crit_bonus(crit_pct: f64, pre_bifurcate_pct: f64, bifurcate: bool) -> f64 {
    let crit_chance = crit_pct / 100.0;
    let non_crit = 1.0 - crit_chance;

    let mut crit_bonus_multiplier = crit_chance
        + 0.7 * non_crit * crit_chance
        + 0.4 * non_crit.powi(2) * crit_chance
        + 0.1 * non_crit.powi(3) * crit_chance;

    if bifurcate {
        // The final tier can crit twice, scaled by that tier's expected crit count (PoB2 L3724–3729).
        crit_bonus_multiplier = crit_bonus_multiplier * 2.0 * pre_bifurcate_pct / crit_pct;
    }

    // PoB2: `round((1 - critBonusMultiplier) * -100.0, 0)` (integer).
    ((1.0 - crit_bonus_multiplier) * -100.0).round()
}

/// Writes crit contributions into the TraceGraph: BASE/INC/MORE (both
/// CritChance and CritMultiplier) plus enemy SelfCrit*, all connected into
/// one [`TraceOperation::Chance`] node carrying `crit_effect`. Sets `sink.node_id`.
fn record_trace(
    sink: &mut TraceSink<'_>,
    player: &ModDb,
    enemy: &ModDb,
    cfg: &CalcConfig,
    mode_effective: bool,
    crit_effect: f64,
) {
    let trace = &mut *sink.trace;
    let crit_node = trace.add_node("crit average factor", crit_effect, TraceOperation::Chance);

    let crit_chance_names = [ModName::from("CriticalStrikeChance")];
    let crit_multiplier_names = [ModName::from("CriticalStrikeMultiplier")];

    // CritChance BASE / INC / MORE (BASE includes the source bucket
    // SkillBaseCritChance, so attribution doesn't lose the weapon/gem source).
    let chance_base = player.sum_traced(
        ModType::Base,
        cfg,
        &[
            ModName::from("CriticalStrikeChance"),
            ModName::from("SkillBaseCritChance"),
        ],
        trace,
        "CriticalStrikeChance BASE sum",
    );
    trace.add_edge(chance_base.node_id, crit_node);
    let chance_inc = player.sum_traced(
        ModType::Inc,
        cfg,
        &crit_chance_names,
        trace,
        "CriticalStrikeChance INC sum",
    );
    trace.add_edge(chance_inc.node_id, crit_node);
    let chance_more = more_factor_traced(
        player,
        cfg,
        &crit_chance_names,
        "CritChance MORE factor",
        trace,
    );
    trace.add_edge(chance_more.node_id, crit_node);

    // CritMultiplier BASE / INC / MORE.
    let mult_base = player.sum_traced(
        ModType::Base,
        cfg,
        &crit_multiplier_names,
        trace,
        "CriticalStrikeMultiplier BASE sum",
    );
    trace.add_edge(mult_base.node_id, crit_node);
    let mult_inc = player.sum_traced(
        ModType::Inc,
        cfg,
        &crit_multiplier_names,
        trace,
        "CriticalStrikeMultiplier INC sum",
    );
    trace.add_edge(mult_inc.node_id, crit_node);
    let mult_more = more_factor_traced(
        player,
        cfg,
        &crit_multiplier_names,
        "CritMultiplier MORE factor",
        trace,
    );
    trace.add_edge(mult_more.node_id, crit_node);

    // Enemy SelfCritChance / SelfCritMultiplier (mode_effective only, attributed to EnemyConfig).
    if mode_effective {
        let enemy_chance = enemy.sum_traced(
            ModType::Base,
            cfg,
            &[ModName::from("SelfCritChance")],
            trace,
            "enemy SelfCritChance BASE sum",
        );
        trace.add_edge(enemy_chance.node_id, crit_node);
        let enemy_mult = enemy.sum_traced(
            ModType::Base,
            cfg,
            &[ModName::from("SelfCritMultiplier")],
            trace,
            "enemy SelfCritMultiplier BASE sum",
        );
        trace.add_edge(enemy_mult.node_id, crit_node);
    }

    // Crit flag contributions (Lucky / Bifurcate / Inevitable /
    // NoCritMultiplier) are connected into attribution so breakdowns can
    // trace back to the flag's source (SourceKind::Config or the mod's own origin).
    for flag in [
        "CritChanceLucky",
        "BifurcateCrit",
        "InevitableCriticalHits",
        "NoCritMultiplier",
    ] {
        if player.flag(cfg, ModName::from(flag)) {
            let flag_node =
                trace.add_source_node(format!("{flag} flag"), 1.0, flag_source(player, cfg, flag));
            trace.add_edge(flag_node, crit_node);
        }
    }

    sink.node_id = Some(crit_node);
}

/// Gets the attribution `SourceId` of the modifier that set a given flag (falls back to Derived `<flag>.FLAG` when there's no origin).
fn flag_source(player: &ModDb, cfg: &CalcConfig, flag: &str) -> SourceId {
    player
        .flag_origin(cfg, ModName::from(flag))
        .unwrap_or_else(|| SourceId::new(SourceKind::Derived, format!("{flag}.FLAG")))
}
