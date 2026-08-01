//! Skill functionality-panel mechanics: AoE radius, projectiles, cooldown, cost/reservation.
//!
//! Implemented per `agent-docs/skill-mechanics.md` + PoB2 `CalcOffence.lua`
//! (the calcRadius / calcSkillCooldown / ProjectileCount / cost sections),
//! with no I/O dependency, pure functions + determinism.
//!
//! ## Design constraints
//! - **Every function is `pub`** (doesn't touch perform/output/offence; the
//!   integration layer is responsible for writing OutputTable during the integration stage).
//! - **Immutable**: inputs are read-only, outputs are new values.
//! - Reuses the `db.sum(Base/Inc)`, `db.more()`, `db.flag()` primitives for ModDb aggregation.
//!
//! ## Deferred
//! - Full numeric values for complex projectile-chain interactions
//!   (DistanceRamp/PointBlank/FarShot damage adjustments).
//! - AreaEffect's special case for DoT (duration transfer like `bleedDurationIsSkillDuration`).
//! - The Spirit reservation pool formula (see recovery-charges-buffs.md; this module only computes the skill-side reservation amount).
//!
//! SupportManaMultiplier has already landed (see §4's cost formula), no longer deferred.

use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb, TraceGraph, TraceNodeId, TraceOperation, TracedValue};

use super::round;

// §1  AoE (area of effect)

/// AoE calculation result.
///
/// - `area_mod`: the area multiplier (inc × more, standard aggregation).
/// - `radius`: the final circular skill radius (integer steps, using PoB2's `calcRadius` formula).
/// - `base_radius_input`: the base radius passed in (for breakdown display).
///
/// Source: `agent-docs/skill-mechanics.md` §Radius;
///       PoB2 `CalcOffence.lua::calcRadius` (`floor(baseRadius * floor(100 * sqrt(areaMod)) / 100)`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AoeResult {
    /// Area multiplier (the inc × more combination).
    pub area_mod: f64,
    /// Final radius (the `calcRadius` formula, internal coordinate units; / 10 gives meters).
    pub radius: f64,
    /// The base radius passed in.
    pub base_radius_input: f64,
}

/// PoB2's `calcRadius`: `floor(baseRadius × floor(100 × √areaMod) / 100)`.
///
/// `base_radius`'s unit matches PoB2 (internal coordinates, / 10 is meters);
/// `areaMod` is the area multiplier (≥0). Source: PoB2 `CalcOffence.lua` L161-162.
pub fn calc_radius(base_radius: f64, area_mod: f64) -> f64 {
    if base_radius <= 0.0 || area_mod <= 0.0 {
        return 0.0;
    }
    (base_radius * (100.0 * area_mod.sqrt()).floor() / 100.0).floor()
}

/// Aggregates AreaOfEffect INC + MORE from the ModDb and computes the radius.
///
/// `base_radius` is the skill/gem's base radius; `extra_base` is
/// `Sum("BASE","AreaOfEffect")`'s fixed addition (in PoB2 this field acts as
/// skillData.radius + radiusExtra + the BASE sum).
///
/// Aggregation formula: `areaMod = (1 + Σinc/100) × Πmore`.
/// `base_radius + extra_base` is calcRadius's input.
///
/// Source: PoB2 `CalcOffence.lua` L414-430 (`calcAreaOfEffect`),
///       `agent-docs/skill-mechanics.md` §AoE.
pub fn calc_aoe(db: &ModDb, cfg: &CalcConfig, base_radius: f64, extra_base: f64) -> AoeResult {
    let aoe_names = [
        ModName::from("AreaOfEffect"),
        ModName::from("AreaOfEffectPrimary"),
    ];
    let inc = db.sum(ModType::Inc, cfg, &aoe_names);
    let more = db.more(cfg, &aoe_names);
    let area_mod = round((1.0 + inc / 100.0) * more);
    let effective_base = base_radius + extra_base + db.sum(ModType::Base, cfg, &aoe_names);
    AoeResult {
        area_mod,
        radius: calc_radius(effective_base, area_mod),
        base_radius_input: base_radius,
    }
}

