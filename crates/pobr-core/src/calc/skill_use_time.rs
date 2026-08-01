//! Skill use time and action rate (08-mechanics §2.1, §3.2; `agent-docs/skill-speed.md`).
//!
//! - AttackSpeed / CastSpeed / SkillSpeed all belong to one additive Inc speed bucket.
//! - ActionSpeed is a separate multiplicative factor applied to the final rate.
//! - `+# seconds to use time` penalties are added after the speed adjustment and are not scaled by speed.
//! - The effective rate of non-channelling actions is capped by the server tick rate (~30.3 actions/s).

use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb, TraceGraph, TraceNodeId, TraceOperation, TracedValue};

use super::round;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkillUseTime {
    pub base_use_time: f64,
    /// Total from the additive speed bucket (%).
    pub total_skill_speed: f64,
    /// Total from the independent action speed factor (%).
    pub total_action_speed: f64,
    pub total_use_time_penalty: f64,
    pub tooltip_use_time: f64,
    pub tooltip_rate: f64,
    pub effective_rate: f64,
    pub capped_by_server_tick: bool,
}

/// The additive Inc / multiplicative More speed bucket: AttackSpeed / CastSpeed
/// / SkillSpeed all share one speed factor (PoB CalcOffence: `inc`/`more`
/// sums/multiplies all three together). ActionSpeed is not part of it — it is
/// a separate factor, multiplied in on its own (see [`action_speed_name`]).
pub const SPEED_BUCKET: [&str; 3] = ["AttackSpeed", "CastSpeed", "SkillSpeed"];

/// Stat name for the independent ActionSpeed factor (kept apart from the
/// speed bucket, multiplied into the final rate separately).
pub const ACTION_SPEED: &str = "ActionSpeed";

/// The speed bucket as a [`ModName`] array (for [`ModDb::sum`]/[`ModDb::more`]
/// aggregation). Contains all three members — used for the "use time" display
/// (attacks and spells each only have one non-zero side).
pub fn speed_names() -> [ModName; 3] {
    [
        ModName::from(SPEED_BUCKET[0]),
        ModName::from(SPEED_BUCKET[1]),
        ModName::from(SPEED_BUCKET[2]),
    ]
}

/// Picks the speed bucket by skill type: attack → `[AttackSpeed, SkillSpeed]`,
/// spell → `[CastSpeed, SkillSpeed]`, neither (e.g. companion/minion/reservation
/// main skills) → just `[SkillSpeed]`.
///
/// Source: PoB CalcOffence — attacks only benefit from attack speed, spells
/// only from cast speed, and `SkillSpeed` applies to both; they must not be
/// conflated (an attack must not pick up `increased Cast Speed`, nor a spell
/// `increased Attack Speed`). For unflagged skills: vendor's `Speed` mod
/// carries ModFlag.Attack/Cast, and flag matching requires cfg to have the
/// corresponding bit set — a cfg that is neither attack nor spell picks up
/// neither (confirmed on Wolf Pack: its main skill once incorrectly picked up
/// the weapon's `12% reduced Attack Speed`, giving Speed 0.88 vs vendor's 1.00).
pub fn speed_names_for(cfg: &CalcConfig) -> Vec<ModName> {
    // Attack/spell detection accepts either ModFlags (injected by the
    // orchestrator via `skill_type_flags`) or SkillTypes
    // (`CalcConfig::attack()`/`spell()` presets) — either one matching is
    // enough, to be compatible with both assembly paths.
    let is_attack = cfg.flags.intersects(ModFlags::ATTACK) || cfg.is_attack();
    let is_spell = cfg.flags.intersects(ModFlags::SPELL) || cfg.is_spell();
    let mut names = Vec::with_capacity(3);
    if is_attack {
        names.push(ModName::from(SPEED_BUCKET[0])); // AttackSpeed
    }
    if is_spell {
        names.push(ModName::from(SPEED_BUCKET[1])); // CastSpeed
    }
    names.push(ModName::from(SPEED_BUCKET[2])); // SkillSpeed (always)
    names
}

