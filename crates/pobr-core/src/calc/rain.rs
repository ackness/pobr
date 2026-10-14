//! Rains: skills whose hits come from what falls after each use.
//!
//! Rain of Arrows fires its arrows into the air and they come down one after
//! another. What the game states, in the skill's stats and their texts:
//!
//! - "One arrow lands every {0} seconds" (`rain_of_arrows_delay_per_arrow`,
//!   80 ms), and "Modifiers to Projectile speed apply to arrow landing
//!   frequency": the interval is divided by the skill's projectile speed
//!   multiplier (the game shows the result as
//!   `virtual_rain_of_arrows_delay_per_arrow_display_ms`).
//! - "Can Hit each enemy only once every {0} seconds" (`rain_hit_delay_ms`,
//!   120 ms).
//! - A use fires the skill's projectiles as arrows
//!   (`base_number_of_projectiles`, 30 at level 1 and 39 at level 20, with
//!   whatever adds to the count).
//! - "Maximum 1 arrow rain" (`base_number_of_arrow_rains_allowed`): a new use
//!   replaces the rain of the use before it.
//!
//! An arrow that lands while the enemy cannot be hit again does nothing, so
//! the enemy is hit by one arrow in every ⌈hit interval ÷ landing interval⌉,
//! and a rain of N arrows hits it ⌈N ÷ that⌉ times. While the uses follow
//! each other closely enough for a rain to be falling all the time, the hits
//! come at that pace; otherwise each use gives one rain's hits.
//!
//! What the game does not state, and what is assumed here:
//!
//! - Every arrow that lands reaches the enemy. The rain falls over a radius
//!   of 3.6 metres and an arrow hits within 0.7 metres of where it lands:
//!   against a single enemy fewer do, and how many depends on its size.
//! - The arrows land at an even pace from the first one, at the game's
//!   times, not rounded to the server's ticks.
//! - The character uses the skill again and again.
//! - "Arrows fall {0}% faster" (the gem's quality) is how fast an arrow
//!   falls, not how often one lands: it changes nothing here.
//!
//! The exporter of a build code's reference values counts a hit whenever
//! the enemy can be hit again, as if an arrow landed at that moment:
//! [`CalcConfig::rain_hits_whenever_allowed`] asks the same question.

use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb};

use super::formula::{FormulaBuilder, FormulaOp, StatFormula};

/// Milliseconds between two arrows of a rain landing, before projectile
/// speed.
pub const LANDING_INTERVAL: &str = "RainLandingInterval";
/// Milliseconds before what a rain drops can hit the same enemy again.
pub const HIT_INTERVAL: &str = "RainHitInterval";
/// What scales how often the arrows land.
const LANDING_SPEED: &str = "ProjectileSpeed";

/// How a rain's hits come, as the calculation of the main skill counted them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RainSummary {
    /// Seconds between two arrows landing.
    pub landing_interval: f64,
    /// Seconds before the enemy can be hit again.
    pub hit_interval: f64,
    /// The enemy is hit by one landing arrow in this many; 1 on the
    /// reference's basis, which counts a hit whenever one is allowed.
    pub arrows_per_hit: f64,
    /// Arrows one use fires.
    pub arrows: f64,
    pub hits_per_second: f64,
}

/// How often a rain hits an enemy, with the terms it comes from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RainHits {
    /// Arrows landing per second, before projectile speed.
    base_landing_rate: f64,
    speed_inc: f64,
    speed_more: f64,
    /// Arrows landing per second.
    landing_rate: f64,
    /// Seconds before the enemy can be hit again.
    hit_interval: f64,
    /// The enemy is hit by one arrow in this many.
    pub(crate) arrows_per_hit: f64,
    /// Arrows one use fires.
    pub(crate) arrows: f64,
    /// Hits per second while a rain is falling.
    while_falling: f64,
    /// Hits per second the uses give at most: one rain's hits per use.
    per_use_limit: Option<f64>,
    /// Counted on the reference's basis: a hit whenever one is allowed.
    whenever_allowed: bool,
    /// Hits per second.
    pub(crate) rate: f64,
}