/// Traced version of `calc_aoe`: writes INC/MORE contributions into the
/// TraceGraph, returning `(result, radius_node)`.
///
/// `radius_node` carries the final radius value; the caller can continue connecting downstream nodes.
pub fn calc_aoe_traced(
    db: &ModDb,
    cfg: &CalcConfig,
    base_radius: f64,
    extra_base: f64,
    trace: &mut TraceGraph,
) -> (AoeResult, TraceNodeId) {
    let result = calc_aoe(db, cfg, base_radius, extra_base);

    let aoe_names = [
        ModName::from("AreaOfEffect"),
        ModName::from("AreaOfEffectPrimary"),
    ];
    let base_node = trace.add_source_node(
        "base radius",
        base_radius,
        SourceId::new(SourceKind::CharacterBase, "base.AoeRadius"),
    );
    let inc_node = db.sum_traced(ModType::Inc, cfg, &aoe_names, trace, "AreaOfEffect INC sum");
    let more_node = db.more_traced(cfg, &aoe_names, trace, "AreaOfEffect MORE factor");
    let radius_node = trace.add_node("AoE radius", result.radius, TraceOperation::Multiply);
    trace.add_edge(base_node, radius_node);
    trace.add_edge(inc_node.node_id, radius_node);
    trace.add_edge(more_node.node_id, radius_node);

    (result, radius_node)
}

/// Convenience variant of `calc_aoe_traced` that directly returns a `TracedValue` (radius + node).
pub fn calc_aoe_traced_value(
    db: &ModDb,
    cfg: &CalcConfig,
    base_radius: f64,
    extra_base: f64,
    trace: &mut TraceGraph,
) -> TracedValue {
    let (result, node_id) = calc_aoe_traced(db, cfg, base_radius, extra_base, trace);
    TracedValue {
        value: result.radius,
        node_id,
    }
}

// §2  Projectiles

/// Projectile behavior priority (PoE2's fixed order: Split → Pierce → Fork → Chain).
///
/// A single collision can only trigger one behavior; a piercing or forking
/// projectile doesn't chain off enemies (but can still chain off terrain).
/// Source: `agent-docs/skill-mechanics.md` §Projectile behavior priority; PoE2 wiki/chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectileBehavior {
    /// Split: splits into N new projectiles on first hit.
    Split,
    /// Pierce: continues flying through the target.
    Pierce,
    /// Fork: splits into two on first hit (a fixed angle).
    Fork,
    /// Chain: redirects to the nearest unhit target after collision (6m range).
    Chain,
    /// No behavior at all (disappears on impact).
    None,
}

/// Projectile behavior resolution input, sourced from ModDb flags + counts.
///
/// Every field is either "is it active" or "max count", used by
/// `resolve_projectile_behavior` to decide the priority chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProjectileBehaviorInput {
    // Split
    pub cannot_split: bool,
    pub split_count: u32,
    // Pierce
    pub cannot_pierce: bool,
    pub pierce_all_targets: bool,
    pub pierce_count: u32,  // BASE pierce count
    pub pierce_chance: u32, // BASE pierce chance (0-100)
    // Fork
    pub cannot_fork: bool,
    pub fork_once: bool,     // ForkOnce flag
    pub fork_twice: bool,    // ForkTwice flag
    pub fork_count_max: u32, // Extra Fork count
    // Chain
    pub cannot_chain: bool,
    pub chain_count_max: u32, // BASE max chain count
    // Special conversion flags
    pub additional_projectiles_add_splits_instead: bool,
    pub additional_projectiles_add_chains_instead: bool,
}

/// Projectile behavior resolution result: determines the effective chain by priority Split→Pierce→Fork→Chain.
///
/// `behaviors` is the list of active behaviors in priority order (may be
/// empty); `effective_pierce_all` indicates infinite pierce is active (the
/// highest-priority pierce mode, after which no further behavior ever triggers).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectileBehaviorResult {
    /// Active behaviors in priority order Split→Pierce→Fork→Chain.
    pub behaviors: Vec<ProjectileBehavior>,
    /// Whether infinite pierce is active (PierceAllTargets / 100 pierces).
    pub effective_pierce_all: bool,
    /// Split count (0 = not active).
    pub split_count: u32,
    /// Effective pierce count (chance folded in; 0 = not active; `u32::MAX` = infinite pierce).
    pub pierce_count: u32,
    /// Fork max count (0 = not active).
    pub fork_count_max: u32,
    /// Chain max count (0 = not active).
    pub chain_count_max: u32,
}

