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
/// The skill's radius stats and area modifiers determine its circular area;
/// echoes use the selected repeat view (`agent-docs/skill-mechanics.md` §AoE).
pub fn calc_aoe(db: &ModDb, cfg: &CalcConfig, base_radius: f64, extra_base: f64) -> AoeResult {
    let aoe_names = [
        ModName::from("AreaOfEffect"),
        ModName::from("AreaOfEffectPrimary"),
    ];
    // Spell Echo's area bonus belongs to echoes only. The Average view
    // weights it by echo count / all casts; Final shows the echo's area.
    // Game stats: support_spell_echo_number_of_echo_cascades and
    // echoed_spell_area_of_effect_+% (one echo, 40% increased area).
    let repeats = db.sum(ModType::Base, cfg, &[ModName::from("RepeatCount")]);
    let repeat_inc = if repeats > 0.0 && !db.flag(cfg, "CannotRepeat") {
        let weight = if cfg.condition("averageRepeat") {
            repeats / (1.0 + repeats)
        } else if cfg.condition("alwaysFinalRepeat") {
            1.0
        } else {
            0.0
        };
        db.sum(ModType::Inc, cfg, &[ModName::from("RepeatAreaOfEffect")]) * weight
    } else {
        0.0
    };
    let inc = db.sum(ModType::Inc, cfg, &aoe_names) + repeat_inc;
    let more = db.more(cfg, &aoe_names);
    let area_mod = round((1.0 + inc / 100.0) * more);
    let effective_base = base_radius + extra_base + db.sum(ModType::Base, cfg, &aoe_names);
    let radius_names = [ModName::from("AreaOfEffectRadius")];
    let radius_mod = ((1.0 + db.sum(ModType::Inc, cfg, &radius_names) / 100.0)
        * db.more(cfg, &radius_names))
    .max(0.0);
    let radius_percent = ((100.0 * area_mod.max(0.0).sqrt()).floor() * radius_mod).floor();
    AoeResult {
        area_mod,
        radius: (effective_base.max(0.0) * radius_percent / 100.0).floor(),
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
    /// Effective pierce count (chance folded in; 0 = not active; 100 = infinite pierce).
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
    let mut chain_count_max = 0;
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
        chain_count_max = input.chain_count_max;
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
    if db.flag(cfg, "NoAdditionalProjectiles") {
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

/// "Skills have -# seconds to Cooldown" (Temporalis, the game's
/// `unique_cooldown_modifier_ms`), in seconds: added to a skill's cooldown
/// before Cooldown Recovery Rate.
pub const FLAT_COOLDOWN: &str = "CooldownRecoveryFromTemporalis";

/// The shortest cooldown [`FLAT_COOLDOWN`] can bring a skill to: such
/// modifiers cannot reduce a cooldown below 0.1 seconds (the game's wiki;
/// since 0.1.1). Whether a Cooldown Recovery Rate applied after them may
/// take it lower is not stated; it is kept at 0.1, as the exporting tool
/// keeps it, which has not been measured in the game.
pub const FLAT_COOLDOWN_MINIMUM: f64 = 0.1;

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
    // Only a skill with a cooldown of its own has seconds taken off it.
    let flat_cooldown = if base_cooldown_s > 0.0 {
        db.sum(ModType::Base, cfg, &[ModName::from(FLAT_COOLDOWN)])
    } else {
        0.0
    };
    let cooldown_base = base_cooldown_s + added_cooldown + flat_cooldown;

    // Override check (PoB2 L326)
    let override_val = db.override_(cfg, "CooldownRecovery");
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
    let raw_cd = if flat_cooldown < 0.0 {
        raw_cd.max(FLAT_COOLDOWN_MINIMUM)
    } else {
        raw_cd
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
    if db.flag(cfg, "HasNoCost") {
        return SkillCostResult {
            kind,
            base_cost,
            final_cost: 0.0,
            no_cost: true,
        };
    }

    // Support gem cost multiplier: the product truncated to 4 decimal places
    // → applied to base → floored (PoB2 CalcOffence.lua:2052/:2076-2077, before the inc/more chain).
    let base_cost_after_support = (base_cost * support_cost_multiplier(db, cfg)
        + unscaled_cost(db, cfg, resource_mod_prefix))
    .floor();
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

/// A cost added to the skill's own that its supports' cost multipliers do not
/// scale (`<Resource>CostNoMult`): Archmage's "Non-Channelling Spells cost an
/// additional N% of your maximum Mana".
fn unscaled_cost(db: &ModDb, cfg: &CalcConfig, resource_mod_prefix: &str) -> f64 {
    let name = format!("{resource_mod_prefix}CostNoMult");
    db.sum(ModType::Base, cfg, &[ModName::from(name.as_str())])
        .max(0.0)
}

/// The share (0 and up) of the skill's base Mana cost it also costs in Life:
/// "Skills gain a Base Life Cost equal to Base Mana Cost" is 1.
fn base_mana_cost_as_life_share(db: &ModDb, cfg: &CalcConfig) -> f64 {
    (db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("BaseManaCostAsLifeCost")],
    ) / 100.0)
        .max(0.0)
}

/// What lines like "Non-Channelling Spells cost an additional 6% of your
/// maximum Life" add to the skill's base cost in that pool (game stats
/// `base_life_cost_+_with_non_channelling_spells_%_maximum_life`,
/// `attacks_base_mana_cost_+_%_of_maximum_mana`,
/// `base_mana_cost_+_with_non_channelling_attacks_%_maximum_mana`): that
/// share of the maximum in whole points, rounded down. Like the skill's own
/// base cost, it is then scaled by the supports' cost multipliers and the
/// cost modifiers.
fn cost_from_maximum_pool(db: &ModDb, cfg: &CalcConfig, pool: &str) -> f64 {
    let name = format!("{pool}CostPercentOfMaximum{pool}");
    let percent = db.sum(ModType::Base, cfg, &[ModName::from(name.as_str())]);
    if percent <= 0.0 {
        return 0.0;
    }
    (cfg.stat(pool) * percent / 100.0).floor()
}

/// What one use of the skill costs.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SkillCosts {
    pub mana: f64,
    pub life: f64,
}

/// The Mana and Life cost of the skill, from its base costs
/// (`SkillManaCostBase` / `SkillLifeCostBase`) and everything that moves a
/// cost between the two. A resource the skill has no cost in stays 0.
pub fn skill_costs(db: &ModDb, cfg: &CalcConfig) -> SkillCosts {
    let base = |name: &str| db.sum(ModType::Base, cfg, &[ModName::from(name)]);
    let base_mana = db
        .override_(cfg, "BaseManaCostOverride")
        .unwrap_or_else(|| base("SkillManaCostBase"))
        + base("ManaCostBase")
        + cost_from_maximum_pool(db, cfg, "Mana")
        + cfg.stat("ManaUnreserved") * base("SkillUnreservedManaPercentCostBase") / 100.0;
    let base_life = db
        .override_(cfg, "BaseLifeCostOverride")
        .unwrap_or_else(|| base("SkillLifeCostBase"))
        + base("LifeCostBase")
        + cost_from_maximum_pool(db, cfg, "Life");
    let has_mana_cost = base_mana > 0.0 || unscaled_cost(db, cfg, "Mana") > 0.0;
    let hybrid = hybrid_life_cost_share(db, cfg);
    let mana = if has_mana_cost {
        let mana = calc_mana_cost(db, cfg, base_mana).final_cost;
        // The share converted to Life is no longer paid in Mana.
        if hybrid > 0.0 {
            ((1.0 - hybrid) * mana).floor().max(0.0)
        } else {
            mana
        }
    } else {
        0.0
    };
    let mana_becomes_life = hybrid > 0.0 || base_mana_cost_as_life_share(db, cfg) > 0.0;
    let has_life_cost = base_life > 0.0
        || unscaled_cost(db, cfg, "Life") > 0.0
        || (mana_becomes_life && has_mana_cost);
    let life = if has_life_cost {
        calc_life_cost_hybrid(db, cfg, base_life, base_mana).final_cost
    } else {
        0.0
    };
    SkillCosts { mana, life }
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
    let base_as_life = base_mana_cost_as_life_share(db, cfg);
    if hybrid <= 0.0 && base_as_life <= 0.0 {
        return calc_life_cost(db, cfg, base_life_cost);
    }
    if db.flag(cfg, "HasNoCost") {
        return SkillCostResult {
            kind: SkillCostKind::Life,
            base_cost: base_life_cost,
            final_cost: 0.0,
            no_cost: true,
        };
    }
    let mult = support_cost_multiplier(db, cfg);
    // The whole Mana cost converts; only the gem's own, as its supports
    // scale it, is also gained as a base Life cost.
    let scaled_base_mana = (base_mana_cost * mult).floor();
    let mana_final_base = (base_mana_cost * mult + unscaled_cost(db, cfg, "Mana")).floor();
    let life_final_base = (base_life_cost * mult
        + unscaled_cost(db, cfg, "Life")
        + (mana_final_base * hybrid).round()
        + scaled_base_mana * base_as_life)
        .round();
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
    if db.flag(cfg, "HasNoCost") {
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

/// Skill effect durations share the general Duration modifiers; the first
/// two channels also read their own PrimaryDuration / SecondaryDuration.
/// The base_*_skill_effect_duration game stats arrive in seconds.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SkillDurations {
    pub primary: f64,
    pub secondary: f64,
    pub aura: f64,
    pub reservation: f64,
}

/// The exact duration terms used by both calculation and the detail panel.
#[derive(Debug, Clone, Copy)]
pub struct DurationTerms {
    pub base: f64,
    pub modifier: f64,
    pub expiration: f64,
    pub tick: f64,
}
impl DurationTerms {
    pub fn value(self) -> f64 {
        if self.base <= 0.0 {
            0.0
        } else {
            round((self.base * self.modifier * self.expiration / self.tick).ceil() * self.tick)
        }
    }
}
/// buff_time_passed_+%_only_buff_category: slower elapsed time extends buffs.
pub fn buff_expiration_mult(db: &ModDb, cfg: &CalcConfig) -> f64 {
    let rate = 1.0 + db.sum(ModType::Inc, cfg, &[ModName::from("BuffExpiryRate")]) / 100.0;
    // A zero expiry rate has no finite duration; use the same finite 75% slow
    // ceiling as other effect-expiry calculations.
    1.0 / rate.max(cfg.constants.game().buff_expiration_slow_cap)
}

pub fn duration_terms(db: &ModDb, enemy: &ModDb, cfg: &CalcConfig, stat: &str) -> DurationTerms {
    let (base_name, scope, debuff) = match stat {
        "Duration" => (
            "SkillDurationBase",
            Some("PrimaryDuration"),
            "SkillDurationIsDebuff",
        ),
        "DurationSecondary" => (
            "SkillDurationSecondaryBase",
            Some("SecondaryDuration"),
            "SkillDurationSecondaryIsDebuff",
        ),
        "AuraDuration" => ("SkillAuraDurationBase", None, "SkillAuraDurationIsDebuff"),
        _ => (
            "SkillReserveDurationBase",
            None,
            "SkillReserveDurationIsDebuff",
        ),
    };
    let mut names = vec![ModName::from("Duration")];
    if let Some(scope) = scope {
        names.push(ModName::from(scope));
    }
    let base = db.sum(ModType::Base, cfg, &[ModName::from(base_name)])
        + if scope.is_some() {
            db.sum(ModType::Base, cfg, &names)
        } else {
            0.0
        };
    DurationTerms {
        base,
        modifier: ((1.0 + db.sum(ModType::Inc, cfg, &names) / 100.0) * db.more(cfg, &names))
            .max(0.0),
        expiration: if db.flag(cfg, debuff) {
            super::ailment::debuff_duration_mult(enemy, cfg)
        } else if cfg
            .skill_types
            .intersects(SkillTypes::from_pob2_name("Buff").unwrap())
        {
            buff_expiration_mult(db, cfg)
        } else {
            1.0
        },
        tick: cfg.constants.game().server_tick_seconds,
    }
}
pub fn skill_durations(db: &ModDb, enemy: &ModDb, cfg: &CalcConfig) -> SkillDurations {
    SkillDurations {
        primary: duration_terms(db, enemy, cfg, "Duration").value(),
        secondary: duration_terms(db, enemy, cfg, "DurationSecondary").value(),
        aura: duration_terms(db, enemy, cfg, "AuraDuration").value(),
        reservation: duration_terms(db, enemy, cfg, "ReserveDuration").value(),
    }
}

/// Available projectile interactions. Fractional chance-based counts are
/// averages, while the pure resolver determines which interactions can occur.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ProjectileCounts {
    pub pierce: f64,
    pub fork: f64,
    pub chain: f64,
    pub split: f64,
    pub bounce: f64,
}

pub fn projectile_behavior_counts(db: &ModDb, enemy: &ModDb, cfg: &CalcConfig) -> ProjectileCounts {
    let sum = |name: &str| db.sum(ModType::Base, cfg, &[ModName::from(name)]);
    let flag = |name: &str| db.flag(cfg, name);
    let projectile = cfg.flags.intersects(ModFlags::PROJECTILE) || sum("ProjectileCount") > 0.0;
    // Conversions use additional projectiles even when the launched count is
    // locked to one by "modifiers ... instead apply to chaining/splitting".
    let additional = ((sum("ProjectileCount")
        + 2.0 * sum("TwoAdditionalProjectilesChance") / 100.0
        + sum("SurpassingProjectileChance") / 100.0
        - 1.0)
        * db.more(cfg, &[ModName::from("ProjectileCount")]))
    .max(0.0);
    let chain_names = if projectile {
        vec![ModName::from("ChainCountMax")]
    } else {
        vec![
            ModName::from("ChainCountMax"),
            ModName::from("BeamChainCountMax"),
        ]
    };
    let chain = ((db.sum(ModType::Base, cfg, &chain_names) + sum("ChainChance") / 100.0)
        * db.more(cfg, &chain_names)
        + if flag("AdditionalProjectilesAddChainsInstead") {
            additional
        } else {
            0.0
        })
    .max(0.0);
    let split = (sum("SplitCount")
        + enemy.sum(ModType::Base, cfg, &[ModName::from("SelfSplitCount")])
        + if flag("AdditionalProjectilesAddSplitsInstead") {
            additional
        } else {
            0.0
        }
        + if flag("AdditionalChainsAddSplitsInstead") {
            sum("ChainCountMax")
        } else {
            0.0
        })
    .max(0.0);
    let pierce = (sum("PierceCount") + sum("PierceChance") / 100.0).max(0.0);
    let projectile_number = SkillTypes::from_pob2_name("ProjectileNumber")
        .is_some_and(|kind| cfg.skill_types.intersects(kind));
    let input = ProjectileBehaviorInput {
        cannot_split: !projectile || flag("CannotSplit") || projectile_number,
        split_count: split.ceil() as u32,
        cannot_pierce: !projectile || flag("CannotPierce"),
        pierce_all_targets: flag("PierceAllTargets") || enemy.flag(cfg, "AlwaysPierceSelf"),
        pierce_count: pierce.ceil() as u32,
        cannot_fork: !projectile || flag("CannotFork"),
        fork_once: flag("ForkOnce"),
        fork_twice: flag("ForkTwice"),
        fork_count_max: sum("ForkCountMax").max(0.0) as u32,
        cannot_chain: flag("CannotChain") || flag("NoAdditionalChains"),
        chain_count_max: chain.ceil() as u32,
        ..ProjectileBehaviorInput::default()
    };
    let resolved = resolve_projectile_behavior(&input, 0);
    ProjectileCounts {
        pierce: if resolved.effective_pierce_all {
            100.0
        } else if resolved.pierce_count > 0 {
            pierce
        } else {
            0.0
        },
        fork: resolved.fork_count_max as f64,
        chain: if resolved.chain_count_max > 0 {
            chain
        } else {
            0.0
        },
        split: if resolved.split_count > 0 { split } else { 0.0 },
        bounce: if projectile {
            (sum("BounceCount") * db.more(cfg, &[ModName::from("ProjectileCount")])
                + if flag("AdditionalProjectilesAddBouncesInstead") {
                    additional
                } else {
                    0.0
                })
            .max(0.0)
        } else {
            0.0
        },
    }
}

/// Highest requirement among equipment and active gems; support gems require
/// five of their attribute each across the build. Global gem modifiers do not
/// change this cumulative support requirement.
pub fn attribute_requirements(db: &ModDb, cfg: &CalcConfig) -> [f64; 3] {
    attribute_requirement_terms(db, cfg).map(|values| values.into_iter().fold(0.0, f64::max))
}

/// Adjusted category requirements, after substitutions and ignored requirements.
pub fn attribute_requirement_terms(db: &ModDb, cfg: &CalcConfig) -> [[f64; 6]; 3] {
    if db.flag(cfg, "IgnoreAttributeRequirements") {
        return [[0.0; 6]; 3];
    }
    let mut global = cfg.clone();
    global.flags = ModFlags::NONE;
    global.keyword_flags = KeywordFlags::NONE;
    let stat = |name: &str| cfg.stats.get(name).copied().unwrap_or(0.0);
    let highest = ["Strength", "Dexterity", "Intelligence"]
        .map(stat)
        .into_iter()
        .fold(0.0, f64::max);
    let highest_satisfies = db.flag(
        &global,
        "GemAttributeRequirementsSatisfiedByHighestAttribute",
    );
    let strength_satisfies = db.flag(&global, "StrengthSatisfiesMeleeWeaponsAndSkills");
    ["Str", "Dex", "Int"].map(|attr| {
        let mut result = [0.0; 6];
        result[0] = stat(&format!("SupportRequirement{attr}"));
        for (index, category) in ["Item", "Weapon", "MeleeWeapon", "Gem", "MeleeGem"]
            .into_iter()
            .enumerate()
        {
            let gem = category.ends_with("Gem");
            let mut names = vec![
                ModName::from("GlobalAttributeRequirements"),
                ModName::from(if gem {
                    "GlobalGemAttributeRequirements"
                } else {
                    "GlobalItemAttributeRequirements"
                }),
            ];
            if category.ends_with("Weapon") {
                names.push(ModName::from("GlobalWeaponAttributeRequirements"));
            }
            let modifier = ((1.0 + db.sum(ModType::Inc, &global, &names) / 100.0)
                * db.more(&global, &names))
            .max(0.0);
            let requirement = (stat(&format!("{category}Requirement{attr}")) * modifier).floor();
            if (gem && highest_satisfies && highest >= requirement)
                || (category.starts_with("Melee")
                    && strength_satisfies
                    && attr != "Str"
                    && stat("Strength") >= requirement)
            {
                continue;
            }
            result[index + 1] = requirement;
        }
        result
    })
}

/// Cost channels in Mana, Life, Energy Shield, Rage, Ward order.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ResourceCosts {
    pub upfront: [f64; 5],
    pub percent: [f64; 5],
    pub per_second: [f64; 5],
    pub percent_per_second: [f64; 5],
}

