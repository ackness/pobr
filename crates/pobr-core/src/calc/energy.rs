//! Energy: how often a meta gem triggers the spells socketed in it.
//!
//! A meta gem such as Cast on Critical "gains Energy when you Critically Hit
//! enemies and triggers socketed Spells on reaching maximum Energy". A spell
//! in it is not cast at its own cast speed: it happens as often as the gem
//! fills. The rules, as the game's own stats, descriptions and glossary give
//! them:
//!
//! - Maximum Energy. The gem's support half carries
//!   `generic_ongoing_trigger_1_maximum_energy_per_Xms_total_cast_time = 10`:
//!   "Has 10 maximum Energy per 0.1 seconds of base cast time of Socketed
//!   Spells". The glossary entry for Energy adds: "When calculating Energy
//!   gain or consumption from the use time of a Skill, modifiers to Total use
//!   time are treated as though they were double the value." Comet's base
//!   cast time is 1 second and it adds 1 second to its total cast time
//!   (`total_cast_time_+_ms = 1000`): 100 + 2 × 100 = 300 Energy.
//! - Energy gained. `cast_on_crit_gain_X_centienergy_per_monster_power_on_crit
//!   = 100`: "Gains 1 Energy per Power of enemies you Critically Hit with
//!   Skills, modified by the percentage of the enemy's Ailment Threshold the
//!   Critical Hit will deal", with the gem's `energy_generated_+%`, which is
//!   also the stat of "Meta Skills gain #% increased Energy". The glossary
//!   gives an average monster a Power of 1 and "Unique monsters always have
//!   20 Power".
//! - Sources. "Energy cannot be gained from direct effects of Triggered
//!   Skills" (the glossary again): only the skills the character uses itself
//!   fill the gem.
//! - Spending. "Triggers all Socketed Spells and loses all Energy on reaching
//!   maximum Energy": what a hit gives beyond the maximum is lost, and one
//!   hit triggers at most once. "#% chance for Trigger skills to refund half
//!   of Energy Spent" leaves the gem half full that often.
//!
//! What the game does not state, and what is assumed here:
//!
//! - The "percentage of the enemy's Ailment Threshold" is taken as the plain
//!   ratio of the hit, before the enemy's mitigation, to the threshold, with
//!   no upper limit. That is how players have measured it; the game publishes
//!   no formula.
//! - Every critical hit of a skill deals its average critical damage, so the
//!   gem fills after a whole number of them.
//! - A skill hits the enemy as often as its own damage per second assumes.
//!   A skill whose projectiles hit several times gives more than this.
//! - The character uses one skill to fill the gem. The stage takes the one
//!   that fills it most often.
//! - The gem gains its Energy from critical hits, so "if you've dealt a
//!   Critical Hit Recently" holds whenever it gains any: the condition is
//!   set for the Energy gained, whatever the build's configuration says.
//!
//! Cast on Elemental Ailment uses ignite/shock/freeze events (not chill).
//! Ignite Energy scales by the ignite's total damage / ailment threshold;
//! shock gives 1 and freeze 10 Energy per Power. Cast on Block gives 25 per
//! block; Cast on Minion Death gives 50 per Power of the killed minion.
//! Spellslinger gains 0.2 Energy per 10 ms base cast time of self-cast spells.
//! Its and Elemental Invocation's 500-Energy capacity is separate from the
//! socketed spells' cost: manual activations can spend several cycles.
//!
//! These rates are estimates: mixed ailment events use their mean Energy;
//! freeze repeats after whole hits fill buildup, without a refractory period.
//! Invocations assume immediate spending unless a manual-use rate is supplied,
//! with unspent Energy retained and a cooldown/storage cap on throughput.
//! Missing event sources retain the spell's own rate, with an explicit fallback
//! explanation. Configured zero event rates still stop triggers. Killed-minion
//! Power defaults to one.

use pobr_data::prelude::*;

use super::{Env, round};