/// Resolves the effective projectile behavior chain by priority
/// Split→Pierce→Fork→Chain (pure logic).
///
/// Activation rules:
/// - **Split**: `split_count > 0` and `!cannot_split`.
/// - **Pierce**: `pierce_all_targets` or `pierce_count + pierce_chance/100 > 0`, and `!cannot_pierce`.
/// - **Fork**: `fork_once || fork_twice || fork_count_max > 0`, and `!cannot_fork`.
/// - **Chain**: `chain_count_max > 0` and `!cannot_chain`.
///
/// When infinite pierce (`pierce_all_targets` or pierce count ≥ 100) is
/// active, subsequent behaviors (Fork/Chain) **never trigger** (a projectile
/// that can pierce every target never chains off enemies). This constraint
/// only applies to chaining off enemies; terrain-chaining logic is
/// **deferred** at this stage.
///
/// Source: `agent-docs/skill-mechanics.md` §Projectile behavior priority; PoB2 `CalcOffence.lua` L1298-1344.
pub fn resolve_projectile_behavior(
    input: &ProjectileBehaviorInput,
    additional_projectile_count: u32,
) -> ProjectileBehaviorResult {
    let mut behaviors = Vec::new();
    let mut split_count = 0u32;
    let mut pierce_count = 0u32;
    let mut fork_count_max = 0u32;
    let mut chain_count_max = input.chain_count_max;
    let mut effective_pierce_all = false;

    // Additional projectile conversion (PoB2 L1307-1311):
    // AdditionalProjectilesAddSplitsInstead or AdditionalProjectilesAddChainsInstead
    // converts additional projectiles into Split / Chain counts.
    let extra = if additional_projectile_count > 0 {
        additional_projectile_count
    } else {
        0
    };

    // Split
    let raw_split = input.split_count
        + if input.additional_projectiles_add_splits_instead {
            extra
        } else {
            0
        };
    if raw_split > 0 && !input.cannot_split {
        split_count = raw_split;
        behaviors.push(ProjectileBehavior::Split);
    }

    // Pierce
    if !input.cannot_pierce {
        if input.pierce_all_targets {
            // Infinite pierce: pierce_count is set to 100 to mean "pierces everything"
            pierce_count = 100;
            effective_pierce_all = true;
            behaviors.push(ProjectileBehavior::Pierce);
        } else {
            // pierce_count BASE + pierce_chance/100 folded into an effective count
            let effective_pc = input.pierce_count + input.pierce_chance / 100;
            if effective_pc > 0 {
                pierce_count = effective_pc;
                behaviors.push(ProjectileBehavior::Pierce);
            }
        }
    }

    // Fork (only considered when infinite pierce isn't active)
    if !effective_pierce_all && !input.cannot_fork {
        // ForkOnce → max=1; ForkTwice → max=2; extra fork_count_max stacks but clamps to 2 (PoB2 L1320-1323)
        let raw_fork_max = if input.fork_twice {
            2u32
        } else if input.fork_once {
            1u32
        } else {
            0u32
        };
        let raw_fork = raw_fork_max.max(input.fork_count_max.min(2));
        if raw_fork > 0 {
            fork_count_max = raw_fork;
            behaviors.push(ProjectileBehavior::Fork);
        }
    }

    // Chain (only considered when infinite pierce isn't active)
    if !effective_pierce_all && !input.cannot_chain {
        // AdditionalProjectilesAddChainsInstead converts additional projectiles into chains
        if input.additional_projectiles_add_chains_instead {
            chain_count_max += extra;
        }
        if chain_count_max > 0 {
            behaviors.push(ProjectileBehavior::Chain);
        }
    }

    ProjectileBehaviorResult {
        behaviors,
        effective_pierce_all,
        split_count,
        pierce_count,
        fork_count_max,
        chain_count_max,
    }
}

/// Projectile count calculation result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProjectileCountResult {
    /// Total projectile count (projBase × projMore).
    pub projectile_count: f64,
    /// Base projectile count (sum of Base).
    pub base_count: f64,
    /// The MORE factor (product).
    pub more_factor: f64,
    /// Additional projectile count (base_count - 1; PoE2's convention of base=-1 meaning "1 base shot").
    pub additional_count: f64,
}

