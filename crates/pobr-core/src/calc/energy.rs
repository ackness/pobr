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
//! The spell's own rate of use is replaced with the rate of triggers. Without
//! a skill that can fill the gem the spell keeps its own rate, as if it were
//! cast by hand.

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
/// One of the character's skills that can fill the gem: a list entry whose
/// nested modifiers are the four values below, named by its source.
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

/// The source that fills the gem most often.
#[derive(Debug, Clone, PartialEq)]
struct BestSource {
    skill: String,
    energy_per_hit: f64,
    triggers_per_second: f64,
}

/// Replaces the main skill's rate of use with the rate at which its meta gem
/// triggers it. Does nothing for a skill outside an Energy meta gem, or when
/// none of the character's skills can fill the gem.
pub fn apply_energy_trigger(env: &mut Env) {
    let db = &env.player.mod_db;
    let cfg = &env.cfg;
    let sum = |name: &str| db.sum(ModType::Base, cfg, &[ModName::from(name)]);
    let maximum = sum(MAXIMUM_ENERGY);
    if maximum <= 0.0 {
        return;
    }
    let own_rate = env.player.output.effective_action_rate;
    let energy_per_power = sum(ENERGY_PER_POWER_ON_CRIT);
    // The Energy comes with a critical hit: one has been dealt Recently.
    let gain_cfg = cfg.clone().with_condition("CritRecently", true);
    let gain_names = [ModName::from(ENERGY_GENERATED)];
    let gain_multiplier = (1.0 + db.sum(ModType::Inc, &gain_cfg, &gain_names) / 100.0)
        * db.more(&gain_cfg, &gain_names);
    let enemy_power = match sum("Multiplier:EnemyPower") {
        power if power > 0.0 => power,
        _ => ORDINARY_ENEMY_POWER,
    };
    let ailment_threshold =
        super::perform::enemy_ailment_threshold_effective(&env.enemy.mod_db, cfg, env.enemy.level);
    let half_refund_chance = sum(REFUND_HALF_CHANCE) / 100.0;

    let mut best: Option<BestSource> = None;
    for source in db
        .named(ENERGY_SOURCE)
        .filter(|modifier| modifier.mod_type == ModType::List && modifier.matches(cfg))
    {
        let Some(values) = source.value.as_nested_mods() else {
            continue;
        };
        let value = |name: &str| {
            values
                .iter()
                .find(|inner| inner.name.as_str() == name)
                .and_then(|inner| inner.value.as_number())
                .unwrap_or(0.0)
        };
        let energy_per_hit = energy_per_critical_hit(
            energy_per_power,
            enemy_power,
            value(SOURCE_CRIT_DAMAGE),
            ailment_threshold,
            gain_multiplier,
        );
        let critical_hits_per_second =
            value(SOURCE_RATE) * value(SOURCE_HIT_CHANCE) * value(SOURCE_CRIT_CHANCE);
        let rate = triggers_per_second(
            critical_hits_per_second,
            energy_per_hit,
            maximum,
            half_refund_chance,
        );
        if rate > best.as_ref().map_or(0.0, |best| best.triggers_per_second) {
            best = Some(BestSource {
                skill: source.source.clone().unwrap_or_default(),
                energy_per_hit,
                triggers_per_second: rate,
            });
        }
    }

    let out = &mut env.player.output;
    out.energy_trigger_maximum = round(maximum);
    let Some(best) = best else {
        return;
    };
    out.energy_per_trigger_event = round(best.energy_per_hit);
    out.energy_trigger_source = Some(best.skill);
    out.skill_trigger_rate = round(best.triggers_per_second);
    if own_rate <= 0.0 {
        return;
    }
    // The spell's damage per use stands; it is used at the rate of triggers.
    let factor = best.triggers_per_second / own_rate;
    out.dps = round(out.dps * factor);
    out.action_rate = round(out.action_rate * factor);
    out.effective_action_rate = round(best.triggers_per_second);
    if let Some(use_time) = &mut out.skill_use_time {
        use_time.effective_rate = out.effective_action_rate;
    }
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