/// Maximum Energy of the meta gem that holds the main skill.
pub const MAXIMUM_ENERGY: &str = "EnergyTriggerMaximum";
/// Energy the gem gains per Power of an enemy hit critically.
pub const ENERGY_PER_POWER_ON_CRIT: &str = "EnergyPerPowerOnCrit";
/// Increases to the Energy a meta gem gains.
pub const ENERGY_GENERATED: &str = "EnergyGenerated";
/// Chance, in percent, that a trigger refunds half of the Energy it spent.
pub const REFUND_HALF_CHANCE: &str = "EnergyRefundHalfChance";
/// Energy event rules resolved from the meta gem's game stats.
pub const ENERGY_PER_BLOCK: &str = "EnergyPerBlock";
pub const ENERGY_PER_MINION_POWER: &str = "EnergyPerMinionPower";
pub const ENERGY_PER_CAST_MS: &str = "EnergyPerCastMillisecond";
pub const ENERGY_PER_IGNITE_POWER: &str = "EnergyPerIgnitePower";
pub const ENERGY_PER_SHOCK_POWER: &str = "EnergyPerShockPower";
pub const ENERGY_PER_FREEZE_POWER: &str = "EnergyPerFreezePower";
pub const ENERGY_COST: &str = "EnergyTriggerCost";
pub const INVOCATION_COOLDOWN: &str = "EnergyInvocationCooldown";
pub const SOURCE_CAST_TIME: &str = "CastTimeMs";
pub const SOURCE_IGNITE_CHANCE: &str = "IgniteChance";
pub const SOURCE_IGNITE_DAMAGE: &str = "IgniteDamage";
pub const SOURCE_SHOCK_CHANCE: &str = "ShockChance";
pub const SOURCE_FREEZE_CHANCE: &str = "FreezeChance";

/// Per successful hit, independent of the triggered spell's ailments.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AilmentSourceStats {
    pub ignite_chance: f64,
    /// Total damage of one ignite before enemy mitigation, without uptime.
    pub ignite_damage: f64,
    pub shock_chance: f64,
    /// Mean freezes per hit, estimated from whole hits to fill buildup.
    pub freeze_chance: f64,
}

/// Resolved terms shared by JSON output and the trigger-rate explanation.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct EnergyTriggerTerms {
    pub maximum_energy: f64,
    pub energy_per_trigger: f64,
    pub source: Option<String>,
    /// No event source is available, so the payload retains its own use rate.
    pub at_own_rate: bool,
    /// Stable explanation id identifying the missing input or mechanic.
    pub missing_source: Option<&'static str>,
    pub events_per_second: f64,
    pub energy_per_event: f64,
    pub refund_half_chance: f64,
    pub events_per_trigger: f64,
    pub energy_limited_rate: f64,
    pub trigger_rate_cap: f64,
    pub triggers_per_second: f64,
    pub invocation: bool,
    pub rate_multiplier: f64,
}

/// The existing ailment pipeline supplies the application probabilities.
/// Freeze buildup is a percentage per damage point, not a chance to freeze.
pub(crate) fn source_ailments(env: &Env) -> AilmentSourceStats {
    use super::ailment::{self, AilmentSource};
    let out = &env.player.output;
    let player = &env.player.mod_db;
    let cfg = &env.cfg;
    let mut result = AilmentSourceStats::default();
    if let Some((_, terms)) = out
        .ailment_terms
        .iter()
        .find(|(kind, _)| *kind == AilmentType::Ignite)
    {
        let count = terms.passes.len() as f64;
        for (_, t) in &terms.passes {
            result.ignite_chance += t.apply_chance / count;
            result.ignite_damage += t.application_source_damage * t.per_second_pct / 100.0
                * (1.0 + t.magnitude_inc / 100.0)
                * t.magnitude_more
                * t.effect
                * t.rate
                * t.duration
                / count;
        }
    }
    let lightning =
        ailment::cross_type_source_hit(AilmentType::Shock, &out.damage_components, player, cfg);
    if lightning > 0.0 {
        let source = AilmentSource::new(
            lightning,
            out.crit_multiplier,
            out.crit_chance,
            player.flag(cfg, "AilmentsAreNeverFromCrit"),
        );
        let threshold = super::perform::enemy_ailment_threshold_effective(
            &env.enemy.mod_db,
            cfg,
            env.enemy.level,
        );
        result.shock_chance = ailment::shock_traced(
            &source,
            player,
            &env.enemy.mod_db,
            cfg,
            threshold,
            &mut crate::TraceGraph::new(),
        )
        .0;
    }
    let cold =
        ailment::cross_type_source_hit(AilmentType::Chill, &out.damage_components, player, cfg);
    let buildup = cold * out.freeze_buildup_pct;
    if buildup > 0.0 {
        result.freeze_chance = 1.0 / (100.0 / buildup - 1e-9).ceil().max(1.0);
    }
    result
}