/// Calculates the projectile count.
///
/// PoB2 `CalcOffence.lua` L1286-1291:
/// ```lua
/// projBase = Sum("BASE","ProjectileCount") + 2*TwoAdditionalProjectilesChance/100 + SurpassingProjectileChance/100
/// projMore = More("ProjectileCount")
/// output.ProjectileCount = projBase * projMore
/// ```
///
/// Forces a return of 1 shot when the `NoAdditionalProjectiles` flag is true (PoB2 L1286-1287).
///
/// Source: `agent-docs/skill-mechanics.md` §Projectile count; PoB2 `CalcOffence.lua` L1286-1291.
pub fn calc_projectile_count(db: &ModDb, cfg: &CalcConfig) -> ProjectileCountResult {
    // NoAdditionalProjectiles: locks to 1 shot
    if db.flag(cfg, ModName::from("NoAdditionalProjectiles")) {
        return ProjectileCountResult {
            projectile_count: 1.0,
            base_count: 1.0,
            more_factor: 1.0,
            additional_count: 0.0,
        };
    }

    let proj_names = [ModName::from("ProjectileCount")];
    let two_add_names = [ModName::from("TwoAdditionalProjectilesChance")];
    let surpassing_names = [ModName::from("SurpassingProjectileChance")];

    let base = db.sum(ModType::Base, cfg, &proj_names)
        + 2.0 * db.sum(ModType::Base, cfg, &two_add_names) / 100.0
        + db.sum(ModType::Base, cfg, &surpassing_names) / 100.0;
    let more = db.more(cfg, &proj_names);
    let count = round(base * more);
    let additional = (count - 1.0).max(0.0);

    ProjectileCountResult {
        projectile_count: count,
        base_count: base,
        more_factor: more,
        additional_count: additional,
    }
}

/// Traced version of `calc_projectile_count`.
pub fn calc_projectile_count_traced(
    db: &ModDb,
    cfg: &CalcConfig,
    trace: &mut TraceGraph,
) -> (ProjectileCountResult, TraceNodeId) {
    let result = calc_projectile_count(db, cfg);

    let proj_names = [ModName::from("ProjectileCount")];
    let base_node = db.sum_traced(
        ModType::Base,
        cfg,
        &proj_names,
        trace,
        "ProjectileCount BASE sum",
    );
    let more_node = db.more_traced(cfg, &proj_names, trace, "ProjectileCount MORE factor");
    let count_node = trace.add_node(
        "projectile count",
        result.projectile_count,
        TraceOperation::Multiply,
    );
    trace.add_edge(base_node.node_id, count_node);
    trace.add_edge(more_node.node_id, count_node);

    (result, count_node)
}

// §3  Cooldown

/// Cooldown calculation result.
///
/// Source: `agent-docs/skill-mechanics.md` §Cooldown;
///       PoB2 `CalcOffence.lua::calcSkillCooldown` (L325-346).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CooldownResult {
    /// Base cooldown (the skill's inherent cooldown + the Base addition, seconds).
    pub base_cooldown: f64,
    /// Cooldown recovery rate factor (`(1 + Σinc/100) × Πmore`, used as a divisor).
    pub recovery_rate: f64,
    /// Actual cooldown (`base / recovery_rate`, seconds).
    ///
    /// - No multiple storage (`stored_uses ≤ 1`): rounded up to the server tick (≈ 1/30.3 s).
    /// - Multiple storage (`stored_uses > 1`): **not rounded** (per PoB2 L340's comment).
    pub cooldown: f64,
    /// Maximum storable uses (`stored_uses_base + AdditionalCooldownUses BASE`).
    pub stored_uses: u32,
    /// Whether the cooldown was rounded up to the server tick.
    pub rounded_to_tick: bool,
}

