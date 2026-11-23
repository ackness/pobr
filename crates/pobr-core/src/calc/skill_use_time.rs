//! Skill use time and action rate (08-mechanics §2.1, §3.2; `agent-docs/skill-speed.md`).
//!
//! - AttackSpeed / CastSpeed / SkillSpeed all belong to one additive Inc speed bucket.
//! - ActionSpeed is a separate multiplicative factor applied to the final rate.
//! - `+# seconds to use time` penalties are added after the speed adjustment and are not scaled by speed.
//! - The effective rate of non-channelling actions is capped by the server tick rate (~30.3 actions/s).

use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb};

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
/// a separate factor, multiplied in on its own (see [`action_speed_factor`]).
pub const SPEED_BUCKET: [&str; 3] = ["AttackSpeed", "CastSpeed", "SkillSpeed"];

/// Stat name for the independent ActionSpeed factor (kept apart from the
/// speed bucket, multiplied into the final rate separately).
pub const ACTION_SPEED: &str = "ActionSpeed";

/// Action-speed bonuses and the generic slows represented by negative
/// ActionSpeed modifiers. The game's Slow keyword says that separate slows
/// multiply (https://poe2db.tw/us/Keywords). Slowing potency scales each
/// penalty; it never scales positive speed bonuses or equipment penalties.
/// `base_slow_potency_+%` is the potency of debuffs on this actor, and
/// `slows_have_no_potency_on_you` makes their slowing component zero.
pub fn action_speed_factor(db: &ModDb, cfg: &CalcConfig) -> f64 {
    let names = [ACTION_SPEED];
    let increases = db.contributions_by_name(ModType::Inc, cfg, &names);
    let multipliers = db.contributions_by_name(ModType::More, cfg, &names);
    let has_slow = increases
        .iter()
        .chain(&multipliers)
        .any(|row| row.value < 0.0);
    if !has_slow {
        return ((1.0 + db.sum_by_name(ModType::Inc, cfg, &names) / 100.0)
            * db.more_by_name(cfg, &names))
        .max(0.0);
    }
    let potency = if db.flag(cfg, "UnaffectedBySlows") {
        0.0
    } else {
        let effect = ["SlowEffectOnSelf"];
        ((1.0 + db.sum_by_name(ModType::Inc, cfg, &effect) / 100.0) * db.more_by_name(cfg, &effect))
            .max(0.0)
    };
    let mut increase = 0.0;
    let mut more = 1.0;
    let mut slows = 1.0;
    for row in increases.into_iter().chain(multipliers) {
        let value = row.value;
        if value < 0.0 {
            slows *= (1.0 + value * potency / 100.0).max(0.0);
        } else if row.mod_type == ModType::Inc {
            increase += value;
        } else {
            more *= 1.0 + value / 100.0;
        }
    }
    // Preserve the ordinary MORE bucket's precision, without rounding the
    // independent slowing factors into that bucket.
    more = if let Some(precision) = db.more_precision_for(ACTION_SPEED) {
        let scale = 10f64.powi(precision as i32);
        (more * scale).floor() / scale
    } else {
        (more * 100.0).round() / 100.0
    };
    (1.0 + increase / 100.0) * more * slows
}

/// The speed bucket as a [`ModName`] array (for [`ModDb::sum`]/[`ModDb::more`]
/// aggregation). Contains every family; use [`speed_names_for`] when
/// calculating a particular skill.
pub fn speed_names() -> [ModName; 3] {
    [
        ModName::from(SPEED_BUCKET[0]),
        ModName::from(SPEED_BUCKET[1]),
        ModName::from(SPEED_BUCKET[2]),
    ]
}

/// Zero-allocation static slice behind [`speed_names_for`].
pub(crate) fn speed_names_slice(cfg: &CalcConfig) -> &'static [&'static str] {
    if cfg.skill_types.intersects(SkillTypes::WARCRY) {
        return &["WarcrySpeed"];
    }
    // Attack/spell detection accepts either ModFlags (injected by the
    // orchestrator via `skill_type_flags`) or SkillTypes
    // (`CalcConfig::attack()`/`spell()` presets) — either one matching is
    // enough, to be compatible with both assembly paths.
    let is_attack = cfg.flags.intersects(ModFlags::ATTACK) || cfg.is_attack();
    let is_spell = cfg.flags.intersects(ModFlags::SPELL) || cfg.is_spell();
    let separate_timing = cfg
        .skill_types
        .intersects(SkillTypes::SUMMONS_TOTEM | SkillTypes::TRAPPED | SkillTypes::REMOTE_MINED);
    let include_cast = is_spell || (!is_attack && !separate_timing);
    match (is_attack, include_cast) {
        (true, true) => &SPEED_BUCKET,
        (true, false) => &[SPEED_BUCKET[0], SPEED_BUCKET[2]],
        (false, true) => &[SPEED_BUCKET[1], SPEED_BUCKET[2]],
        (false, false) => &[SPEED_BUCKET[2]],
    }
}