pub fn resource_costs(db: &ModDb, cfg: &CalcConfig) -> ResourceCosts {
    let mut costs = ResourceCosts::default();
    if db.flag(cfg, "HasNoCost") {
        return costs;
    }
    let sum = |name: &str| db.sum(ModType::Base, cfg, &[ModName::from(name)]);
    let mult = support_cost_multiplier(db, cfg);
    for (suffix, output) in [
        ("", &mut costs.upfront),
        ("Percent", &mut costs.percent),
        ("PerSecond", &mut costs.per_second),
        ("PercentPerSecond", &mut costs.percent_per_second),
    ] {
        let mut bases = [0.0; 5];
        let mut raw_bases = [0.0; 5];
        for (i, resource) in ["Mana", "Life", "ES", "Rage", "Ward"]
            .into_iter()
            .enumerate()
        {
            let key = format!("{resource}{suffix}");
            let mut base = db
                .override_(cfg, &format!("Base{key}CostOverride"))
                .unwrap_or_else(|| sum(&format!("Skill{key}CostBase")))
                + sum(&format!("{key}CostBase"));
            if suffix.is_empty() && i < 2 {
                base += cost_from_maximum_pool(db, cfg, resource);
            }
            if suffix.is_empty() && i == 0 {
                base +=
                    cfg.stat("ManaUnreserved") * sum("SkillUnreservedManaPercentCostBase") / 100.0;
            }
            raw_bases[i] = base * mult;
            bases[i] = (raw_bases[i] + unscaled_cost(db, cfg, &key)).floor();
        }
        let hybrid = hybrid_life_cost_share(db, cfg);
        bases[1] += (bases[0] * hybrid).round()
            + raw_bases[0].floor() * base_mana_cost_as_life_share(db, cfg);
        bases[2] += (bases[0] * sum("ManaCostAsEnergyShieldCost") / 100.0).round();
        if suffix.is_empty() {
            bases[4] += (bases[0] * sum("WardCostAsPercentOfManaCost") / 100.0).round()
                + (bases[1] * sum("WardCostAsPercentOfLifeCost") / 100.0).round();
        }
        for (i, resource) in ["Mana", "Life", "ES", "Rage", "Ward"]
            .into_iter()
            .enumerate()
        {
            let flat = if suffix.is_empty() || suffix == "Percent" {
                sum(&format!("{resource}{suffix}Cost")) + sum("Cost")
            } else {
                0.0
            };
            output[i] = (apply_cost_chain(db, cfg, resource, bases[i]) + flat).max(0.0);
            if i == 0 && hybrid > 0.0 {
                output[i] = (output[i] * (1.0 - hybrid)).floor();
            }
        }
    }
    // Keep the established Mana/Life rounding and conversion behavior. Their
    // base channel is also used before damage for "per Life cost" modifiers.
    let basic = skill_costs(db, cfg);
    costs.upfront[0] = basic.mana;
    costs.upfront[1] = basic.life;
    costs
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SkillTimings {
    pub totem: f64,
    pub trap: f64,
    pub trap_count: f64,
    pub mine: f64,
    pub mine_count: f64,
    pub warcry: f64,
    pub channel: f64,
}

/// Placement and throwing speed apply to the character's use, independently
/// of the proxy's hit rate. A channel's displayed time excludes its release.
pub fn skill_timings(db: &ModDb, cfg: &CalcConfig, base_use_time: f64) -> SkillTimings {
    let sum = |name: &str| db.sum(ModType::Base, cfg, &[ModName::from(name)]);
    let modifier = |names: &[ModName]| {
        ((1.0 + db.sum(ModType::Inc, cfg, names) / 100.0) * db.more(cfg, names)).max(0.0)
    };
    let action = super::skill_use_time::action_speed_factor(db, cfg);
    let time = |base_name: &str, time_name: &str, speed_name: &str, time_mod: Option<&str>| {
        let base = sum(base_name) + sum(time_name);
        if base <= 0.0 || action <= 0.0 {
            return 0.0;
        }
        let speed = modifier(&[ModName::from(speed_name)]) * action;
        if speed <= 0.0 {
            return 0.0;
        }
        let time_factor = time_mod.map_or(1.0, |name| modifier(&[ModName::from(name)]));
        round((base * time_factor / speed).max(cfg.constants.game().server_tick_seconds))
    };
    let count = |name: &str| {
        db.override_(cfg, name)
            .unwrap_or_else(|| (1.0 + sum(name)) * modifier(&[ModName::from(name)]))
            .max(0.0)
    };
    let trap_count = count("TrapThrowCount");
    let mine_count = count("MineThrowCount");
    let trap = time(
        "SkillTrapThrowingTimeBase",
        "TrapThrowingTime",
        "TrapThrowingSpeed",
        Some("SkillTrapThrowingTime"),
    );
    let mine = time(
        "SkillMineLayingTimeBase",
        "MineLayingTime",
        "MineLayingSpeed",
        Some("SkillMineThrowingTime"),
    );
    let minimum = sum("ChannelMinimumTime").max(sum(super::flameblast::MIN_CHANNEL) / 1000.0);
    let channel = if minimum > 0.0
        || sum("SkillChannelStages") > 0.0
        || sum("SkillChannelStageTimeOverride") > 0.0
    {
        let speed = modifier(&super::skill_use_time::speed_names_for(cfg)) * action;
        let stage = if sum("SkillChannelStageTimeOverride") > 0.0 {
            sum("SkillChannelStageTimeOverride")
        } else if speed > 0.0 {
            base_use_time / speed
        } else {
            0.0
        };
        let stages = if sum(super::flameblast::MAX_STAGES) > 0.0 {
            cfg.multiplier_if_set(super::flameblast::STAGE)
                .unwrap_or(1.0)
        } else {
            sum("SkillChannelStages").max(1.0)
        };
        round((stage * stages).max(minimum))
    } else {
        0.0
    };
    SkillTimings {
        totem: time(
            "SkillTotemPlacementTimeBase",
            "TotemPlacementTime",
            "TotemPlacementSpeed",
            None,
        ),
        trap,
        trap_count: if trap > 0.0 { trap_count } else { 0.0 },
        mine,
        mine_count: if mine > 0.0 { mine_count } else { 0.0 },
        warcry: if db.flag(cfg, "InstantWarcry") {
            0.0
        } else {
            time(
                "SkillWarcryCastTimeBase",
                "WarcryCastTime",
                "WarcrySpeed",
                None,
            )
        },
        channel,
    }
}