/// Calculates skill use time and effective action rate.
pub fn calc_skill_use_time(
    db: &ModDb,
    cfg: &CalcConfig,
    base_use_time: f64,
    use_time_penalty: f64,
    is_channelling: bool,
) -> SkillUseTime {
    let total_skill_speed = db.sum(ModType::Inc, cfg, &speed_names());
    let total_action_speed = db.sum(ModType::Inc, cfg, &[ModName::from(ACTION_SPEED)]);

    let tooltip_use_time = if base_use_time > 0.0 {
        base_use_time / (1.0 + total_skill_speed / 100.0) + use_time_penalty
    } else {
        use_time_penalty
    };
    let tooltip_rate = if tooltip_use_time > 0.0 {
        1.0 / tooltip_use_time
    } else {
        0.0
    };

    let action_factor = 1.0 + total_action_speed / 100.0;
    let uncapped_rate = tooltip_rate * action_factor;

    //  Server tick time now reads from the injected constants pack (fallback == old const, value unchanged).
    let server_rate = 1.0 / cfg.constants.game().server_tick_seconds;
    let (effective_rate, capped_by_server_tick) = if !is_channelling && uncapped_rate > server_rate
    {
        (server_rate, true)
    } else {
        (uncapped_rate, false)
    };

    SkillUseTime {
        base_use_time,
        total_skill_speed: round(total_skill_speed),
        total_action_speed: round(total_action_speed),
        total_use_time_penalty: use_time_penalty,
        tooltip_use_time: round(tooltip_use_time),
        tooltip_rate: round(tooltip_rate),
        effective_rate: round(effective_rate),
        capped_by_server_tick,
    }
}

// Crossbow reload model (line-by-line mirror of PoB2 CalcOffence.lua:2867-2897)

/// Crossbow reload calculation result (vendor `output.FiringRate/EffectiveBoltCount/
/// TotalFiringTime/EffectiveReloadTime/Speed`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CrossbowReload {
    /// Firing rate before reload folding (= the attack rate after the server
    /// tick cap, vendor `FiringRate = Speed`).
    pub firing_rate: f64,
    /// Effective magazine size (`boltCount / (1 − ChanceToNotConsumeAmmo/100)`;
    /// chance ≥100 → ∞, i.e. never reloads).
    pub effective_bolt_count: f64,
    /// Time to empty the magazine (seconds; 0 when chance ≥100).
    pub total_firing_time: f64,
    /// Effective reload time (seconds; `reload × (1 − InstantReloadChance/100)`).
    pub effective_reload_time: f64,
    /// Average rate over a full firing cycle (actions/s; vendor writes this
    /// back to `output.Speed`).
    pub effective_rate: f64,
}