/// One of the character's skills that can fill the gem: a list entry whose
/// nested modifiers describe the event rate and gain, named by its source.
pub const ENERGY_SOURCE: &str = "EnergySource";
/// Hits per second of a source skill.
pub const SOURCE_RATE: &str = "Rate";
/// Its chance to hit, as a fraction.
pub const SOURCE_HIT_CHANCE: &str = "HitChance";
/// Its chance to hit critically, as a fraction.
pub const SOURCE_CRIT_CHANCE: &str = "CritChance";
/// The average damage of its critical hit, before the enemy's mitigation.
pub const SOURCE_CRIT_DAMAGE: &str = "CritDamage";

/// A unique enemy's Power; an enemy the tier preset says nothing about is an
/// ordinary monster.
const ORDINARY_ENEMY_POWER: f64 = 1.0;

/// One spell socketed in a meta gem, as far as the gem's Energy goes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SocketedSpell {
    /// Base cast time, in milliseconds.
    pub cast_time_ms: f64,
    /// What the spell adds to its own total cast time (`total_cast_time_+_ms`).
    pub added_total_cast_time_ms: f64,
}

/// Maximum Energy of a meta gem holding `spells`, at one Energy per
/// `ms_per_energy` milliseconds of their cast time.
pub fn maximum_energy(spells: &[SocketedSpell], ms_per_energy: f64) -> f64 {
    if ms_per_energy <= 0.0 {
        return 0.0;
    }
    spells
        .iter()
        .map(|spell| (spell.cast_time_ms + 2.0 * spell.added_total_cast_time_ms) / ms_per_energy)
        .sum()
}

/// The empty-to-full channel duration uses continuous Energy gain, without
/// assuming a refund from an earlier trigger or changing the spell's DPS.
pub(crate) fn channel_trigger_terms(db: &crate::ModDb, cfg: &crate::CalcConfig) -> (f64, f64) {
    let maximum = db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("ChannelTriggerMaximumEnergy")],
    );
    let gain = db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("ChannelTriggerEnergyPerSecond")],
    );
    let names = [
        ModName::from(ENERGY_GENERATED),
        ModName::from("ChannelTriggerEnergyGenerated"),
    ];
    (
        maximum,
        gain * (1.0 + db.sum(ModType::Inc, cfg, &names) / 100.0) * db.more(cfg, &names),
    )
}

pub(crate) fn channel_time_to_trigger(db: &crate::ModDb, cfg: &crate::CalcConfig) -> f64 {
    let (maximum, gain) = channel_trigger_terms(db, cfg);
    if maximum > 0.0 && gain > 0.0 {
        round(maximum / gain)
    } else {
        0.0
    }
}

/// Energy one critical hit gives: `energy_per_power` for each Power of the
/// enemy, by the share of its Ailment Threshold the hit deals, by the
/// modifiers to Energy gained.
pub fn energy_per_critical_hit(
    energy_per_power: f64,
    enemy_power: f64,
    hit_damage: f64,
    ailment_threshold: f64,
    gain_multiplier: f64,
) -> f64 {
    if ailment_threshold <= 0.0 {
        return 0.0;
    }
    (energy_per_power * enemy_power * (hit_damage / ailment_threshold) * gain_multiplier).max(0.0)
}