/// Calculates skill cooldown.
///
/// `base_cooldown_s`: the skill's inherent cooldown (seconds, 0 means no cooldown).
/// `base_stored_uses`: the skill gem's built-in storage count
/// (`skillData.storedUses`, 0 or 1 means no extra storage).
///
/// Aggregation formula:
/// `cooldown = base / max(0, (1 + Σinc_CooldownRecovery/100) × Πmore_CooldownRecovery)`
///
/// - When there's an Override(CooldownRecovery), uses the override value directly (PoB2 L326).
/// - Not rounded to the server tick when `stored_uses > 1` (PoB2 L340).
///
/// Source: PoB2 `CalcOffence.lua::calcSkillCooldown` L325-346.
pub fn calc_cooldown(
    db: &ModDb,
    cfg: &CalcConfig,
    base_cooldown_s: f64,
    base_stored_uses: u32,
) -> CooldownResult {
    let cd_recovery_names = [ModName::from("CooldownRecovery")];

    // Base cooldown modifier (adds/subtracts a millisecond-level value directly, PoB2 L327)
    let added_cooldown = db.sum(ModType::Base, cfg, &cd_recovery_names);
    let cooldown_base = base_cooldown_s + added_cooldown;

    // Override check (PoB2 L326)
    let override_val = db.override_(cfg, ModName::from("CooldownRecovery"));
    let recovery_rate = if let Some(ov) = override_val {
        // Override gives the final cooldown directly (seconds); recovery_rate is set to 1.0 to mean no scaling
        let _ = ov; // returns directly when using the override value
        // PoB2: cooldown = override
        let stored_uses = base_stored_uses
            + db.sum(
                ModType::Base,
                cfg,
                &[ModName::from("AdditionalCooldownUses")],
            ) as u32;
        let (cd, rounded) =
            finalize_cooldown(ov, stored_uses, cfg.constants.game().server_tick_seconds);
        return CooldownResult {
            base_cooldown: cooldown_base,
            recovery_rate: 1.0,
            cooldown: cd,
            stored_uses,
            rounded_to_tick: rounded,
        };
    } else {
        let inc = db.sum(ModType::Inc, cfg, &cd_recovery_names);
        let more = db.more(cfg, &cd_recovery_names);
        // Used as a divisor, must not be 0
        ((1.0 + inc / 100.0) * more).max(f64::EPSILON)
    };

    let raw_cd = if cooldown_base > 0.0 || base_cooldown_s > 0.0 {
        cooldown_base / recovery_rate
    } else {
        0.0
    };

    let stored_uses = base_stored_uses
        + db.sum(
            ModType::Base,
            cfg,
            &[ModName::from("AdditionalCooldownUses")],
        ) as u32;
    let (cd, rounded) = finalize_cooldown(
        raw_cd,
        stored_uses,
        cfg.constants.game().server_tick_seconds,
    );

    CooldownResult {
        base_cooldown: cooldown_base,
        recovery_rate: round(recovery_rate),
        cooldown: cd,
        stored_uses,
        rounded_to_tick: rounded,
    }
}

/// Decides whether to round to the server tick based on the storage count, returning `(cooldown, rounded_to_tick)`.
///
/// PoB2 L340: not rounded when there's multiple storage (`it doesn't round the cooldown value to server ticks`).
fn finalize_cooldown(raw_cd: f64, stored_uses: u32, tick_seconds: f64) -> (f64, bool) {
    if raw_cd <= 0.0 {
        return (0.0, false);
    }
    // stored_uses > 1: not rounded
    if stored_uses > 1 {
        (round(raw_cd), false)
    } else {
        // Rounded up to the server tick: ceil(cd × ServerTickRate) / ServerTickRate
        // (tick now comes from the injected constants pack via the caller; fallback == old const, value unchanged)
        let tick_rate = 1.0 / tick_seconds;
        let rounded = (raw_cd * tick_rate).ceil() / tick_rate;
        (round(rounded), true)
    }
}

/// Traced version of `calc_cooldown`.
pub fn calc_cooldown_traced(
    db: &ModDb,
    cfg: &CalcConfig,
    base_cooldown_s: f64,
    base_stored_uses: u32,
    trace: &mut TraceGraph,
) -> (CooldownResult, TraceNodeId) {
    let result = calc_cooldown(db, cfg, base_cooldown_s, base_stored_uses);

    let cd_recovery_names = [ModName::from("CooldownRecovery")];
    let base_node = trace.add_source_node(
        "base cooldown",
        base_cooldown_s,
        SourceId::new(SourceKind::CharacterBase, "base.Cooldown"),
    );
    let inc_node = db.sum_traced(
        ModType::Inc,
        cfg,
        &cd_recovery_names,
        trace,
        "CooldownRecovery INC sum",
    );
    let more_node = db.more_traced(
        cfg,
        &cd_recovery_names,
        trace,
        "CooldownRecovery MORE factor",
    );
    let cd_node = trace.add_node("cooldown", result.cooldown, TraceOperation::Multiply);
    trace.add_edge(base_node, cd_node);
    trace.add_edge(inc_node.node_id, cd_node);
    trace.add_edge(more_node.node_id, cd_node);

    (result, cd_node)
}

// §4  Cost and reservation

/// Skill cost calculation result (a single resource type).
///
/// Formula:
/// `cost = floor(floor(base_cost × floor4(ΠSupportManaMultiplier)) × (1 + Σinc/100)) × Πmore`
/// (the support gem cost multiplier is applied to the base and rounded
/// before inc/more; each factor is rounded step by step, matching PoB2's stepwise rounding logic).
///
/// Source: `agent-docs/skill-mechanics.md` §Cost;
///       PoB2 `CalcOffence.lua` L2040+ (`ManaCost / LifeCost / ESCost`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkillCostResult {
    /// Resource type.
    pub kind: SkillCostKind,
    /// Base cost (the gem/skill data's raw value, after dividing by the Divisor).
    pub base_cost: f64,
    /// Final cost (after inc/more aggregation, floored to an integer).
    pub final_cost: f64,
    /// Whether the cost is waived by the `HasNoCost` flag.
    pub no_cost: bool,
}