/// Crossbow reload cycle-average firing rate (line-by-line mirror of vendor
/// `CalcOffence.lua:2867-2887`):
///
/// - `EffectiveBoltCount = boltCount / (1 − c/100)` (`c = ChanceToNotConsumeAmmo`;
///   `c ≥ 100` → ∞, degenerating to pure firing rate);
/// - `TotalFiringTime = EffectiveBoltCount / FiringRate`;
/// - `EffectiveReloadTime = reloadTime × (1 − min(InstantReloadChance,100)/100)`;
/// - `Speed = EffectiveBoltCount / (TotalFiringTime + EffectiveReloadTime)`.
///
/// Ordering convention (vendor `:2864-2867`): the server tick cap is applied
/// before reload folding — the caller's `firing_rate` argument must already
/// be the post-cap rate.
/// `Multiplier:BoltsReloadedPastSix/EightSeconds` write-back (Fresh Clip
/// support) depends on ReplaceMod and is not done this pass (tracked separately).
pub fn apply_crossbow_reload(
    firing_rate: f64,
    bolt_count: f64,
    reload_time_s: f64,
    chance_not_consume_pct: f64,
    instant_reload_pct: f64,
) -> CrossbowReload {
    // vendor `:319`: magazine size floors at 1 (`m_max(Sum(...), 1)`).
    let bolt_count = bolt_count.max(1.0);
    let instant = instant_reload_pct.clamp(0.0, 100.0);
    let effective_reload_time = reload_time_s * (1.0 - instant / 100.0);
    if chance_not_consume_pct >= 100.0 || firing_rate <= 0.0 {
        // Ammo is never consumed (vendor's `1 / 0 = ∞` branch) → Speed =
        // FiringRate; when firing_rate is 0 the cycle has no meaning, so
        // return it as-is.
        return CrossbowReload {
            firing_rate,
            effective_bolt_count: f64::INFINITY,
            total_firing_time: 0.0,
            effective_reload_time,
            effective_rate: firing_rate,
        };
    }
    let effective_bolt_count = if chance_not_consume_pct > 0.0 {
        bolt_count / (1.0 - chance_not_consume_pct / 100.0)
    } else {
        bolt_count
    };
    let total_firing_time = effective_bolt_count / firing_rate;
    let effective_rate = effective_bolt_count / (total_firing_time + effective_reload_time);
    CrossbowReload {
        firing_rate,
        effective_bolt_count,
        total_firing_time,
        effective_reload_time,
        effective_rate,
    }
}

/// Crossbow reload time folding (vendor `calcCrossbowReloadTime`,
/// `CalcOffence.lua:283-291`): `reload = base /
/// calcLib.mod(skillModList, cfg, "ReloadSpeed", "Speed")` — `Speed`'s and
/// `ReloadSpeed`'s INC values share a bucket (summed) and MORE values are
/// multiplied together to form the divisor (attack speed bonuses also speed
/// up reloading). `Speed` expands to pobr's speed bucket names
/// (see [`speed_names_for`]).
pub fn crossbow_reload_time(db: &crate::ModDb, cfg: &CalcConfig, base_reload_s: f64) -> f64 {
    let mut names = vec![ModName::from("ReloadSpeed")];
    names.extend(speed_names_for(cfg));
    let multi = (1.0 + db.sum(ModType::Inc, cfg, &names) / 100.0) * db.more(cfg, &names);
    if multi <= 0.0 {
        return base_reload_s;
    }
    base_reload_s / multi
}

/// Traced version of `calc_skill_use_time`: records nodes for the speed bucket, action speed, and final rate.
pub fn calc_skill_use_time_traced(
    db: &ModDb,
    cfg: &CalcConfig,
    base_use_time: f64,
    use_time_penalty: f64,
    is_channelling: bool,
    trace: &mut TraceGraph,
) -> (SkillUseTime, TraceNodeId) {
    let result = calc_skill_use_time(db, cfg, base_use_time, use_time_penalty, is_channelling);

    let base_node = trace.add_source_node(
        "base use time",
        base_use_time,
        SourceId::new(SourceKind::CharacterBase, "base.UseTime"),
    );
    let speed_bucket = db.sum_traced(
        ModType::Inc,
        cfg,
        &speed_names(),
        trace,
        "skill speed bucket (Attack/Cast/Skill)",
    );
    let action_speed = db.sum_traced(
        ModType::Inc,
        cfg,
        &[ModName::from(ACTION_SPEED)],
        trace,
        "action speed (independent)",
    );
    let rate_node = trace.add_node(
        "effective action rate",
        result.effective_rate,
        TraceOperation::Multiply,
    );
    trace.add_edge(base_node, rate_node);
    trace.add_edge(speed_bucket.node_id, rate_node);
    trace.add_edge(action_speed.node_id, rate_node);

    (result, rate_node)
}

/// Convenience wrapper returning a traced value.
pub fn calc_skill_use_time_traced_value(
    db: &ModDb,
    cfg: &CalcConfig,
    base_use_time: f64,
    use_time_penalty: f64,
    is_channelling: bool,
    trace: &mut TraceGraph,
) -> TracedValue {
    let (result, node_id) = calc_skill_use_time_traced(
        db,
        cfg,
        base_use_time,
        use_time_penalty,
        is_channelling,
        trace,
    );
    TracedValue {
        value: result.effective_rate,
        node_id,
    }
}