/// Triggers per second of a gem that gains `energy_per_event` Energy
/// `events_per_second` times a second. The gem empties when it triggers, so
/// it takes a whole number of events to fill; with `half_refund_chance` (a
/// fraction) it starts the next round half full.
pub fn triggers_per_second(
    events_per_second: f64,
    energy_per_event: f64,
    maximum_energy: f64,
    half_refund_chance: f64,
) -> f64 {
    if !(events_per_second > 0.0 && energy_per_event > 0.0 && maximum_energy > 0.0) {
        return 0.0;
    }
    // The tolerance keeps an event that gives exactly a whole share of the
    // maximum from needing one more because of rounding.
    let events_to_gain = |energy: f64| (energy / energy_per_event - 1e-9).ceil().max(1.0);
    let half = half_refund_chance.clamp(0.0, 1.0);
    let events_to_fill =
        (1.0 - half) * events_to_gain(maximum_energy) + half * events_to_gain(maximum_energy / 2.0);
    events_per_second / events_to_fill
}

/// Replaces the socketed spell's rate after resolving its Energy sources.
pub fn apply_energy_trigger(env: &mut Env) {
    let db = &env.player.mod_db;
    let cfg = &env.cfg;
    let sum = |name: &str| db.sum(ModType::Base, cfg, &[ModName::from(name)]);
    let maximum = sum(MAXIMUM_ENERGY);
    if maximum <= 0.0 {
        return;
    }
    let cost = sum(ENERGY_COST).max(0.0);
    let cost = if cost > 0.0 { cost } else { maximum };
    let invocation_cd = sum(INVOCATION_COOLDOWN);
    let invocation = invocation_cd > 0.0;
    let crit = sum(ENERGY_PER_POWER_ON_CRIT);
    let gain_cfg = if crit > 0.0 {
        cfg.clone().with_condition("CritRecently", true)
    } else {
        cfg.clone()
    };
    let names = [ModName::from(ENERGY_GENERATED)];
    let gain = ((1.0 + db.sum(ModType::Inc, &gain_cfg, &names) / 100.0)
        * db.more(&gain_cfg, &names))
    .max(0.0);
    let power = match sum("Multiplier:EnemyPower") {
        power if power > 0.0 => power,
        _ => ORDINARY_ENEMY_POWER,
    };
    let threshold =
        super::perform::enemy_ailment_threshold_effective(&env.enemy.mod_db, cfg, env.enemy.level);
    let refund = (sum(REFUND_HALF_CHANCE) / 100.0).clamp(0.0, 1.0);
    let mut best = EnergyTriggerTerms {
        maximum_energy: round(maximum),
        energy_per_trigger: round(cost),
        source: None,
        at_own_rate: false,
        missing_source: None,
        events_per_second: 0.0,
        energy_per_event: 0.0,
        refund_half_chance: refund,
        events_per_trigger: 0.0,
        energy_limited_rate: 0.0,
        trigger_rate_cap: 0.0,
        triggers_per_second: 0.0,
        invocation,
        rate_multiplier: 1.0,
    };
    let invocation_cap = if invocation {
        let cooldown_cap = super::trigger::trigger_rate_cap(
            invocation_cd,
            super::trigger::server_tick_rate(cfg.constants.game().server_tick_seconds),
        );
        let uses = cfg
            .multiplier_if_set("EnergyInvocationUsesPerSecond")
            .unwrap_or(cooldown_cap)
            .clamp(0.0, cooldown_cap);
        (maximum / cost).floor() * uses
    } else {
        0.0
    };
    let mut consider = |source: String, rate: f64, energy: f64| {
        let energy = (energy * gain).max(0.0);
        let rate = rate.max(0.0);
        let (limited, events) = if invocation {
            // Invocations retain unspent Energy and can spend several cycles at once.
            let spent = cost * (1.0 - refund / 2.0);
            let events = if energy > 0.0 {
                spent / energy.min(maximum)
            } else {
                0.0
            };
            (if events > 0.0 { rate / events } else { 0.0 }, events)
        } else {
            let limited = triggers_per_second(rate, energy, cost, refund);
            (limited, if limited > 0.0 { rate / limited } else { 0.0 })
        };
        let cap = if invocation { invocation_cap } else { rate };
        let triggers = limited.min(cap);
        if best.source.is_none() || triggers > best.triggers_per_second {
            best.source = Some(source);
            best.events_per_second = rate;
            best.energy_per_event = energy;
            best.events_per_trigger = events;
            best.energy_limited_rate = limited;
            best.trigger_rate_cap = cap;
            best.triggers_per_second = triggers;
        }
    };
    let missing_source;
    if sum(ENERGY_PER_BLOCK) > 0.0 {
        missing_source = if cfg.multiplier_if_set("EnergyIncomingHitRate").is_some() {
            "EnergyNeedsBlockChance"
        } else {
            "EnergyNeedsIncomingHits"
        };
        if let Some(incoming) = cfg.multiplier_if_set("EnergyIncomingHitRate")
            && (incoming == 0.0 || env.player.output.effective_block_chance > 0.0)
        {
            consider(
                "Block".into(),
                incoming * env.player.output.effective_block_chance / 100.0,
                sum(ENERGY_PER_BLOCK),
            );
        }
    } else if sum(ENERGY_PER_MINION_POWER) > 0.0 {
        missing_source = "EnergyNeedsMinionDeaths";
        // The configured average Power describes the minions that actually die.
        let minion_power = cfg
            .multiplier_if_set("EnergyMinionPower")
            .unwrap_or(1.0)
            .max(0.0);
        if let Some(deaths) = cfg.multiplier_if_set("EnergyMinionDeathsPerSecond") {
            consider(
                "MinionDeath".into(),
                deaths,
                sum(ENERGY_PER_MINION_POWER) * minion_power,
            );
        }
    } else {
        missing_source = if crit > 0.0 {
            "EnergyNeedsCriticalHits"
        } else if sum(ENERGY_PER_CAST_MS) > 0.0 {
            "EnergyNeedsSpellCasts"
        } else {
            "EnergyNeedsAilments"
        };
        for source in db
            .named(ENERGY_SOURCE)
            .filter(|m| m.mod_type == ModType::List && m.matches(cfg))
        {
            let Some(values) = source.value.as_nested_mods() else {
                continue;
            };
            let value = |name: &str| {
                values
                    .iter()
                    .find(|m| m.name.as_str() == name)
                    .and_then(|m| m.value.as_number())
                    .unwrap_or(0.0)
            };
            let (rate, energy) = if crit > 0.0 {
                (
                    value(SOURCE_RATE) * value(SOURCE_HIT_CHANCE) * value(SOURCE_CRIT_CHANCE),
                    energy_per_critical_hit(crit, power, value(SOURCE_CRIT_DAMAGE), threshold, 1.0),
                )
            } else if sum(ENERGY_PER_CAST_MS) > 0.0 {
                (
                    value(SOURCE_RATE),
                    value(SOURCE_CAST_TIME) * sum(ENERGY_PER_CAST_MS),
                )
            } else {
                let ignite = value(SOURCE_IGNITE_CHANCE);
                let shock = value(SOURCE_SHOCK_CHANCE);
                let freeze = value(SOURCE_FREEZE_CHANCE);
                let chances = ignite + shock + freeze;
                let energy = if chances > 0.0 {
                    power
                        * (ignite * sum(ENERGY_PER_IGNITE_POWER) * value(SOURCE_IGNITE_DAMAGE)
                            / threshold
                            + shock * sum(ENERGY_PER_SHOCK_POWER)
                            + freeze * sum(ENERGY_PER_FREEZE_POWER))
                        / chances
                } else {
                    0.0
                };
                (
                    value(SOURCE_RATE) * value(SOURCE_HIT_CHANCE) * chances,
                    energy,
                )
            };
            if rate > 0.0 && energy > 0.0 {
                consider(source.source.clone().unwrap_or_default(), rate, energy);
            }
        }
    }
    let out = &mut env.player.output;
    out.energy_trigger_maximum = round(maximum);
    // A missing source is unknown, not a configured zero. Explicitly disabling
    // manual activations still stops an Invocation even without a source skill.
    let stopped = invocation && cfg.multiplier_if_set("EnergyInvocationUsesPerSecond") == Some(0.0);
    if best.source.is_none() && !stopped {
        best.at_own_rate = true;
        best.missing_source = Some(missing_source);
        out.energy_trigger = Some(best);
        return;
    }
    out.energy_per_trigger_event = round(best.energy_per_event);
    out.energy_trigger_source = best.source.clone();
    out.skill_trigger_rate = round(best.triggers_per_second);
    out.trigger_rate_cap = round(best.trigger_rate_cap);
    let own_rate = out.effective_action_rate;
    if own_rate > 0.0 {
        let factor = best.triggers_per_second / own_rate;
        best.rate_multiplier = factor;
        out.dps = round(out.dps * factor);
        out.action_rate = round(out.action_rate * factor);
    }
    out.effective_action_rate = out.skill_trigger_rate;
    if let Some(use_time) = &mut out.skill_use_time {
        use_time.effective_rate = out.effective_action_rate;
    }
    out.energy_trigger = Some(best);
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMET: SocketedSpell = SocketedSpell {
        cast_time_ms: 1000.0,
        added_total_cast_time_ms: 1000.0,
    };
    const SPARK: SocketedSpell = SocketedSpell {
        cast_time_ms: 700.0,
        added_total_cast_time_ms: 0.0,
    };

    #[test]
    fn a_spells_own_addition_to_its_cast_time_counts_twice() {
        assert_eq!(maximum_energy(&[SPARK], 10.0), 70.0);
        assert_eq!(maximum_energy(&[COMET], 10.0), 300.0);
        assert_eq!(maximum_energy(&[COMET, SPARK], 10.0), 370.0);
        assert_eq!(maximum_energy(&[], 10.0), 0.0);
        assert_eq!(maximum_energy(&[COMET], 0.0), 0.0);
    }

    #[test]
    fn a_critical_hit_gives_energy_by_power_and_share_of_the_threshold() {
        // A hit of the threshold's size against an ordinary monster: 1.
        assert_eq!(energy_per_critical_hit(1.0, 1.0, 9723.0, 9723.0, 1.0), 1.0);
        // Ten thresholds against a unique enemy, with 57% increased Energy.
        let energy = energy_per_critical_hit(1.0, 20.0, 97_230.0, 9723.0, 1.57);
        assert!((energy - 314.0).abs() < 1e-9, "{energy}");
        assert_eq!(energy_per_critical_hit(1.0, 20.0, 1000.0, 0.0, 1.0), 0.0);
    }

    #[test]
    fn the_gem_fills_after_a_whole_number_of_events() {
        // Each event fills it: one trigger per event, never more.
        assert_eq!(triggers_per_second(4.0, 300.0, 300.0, 0.0), 4.0);
        assert_eq!(triggers_per_second(4.0, 9000.0, 300.0, 0.0), 4.0);
        // 100 of 300: every third event. 101 of 300 still takes three.
        assert_eq!(triggers_per_second(3.0, 100.0, 300.0, 0.0), 1.0);
        assert_eq!(triggers_per_second(3.0, 101.0, 300.0, 0.0), 1.0);
        // 99 of 300 takes four.
        assert_eq!(triggers_per_second(4.0, 99.0, 300.0, 0.0), 1.0);
        assert_eq!(triggers_per_second(0.0, 100.0, 300.0, 0.0), 0.0);
        assert_eq!(triggers_per_second(3.0, 0.0, 300.0, 0.0), 0.0);
        assert_eq!(triggers_per_second(3.0, 100.0, 0.0, 0.0), 0.0);
    }

    #[test]
    fn a_refund_of_half_the_energy_shortens_the_next_round() {
        // 30 of 300: ten events from empty, five from half. A quarter of the
        // rounds start half full: 8.75 events on average.
        let rate = triggers_per_second(8.75, 30.0, 300.0, 0.25);
        assert!((rate - 1.0).abs() < 1e-12, "{rate}");
        // A hit that fills the gem from empty gains nothing from the refund.
        assert_eq!(triggers_per_second(4.0, 300.0, 300.0, 0.25), 4.0);
    }
}