/// Calculates skill resource cost (Mana / Life / ES, not including Spirit
/// reservation -- Spirit goes through [`calc_spirit_reservation`]; the
/// support gem's generic cost multiplier doesn't increase Spirit reservation).
///
/// `resource_mod_prefix`: the ModName prefix (e.g. `"Mana"` corresponds to `ManaCost` INC/MORE).
/// `base_cost`: the gem's raw cost value (already divided by the Divisor).
/// `kind`: `SkillCostKind` (carried through to the result).
///
/// Free-cost check: the `HasNoCost` flag → `final_cost = 0`.
///
/// Support gem cost multiplier: `SupportManaMultiplier` MORE (sourced from a
/// compatible support's per-level `mana_multiplier`, PoB2
/// `CalcActiveSkill.lua:689-691`) -- the product is **truncated to 4 decimal
/// places**, then applied to base and floored before entering the inc/more
/// chain: PoB2 `CalcOffence.lua:2052`'s `mult = floor(More(skillCfg, "SupportManaMultiplier"), 4)`,
/// `:2076-2077`'s `finalBaseCost = m_floor(baseCost × mult + baseCostNoMult)`.
/// The generic path applies to every resource listed above (PoB2 only marks
/// Soul as `unaffectedByGenericCostMults`; this function doesn't handle Soul).
///
/// Source: PoB2 `CalcOffence.lua` L2050-2160 (the generic path).
pub fn calc_skill_cost(
    db: &ModDb,
    cfg: &CalcConfig,
    kind: SkillCostKind,
    resource_mod_prefix: &str,
    base_cost: f64,
) -> SkillCostResult {
    // HasNoCost waives it entirely
    if db.flag(cfg, ModName::from("HasNoCost")) {
        return SkillCostResult {
            kind,
            base_cost,
            final_cost: 0.0,
            no_cost: true,
        };
    }

    // Support gem cost multiplier: the product truncated to 4 decimal places
    // → applied to base → floored (PoB2 CalcOffence.lua:2052/:2076-2077, before the inc/more chain).
    let base_cost_after_support = (base_cost * support_cost_multiplier(db, cfg)).floor();
    let final_cost = apply_cost_chain(db, cfg, resource_mod_prefix, base_cost_after_support);

    SkillCostResult {
        kind,
        base_cost,
        final_cost: round(final_cost),
        no_cost: false,
    }
}

/// The cost inc/more/efficiency chain (vendor CalcOffence.lua:2126-2160's
/// second loop); the `final_base` parameter = finalBaseCost, already including SupportManaMultiplier.
fn apply_cost_chain(
    db: &ModDb,
    cfg: &CalcConfig,
    resource_mod_prefix: &str,
    final_base: f64,
) -> f64 {
    let type_cost_name = format!("{resource_mod_prefix}Cost");
    let generic_cost_name = "Cost";
    let inc_names = [
        ModName::from(type_cost_name.as_str()),
        ModName::from(generic_cost_name),
    ];
    // ManaCost / Cost INC (PoB2: `skillModList:Sum("INC", skillCfg, type.."Cost", "Cost")`)
    let inc = db.sum(ModType::Inc, cfg, &inc_names);

    // ManaCost MORE (type) × Cost MORE (generic) (PoB2's two-step more multiplication)
    let more_type = db.more(cfg, &[ModName::from(type_cost_name.as_str())]);
    let more_generic = db.more(cfg, &[ModName::from(generic_cost_name)]);

    // PoB2's stepwise rounding:
    //   1) `floor(finalBaseCost × (1+inc/100))` (inc positive → floor, inc negative → ceil)
    //   2) `floor/ceil(step1 × moreType)` (more < 1 → ceil)
    //   3) `floor/ceil(step2 × moreGeneric)`
    let after_inc = if inc >= 0.0 {
        (final_base * (1.0 + inc / 100.0)).floor()
    } else {
        (final_base * (1.0 + inc / 100.0)).ceil()
    };
    let after_more_type = if more_type < 1.0 {
        (after_inc * more_type).ceil()
    } else {
        (after_inc * more_type).floor()
    };
    let after_more = (if more_generic < 1.0 {
        (after_more_type * more_generic).ceil()
    } else {
        (after_more_type * more_generic).floor()
    })
    .max(0.0);

    // Cost Efficiency: **divides** by `1 + efficiency/100` after inc/more
    // (already rounded), with the result no longer rounded.
    // `{type}CostEfficiency` + generic `CostEfficiency` stack additively.
    // PoB2: 9 mana, 50% eff → 6; 25% generic → 7.2; 25%+25% → 6; 50% inc + 50% eff → floor(9×1.5)/1.5 = 8.67.
    let efficiency = db.sum(
        ModType::Inc,
        cfg,
        &[
            ModName::from(format!("{resource_mod_prefix}CostEfficiency").as_str()),
            ModName::from("CostEfficiency"),
        ],
    );
    after_more / (1.0 + efficiency / 100.0)
}