#[cfg(test)]
mod crossbow_reload_tests {
    use super::*;
    use crate::Modifier;

    /// Hand-computed case: boltCount=5, reload=0.8s, FiringRate=3 →
    /// EffectiveBoltCount=5, TotalFiringTime=5/3≈1.6667,
    /// Speed = 5 / (1.6667 + 0.8) ≈ 2.0270.
    #[test]
    fn manual_case_bolt5_reload08_rate3() {
        let r = apply_crossbow_reload(3.0, 5.0, 0.8, 0.0, 0.0);
        assert!((r.effective_bolt_count - 5.0).abs() < 1e-9);
        assert!((r.total_firing_time - 5.0 / 3.0).abs() < 1e-9);
        assert!((r.effective_reload_time - 0.8).abs() < 1e-9);
        assert!(
            (r.effective_rate - 5.0 / (5.0 / 3.0 + 0.8)).abs() < 1e-9,
            "{r:?}"
        );
        assert!((r.effective_rate - 2.027027).abs() < 1e-5, "{r:?}");
    }

    /// ChanceToNotConsumeAmmo ≥ 100: degenerates to pure firing rate (vendor's infinite-magazine branch).
    #[test]
    fn not_consume_ammo_100_degenerates_to_firing_rate() {
        let r = apply_crossbow_reload(3.0, 5.0, 0.8, 100.0, 0.0);
        assert_eq!(r.effective_rate, 3.0);
        assert!(r.effective_bolt_count.is_infinite());
        assert_eq!(r.total_firing_time, 0.0);
    }

    /// ChanceToNotConsumeAmmo 50%: doubles the effective magazine, diluting the reload cost.
    #[test]
    fn partial_not_consume_extends_magazine() {
        let r = apply_crossbow_reload(3.0, 5.0, 0.8, 50.0, 0.0);
        assert!((r.effective_bolt_count - 10.0).abs() < 1e-9);
        // 10 / (10/3 + 0.8) ≈ 2.4194
        assert!(
            (r.effective_rate - 10.0 / (10.0 / 3.0 + 0.8)).abs() < 1e-9,
            "{r:?}"
        );
    }

    /// InstantReloadChance 100%: reload time drops to zero, Speed = FiringRate.
    #[test]
    fn instant_reload_100_removes_reload_cost() {
        let r = apply_crossbow_reload(3.0, 5.0, 0.8, 0.0, 100.0);
        assert_eq!(r.effective_reload_time, 0.0);
        assert!((r.effective_rate - 3.0).abs() < 1e-9);
    }

    /// Magazine size floors at 1 (vendor `:319` m_max; a conservative cycle when bolt data is missing).
    #[test]
    fn bolt_count_floors_to_one() {
        let r = apply_crossbow_reload(3.0, 0.0, 0.8, 0.0, 0.0);
        assert!((r.effective_bolt_count - 1.0).abs() < 1e-9);
        // 1 / (1/3 + 0.8) ≈ 0.8824
        assert!(
            (r.effective_rate - 1.0 / (1.0 / 3.0 + 0.8)).abs() < 1e-9,
            "{r:?}"
        );
    }

    /// Reload folding picks up both ReloadSpeed and the attack speed bucket (vendor calcLib.mod("ReloadSpeed","Speed")).
    #[test]
    fn reload_time_scales_with_reload_and_attack_speed() {
        let mut db = crate::ModDb::new();
        db.add_list([
            Modifier::number("ReloadSpeed", ModType::Inc, 30.0),
            Modifier::number("AttackSpeed", ModType::Inc, 20.0),
        ]);
        let cfg = CalcConfig::attack();
        let t = crossbow_reload_time(&db, &cfg, 0.8);
        assert!((t - 0.8 / 1.5).abs() < 1e-9, "INC 同桶相加：{t}");
    }
}