/// Whether the skill `db` and `cfg` describe is a rain.
pub(crate) fn is_rain(db: &ModDb, cfg: &CalcConfig) -> bool {
    db.sum(ModType::Base, cfg, &[ModName::from(LANDING_INTERVAL)]) > 0.0
}

/// How often the rain of the skill that `db` and `cfg` describe hits an
/// enemy when the character uses the skill `uses_per_second` times a second.
/// `None` for a skill that is not a rain.
pub(crate) fn rain_hits(db: &ModDb, cfg: &CalcConfig, uses_per_second: f64) -> Option<RainHits> {
    let landing_ms = db.sum(ModType::Base, cfg, &[ModName::from(LANDING_INTERVAL)]);
    if landing_ms <= 0.0 {
        return None;
    }
    let hit_ms = db
        .sum(ModType::Base, cfg, &[ModName::from(HIT_INTERVAL)])
        .max(0.0);
    let speed = [ModName::from(LANDING_SPEED)];
    let speed_inc = db.sum(ModType::Inc, cfg, &speed);
    let speed_more = db.more(cfg, &speed);
    let base_landing_rate = 1000.0 / landing_ms;
    let landing_rate = (base_landing_rate * (1.0 + speed_inc / 100.0) * speed_more).max(0.0);
    let hit_interval = hit_ms / 1000.0;
    let arrows = super::skill_mechanics::calc_projectile_count(db, cfg)
        .projectile_count
        .max(1.0);
    let (arrows_per_hit, while_falling, per_use_limit) = if landing_rate <= 0.0 {
        (1.0, 0.0, None)
    } else if cfg.rain_hits_whenever_allowed {
        let while_falling = if hit_interval > 0.0 {
            landing_rate.min(1.0 / hit_interval)
        } else {
            landing_rate
        };
        (1.0, while_falling, None)
    } else {
        // The first arrow to land once the interval has passed. A tolerance
        // keeps an interval that is a whole number of landings at that
        // number.
        let arrows_per_hit = (hit_interval * landing_rate - 1e-9).ceil().max(1.0);
        let hits_per_rain = (arrows / arrows_per_hit).ceil();
        (
            arrows_per_hit,
            landing_rate / arrows_per_hit,
            Some(hits_per_rain * uses_per_second.max(0.0)),
        )
    };
    let rate = per_use_limit.map_or(while_falling, |limit| while_falling.min(limit));
    Some(RainHits {
        base_landing_rate,
        speed_inc,
        speed_more,
        landing_rate,
        hit_interval,
        arrows_per_hit,
        arrows,
        while_falling,
        per_use_limit,
        whenever_allowed: cfg.rain_hits_whenever_allowed,
        rate,
    })
}

impl RainHits {
    pub(crate) fn summary(&self) -> RainSummary {
        RainSummary {
            landing_interval: if self.landing_rate > 0.0 {
                1.0 / self.landing_rate
            } else {
                f64::INFINITY
            },
            hit_interval: self.hit_interval,
            arrows_per_hit: self.arrows_per_hit,
            arrows: self.arrows,
            hits_per_second: super::round(self.rate),
        }
    }