/// The support gem cost multiplier product, truncated to 4 decimal places
/// (PoB2 CalcOffence.lua:2062's `mult = floor(More(skillCfg, "SupportManaMultiplier"), 4)`).
fn support_cost_multiplier(db: &ModDb, cfg: &CalcConfig) -> f64 {
    let m = db.more(cfg, &[ModName::from("SupportManaMultiplier")]);
    (m * 10000.0).floor() / 10000.0
}

/// Hybrid mana→life cost share (0..=1). Sourced from
/// `HybridManaAndLifeCost_Life` BASE (vendor stat
/// `base_skill_cost_life_instead_of_mana_%`, e.g. Atalui's Bloodletting's
/// constantStat 100; the same-named tree mod in the Blood-Magic family),
/// vendor caps this at 100 (CalcOffence.lua:2067's `m_min(Sum(...), 100) / 100`).
pub fn hybrid_life_cost_share(db: &ModDb, cfg: &CalcConfig) -> f64 {
    (db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("HybridManaAndLifeCost_Life")],
    )
    .min(100.0)
        / 100.0)
        .max(0.0)
}

/// Convenience Mana cost calculation.
///
/// Note: under the hybrid mana→life conversion ([`hybrid_life_cost_share`] >
/// 0), vendor appends `floor((1 - hybrid) × ManaCost)` at the end of the
/// chain (CalcOffence.lua:2160-2162) -- performed by the caller (perform
/// fill); this function keeps a pure single-resource chain.
pub fn calc_mana_cost(db: &ModDb, cfg: &CalcConfig, base_mana_cost: f64) -> SkillCostResult {
    calc_skill_cost(db, cfg, SkillCostKind::Mana, "Mana", base_mana_cost)
}

/// Convenience Life cost calculation.
pub fn calc_life_cost(db: &ModDb, cfg: &CalcConfig, base_life_cost: f64) -> SkillCostResult {
    calc_skill_cost(db, cfg, SkillCostKind::Life, "Life", base_life_cost)
}

/// Life cost (including hybrid mana→life conversion, vendor
/// CalcOffence.lua:2090-2104's Life branch):
/// `life.finalBaseCost = round(base_life×mult + round(floor(base_mana×mult) × hybrid))`,
/// then runs the Life cost inc/more/efficiency chain. Equivalent to [`calc_life_cost`] when hybrid = 0.
pub fn calc_life_cost_hybrid(
    db: &ModDb,
    cfg: &CalcConfig,
    base_life_cost: f64,
    base_mana_cost: f64,
) -> SkillCostResult {
    let hybrid = hybrid_life_cost_share(db, cfg);
    if hybrid <= 0.0 {
        return calc_life_cost(db, cfg, base_life_cost);
    }
    if db.flag(cfg, ModName::from("HasNoCost")) {
        return SkillCostResult {
            kind: SkillCostKind::Life,
            base_cost: base_life_cost,
            final_cost: 0.0,
            no_cost: true,
        };
    }
    let mult = support_cost_multiplier(db, cfg);
    let mana_final_base = (base_mana_cost * mult).floor();
    let life_final_base = (base_life_cost * mult + (mana_final_base * hybrid).round()).round();
    SkillCostResult {
        kind: SkillCostKind::Life,
        base_cost: base_life_cost,
        final_cost: round(apply_cost_chain(db, cfg, "Life", life_final_base)),
        no_cost: false,
    }
}