/// Attacks use AttackSpeed; other skills with a cast time use CastSpeed,
/// even without the Spell type (e.g. AncientGiftsPlayer's 1000 ms cast).
/// Warcries use WarcrySpeed, matching their separate timing calculation.
/// Generic Skill Speed is already fanned out to that name by the parser,
/// stat map and Mageblood; summing SkillSpeed too would count it twice.
/// Proxy placement must not acquire cast speed through this fallback.
/// For ordinary attacks and casts, SkillSpeed shares the additive bucket.
pub fn speed_names_for(cfg: &CalcConfig) -> Vec<ModName> {
    speed_names_slice(cfg)
        .iter()
        .copied()
        .map(ModName::from)
        .collect()
}

/// Calculates skill use time and effective action rate.
pub fn calc_skill_use_time(
    db: &ModDb,
    cfg: &CalcConfig,
    base_use_time: f64,
    use_time_penalty: f64,
    is_channelling: bool,
) -> SkillUseTime {
    #[cfg(test)]
    super::hit_read_probe::dependency("skill use time/server tick");
    let total_skill_speed = db.sum_by_name(ModType::Inc, cfg, speed_names_slice(cfg));
    let action_factor = action_speed_factor(db, cfg);
    let total_action_speed = (action_factor - 1.0) * 100.0;

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

/// Expected number of crossbow bolts reloaded within the past `window_s`
/// seconds, averaged over shots fired during a steady-state reload cycle
/// (Fresh Clip I: 6s, Fresh Clip II: 8s).
///
/// Each reload loads `bolt_count` bolts at the start of a cycle whose firing
/// phase lasts `T_fire = reload.total_firing_time` and whose full cycle lasts
/// `C = T_fire + reload.effective_reload_time`. For a shot fired at
/// `t ∈ [0, T_fire]`, the number of reloads inside `(t - window_s, t]` is
/// `k + [t < r]` with `k = floor(window_s / C)` and `r = window_s - k * C`.
/// Averaging uniformly over `t ∈ [0, T_fire]` gives
/// `bolt_count * (k + min(r, T_fire) / T_fire)`.
pub fn crossbow_bolts_reloaded_in_window(
    reload: &CrossbowReload,
    bolt_count: f64,
    window_s: f64,
) -> f64 {
    if !reload.effective_bolt_count.is_finite()
        || reload.total_firing_time <= 0.0
        || window_s <= 0.0
    {
        return 0.0;
    }
    let bolt_count = bolt_count.max(1.0);
    let cycle = reload.total_firing_time + reload.effective_reload_time;
    if cycle <= 0.0 {
        return 0.0;
    }
    let full_cycles = (window_s / cycle).floor();
    let remainder = (window_s - full_cycles * cycle).max(0.0);
    let partial = (remainder / reload.total_firing_time).min(1.0);
    super::round(bolt_count * (full_cycles + partial))
}

/// Crossbow reload time folding (vendor `calcCrossbowReloadTime`,
/// `CalcOffence.lua:283-291`): `reload = base /
/// calcLib.mod(skillModList, cfg, "ReloadSpeed", "Speed")` — `Speed`'s and
/// `ReloadSpeed`'s INC values share a bucket (summed) and MORE values are
/// multiplied together to form the divisor (attack speed bonuses also speed
/// up reloading). `Speed` expands to pobr's speed bucket names
/// (see [`speed_names_for`]).
pub fn crossbow_reload_time(db: &crate::ModDb, cfg: &CalcConfig, base_reload_s: f64) -> f64 {
    let speed = speed_names_slice(cfg);
    let mut buf = ["ReloadSpeed"; 4];
    let len = 1 + speed.len();
    buf[1..len].copy_from_slice(speed);
    let names = &buf[..len];
    let multi =
        (1.0 + db.sum_by_name(ModType::Inc, cfg, names) / 100.0) * db.more_by_name(cfg, names);
    if multi <= 0.0 {
        return base_reload_s;
    }
    base_reload_s / multi
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
        assert!(
            (t - 0.8 / 1.5).abs() < 1e-9,
            "INC sums within the same bucket: {t}"
        );
    }

    #[test]
    fn bolts_reloaded_in_window_matches_shot_averaged_cycle() {
        // Long clip (Rapid Shot: 100 bolts at 8.6221/s, 0.7895s reload):
        // T_fire = 11.5981s > 8s, so 8 / T_fire of fired shots have the 100-bolt reload in window.
        let long = apply_crossbow_reload(8.6221, 100.0, 0.7895, 0.0, 0.0);
        let in_8s = crossbow_bolts_reloaded_in_window(&long, 100.0, 8.0);
        assert!(
            (in_8s - 8.0 * 8.6221).abs() < 1e-3,
            "expected ~68.9768, got {in_8s}"
        );

        // Short clip (2 bolts at 2.5/s, 0.45s reload => T_fire = 0.8s, C = 1.25s):
        // Over 8s: floor(8 / 1.25) = 6 full cycles, remainder = 0.5s => partial = 0.5 / 0.8 = 0.625.
        let short = apply_crossbow_reload(2.5, 2.0, 0.45, 0.0, 0.0);
        let short_8s = crossbow_bolts_reloaded_in_window(&short, 2.0, 8.0);
        assert!(
            (short_8s - 2.0 * 6.625).abs() < 1e-4,
            "expected 13.25, got {short_8s}"
        );

        // Infinite magazine never reloads.
        let inf = apply_crossbow_reload(5.0, 10.0, 0.8, 100.0, 0.0);
        assert_eq!(crossbow_bolts_reloaded_in_window(&inf, 10.0, 8.0), 0.0);
    }
}