    /// The steps from the rate the arrows land at to the hits per second,
    /// ending at `value`.
    pub(crate) fn formula(&self, value: f64) -> StatFormula {
        let mut steps = FormulaBuilder::default();
        steps.scaled(self.base_landing_rate, 0.0, self.speed_inc, self.speed_more);
        if self.whenever_allowed {
            steps.at_most(FormulaOp::Cap, self.while_falling);
        } else if self.while_falling < self.landing_rate {
            steps.set(
                FormulaOp::HitInterval,
                self.hit_interval,
                self.while_falling,
            );
        }
        if let Some(limit) = self.per_use_limit {
            steps.at_most(FormulaOp::Cap, limit);
        }
        steps.finish(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Modifier;

    /// A Rain of Arrows: arrows land every 80 ms and the enemy can be hit
    /// once every 120 ms; `arrows` per use and `speed` increased projectile
    /// speed.
    fn rain(arrows: f64, speed: f64) -> ModDb {
        let mut db = ModDb::new();
        db.add_list(vec![
            Modifier::number(LANDING_INTERVAL, ModType::Base, 80.0),
            Modifier::number(HIT_INTERVAL, ModType::Base, 120.0),
            Modifier::number("ProjectileCount", ModType::Base, arrows),
            Modifier::number("ProjectileSpeed", ModType::Inc, speed),
        ]);
        db
    }

    fn hits(db: &ModDb, cfg: &CalcConfig, uses: f64) -> RainHits {
        rain_hits(db, cfg, uses).expect("a rain")
    }

    /// With no projectile speed an arrow lands every 0.08 s: the one at
    /// 0.08 s is too early, the one at 0.16 s hits. Every second arrow hits.
    #[test]
    fn the_enemy_is_hit_by_the_first_arrow_after_the_interval() {
        let rain = hits(&rain(40.0, 0.0), &CalcConfig::default(), 2.0);
        assert_eq!(rain.arrows_per_hit, 2.0);
        assert!((rain.rate - 6.25).abs() < 1e-9);
    }

    /// Projectile speed makes the arrows land more often; the hits follow in
    /// steps of whole arrows, never faster than the interval allows.
    #[test]
    fn projectile_speed_brings_the_hits_closer_to_the_interval() {
        let cfg = CalcConfig::default();
        // Every 40 ms: the third arrow lands at exactly 0.12 s and hits.
        let even = hits(&rain(40.0, 100.0), &cfg, 2.0);
        assert_eq!(even.arrows_per_hit, 3.0);
        assert!((even.rate - 1.0 / 0.12).abs() < 1e-9);
        // 169% increased: every 0.0297 s, so the fifth arrow, at 0.149 s.
        let fast = hits(&rain(40.0, 169.0), &cfg, 2.0);
        assert_eq!(fast.arrows_per_hit, 5.0);
        assert!((fast.rate - 1000.0 / 80.0 * 2.69 / 5.0).abs() < 1e-9);
    }

    /// A rain of few arrows that a slow use does not replace in time gives
    /// its hits once per use.
    #[test]
    fn a_rain_that_ends_before_the_next_use_gives_its_hits_once_per_use() {
        // 5 arrows, every second one hits: 3 hits per use, 0.5 uses a second.
        let rain = hits(&rain(5.0, 0.0), &CalcConfig::default(), 0.5);
        assert!((rain.rate - 1.5).abs() < 1e-9);
        let formula = rain.formula(rain.rate);
        let ops: Vec<_> = formula.steps.iter().map(|step| step.op).collect();
        assert_eq!(
            ops,
            [FormulaOp::Base, FormulaOp::HitInterval, FormulaOp::Cap]
        );
    }

    /// The reference's basis: a hit whenever the enemy can be hit again.
    #[test]
    fn on_the_reference_basis_a_hit_comes_whenever_one_is_allowed() {
        let cfg = CalcConfig::default().with_rain_hits_whenever_allowed(true);
        let rain = hits(&rain(40.0, 169.0), &cfg, 2.0);
        assert!((rain.rate - 1.0 / 0.12).abs() < 1e-9);
        let formula = rain.formula(rain.rate);
        assert_eq!(
            formula.steps.last().map(|step| step.op),
            Some(FormulaOp::Cap)
        );
    }

    #[test]
    fn a_skill_without_a_landing_interval_is_no_rain() {
        let mut db = ModDb::new();
        db.add_mod(Modifier::number(HIT_INTERVAL, ModType::Base, 150.0));
        assert!(rain_hits(&db, &CalcConfig::default(), 2.0).is_none());
        assert!(!is_rain(&db, &CalcConfig::default()));
    }
}