/// Convenience Spirit (reservation) cost calculation.
///
/// Spirit reservation amount = `ReservationMultiplier MORE` (the gem-level
/// reservation multiplier) applied to the gem's base reservation value, plus
/// `ExtraSpirit BASE` (`spiritReservationFlat`). A support gem's generic
/// cost multiplier does **not** increase Spirit reservation (unless there's a reservation-multiplier mod).
///
/// The corresponding PoB2 fields (CalcActiveSkill.lua / CalcOffence.lua's
/// reservation section): `reservedFlat + floor(pool × reservedPercent/100)`
/// (only for the percentage-based reservation case).
///
/// This function handles **flat Spirit reservation** (the common form: a
/// gem consumes a fixed Spirit amount). `base_spirit_reservation`: the gem's base Spirit reservation (flat).
pub fn calc_spirit_reservation(
    db: &ModDb,
    cfg: &CalcConfig,
    base_spirit_reservation: f64,
) -> SkillCostResult {
    if db.flag(cfg, ModName::from("HasNoCost")) {
        return SkillCostResult {
            kind: SkillCostKind::Spirit,
            base_cost: base_spirit_reservation,
            final_cost: 0.0,
            no_cost: true,
        };
    }

    let reservation_more_names = [ModName::from("ReservationMultiplier")];
    let extra_spirit_names = [ModName::from("ExtraSpirit")];
    let more = db.more(cfg, &reservation_more_names);
    let extra_flat = db.sum(ModType::Base, cfg, &extra_spirit_names);

    // ⚠️ Reservation Efficiency is **not** divided out here -- vendor's
    // efficiency is per-skill (CalcDefence.lua:240-243 computes
    // `Σinc(SpiritReservationEfficiency, ReservationEfficiency)` per
    // activeSkill against its own skillCfg; a scoped mod like "Meta Skills
    // have N% …" only applies to matching skills). PoBR's application point
    // is on the **injection side**, `pobr-build::spirit_reservation_modifiers`
    // (per-gem cfg + the gem quality term), where each
    // `SkillSpiritReservationBase` injected has already been divided.
    // A past implementation divided the aggregate total by global efficiency
    // here too -- constituting **double application** with the injection
    // side (the root cause of frost-bomb's panel reservation showing 148 vs
    // an injected total of 166, an 18 discrepancy) -- and has since been removed.
    //
    // Spirit reservation = base × more + extra_flat
    // (a flat addition, per PoB2 CalcOffence.lua's reservation section)
    let reserved = (base_spirit_reservation * more + extra_flat)
        .floor()
        .max(0.0);

    SkillCostResult {
        kind: SkillCostKind::Spirit,
        base_cost: base_spirit_reservation,
        final_cost: round(reserved),
        no_cost: false,
    }
}

/// Traced version of `calc_skill_cost` (using Mana as the example; other resource types work the same way).
pub fn calc_skill_cost_traced(
    db: &ModDb,
    cfg: &CalcConfig,
    kind: SkillCostKind,
    resource_mod_prefix: &str,
    base_cost: f64,
    trace: &mut TraceGraph,
) -> (SkillCostResult, TraceNodeId) {
    let result = calc_skill_cost(db, cfg, kind, resource_mod_prefix, base_cost);

    let type_cost_name = format!("{resource_mod_prefix}Cost");
    let inc_names = [
        ModName::from(type_cost_name.as_str()),
        ModName::from("Cost"),
    ];
    let base_node = trace.add_source_node(
        format!("{resource_mod_prefix} base cost"),
        base_cost,
        SourceId::new(SourceKind::CharacterBase, format!("base.{type_cost_name}")),
    );
    let inc_node = db.sum_traced(
        ModType::Inc,
        cfg,
        &inc_names,
        trace,
        format!("{type_cost_name} INC sum"),
    );
    let more_type_node = db.more_traced(
        cfg,
        &[ModName::from(type_cost_name.as_str())],
        trace,
        format!("{type_cost_name} MORE factor"),
    );
    // The support gem's cost multiplier: added to the graph as an independent factor node (traceable back to the SupportGem source).
    let support_mult_node = db.more_traced(
        cfg,
        &[ModName::from("SupportManaMultiplier")],
        trace,
        "SupportManaMultiplier MORE factor",
    );
    let cost_node = trace.add_node(
        format!("{resource_mod_prefix} final cost"),
        result.final_cost,
        TraceOperation::Multiply,
    );
    trace.add_edge(base_node, cost_node);
    trace.add_edge(inc_node.node_id, cost_node);
    trace.add_edge(more_type_node.node_id, cost_node);
    trace.add_edge(support_mult_node.node_id, cost_node);

    (result, cost_node)
}
