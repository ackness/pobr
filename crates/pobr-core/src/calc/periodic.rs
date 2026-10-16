//! Sustained hits from orbs and storms, independently of the casting rate.
//!
//! Ball Lightning's `ball_lightning_base_hit_frequency_ms` says "Hits enemies
//! in range every {0} seconds"; Solar Orb's `solar_orb_base_pulse_frequency_ms`
//! says "Pulses every {0} seconds". Firestorm and Icestorm drop one bolt every
//! `fire_storm_fireball_delay_ms`, with the shared `rain_hit_delay_ms` lockout.
//! Their evenly spaced landings use the same rule as Rain of Arrows.
//!
//! Thunderstorm's `lightning_storm_hit_frequency_ms` says "Hits one target
//! every {0} seconds" and `lightning_storm_hit_prevention_duration_ms` limits
//! each enemy. Its strikes have a variable interval, not the even cadence of
//! a rain. We report the potential rate capped by that enemy limit, assuming
//! a strike is available whenever the enemy can be hit. The random interval
//! (`lightning_storm_hit_frequency_variance_ms`) is not a simulated timeline.
//!
//! These are sustained, single-target estimates: the target stays in range,
//! every eligible bolt lands, and the player maintains one storm/orb. Further
//! overlapping storms do not multiply this estimate. Duration limits uptime
//! when the character cannot replace that instance before it expires. Ball
//! Lightning has no duration stat; its rate assumes continuous range exposure,
//! without estimating projectile travel or multiplying by projectile count.

use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb};

use super::formula::{FormulaBuilder, FormulaOp, StatFormula};

pub const INTERVAL: &str = "PeriodicHitInterval";
pub const DURATION: &str = "PeriodicHitDuration";
pub const LIMIT: &str = "PeriodicInstanceLimit";
pub const EVEN_LANDINGS: &str = "PeriodicEvenLandings";

#[derive(Debug, Clone, Copy)]
pub(crate) struct PeriodicHits {
    interval: f64,
    even: bool,
    hit_interval: f64,
    active_rate: f64,
    uptime: f64,
    pub(crate) rate: f64,
}

pub(crate) fn is_periodic(db: &ModDb, cfg: &CalcConfig) -> bool {
    db.sum(ModType::Base, cfg, &[ModName::from(INTERVAL)]) > 0.0
}

pub(crate) fn periodic_hits(db: &ModDb, cfg: &CalcConfig, uses: f64) -> Option<PeriodicHits> {
    let interval = db.sum(ModType::Base, cfg, &[ModName::from(INTERVAL)]) / 1000.0;
    if interval <= 0.0 {
        return None;
    }
    let hit_interval = db
        .sum(
            ModType::Base,
            cfg,
            &[ModName::from(super::rain::HIT_INTERVAL)],
        )
        .max(0.0)
        / 1000.0;
    let landing_rate = 1.0 / interval;
    let even = db.flag(cfg, EVEN_LANDINGS) && !cfg.rain_hits_whenever_allowed;
    let active_rate = if even {
        landing_rate / super::rain::landings_per_hit(landing_rate, hit_interval)
    } else if hit_interval > 0.0 {
        landing_rate.min(1.0 / hit_interval)
    } else {
        landing_rate
    };
    let duration_ms = db.sum(ModType::Base, cfg, &[ModName::from(DURATION)]);
    let uptime = if cfg.periodic_hits_assume_sustained {
        1.0
    } else if duration_ms > 0.0 {
        let names = [ModName::from("Duration")];
        let duration = duration_ms / 1000.0
            * (1.0 + db.sum(ModType::Inc, cfg, &names) / 100.0).max(0.0)
            * db.more(cfg, &names);
        let limit = db.sum(ModType::Base, cfg, &[ModName::from(LIMIT)]).max(0.0);
        (duration * uses.max(0.0)).min(limit).clamp(0.0, 1.0)
    } else if uses > 0.0 {
        1.0
    } else {
        0.0
    };
    Some(PeriodicHits {
        interval,
        even,
        hit_interval,
        active_rate,
        uptime,
        rate: active_rate * uptime,
    })
}

impl PeriodicHits {
    pub(crate) fn formula(&self, value: f64) -> StatFormula {
        let mut steps = FormulaBuilder::default();
        steps.scaled(1.0 / self.interval, 0.0, 0.0, 1.0);
        if self.even && self.active_rate < 1.0 / self.interval {
            steps.set(FormulaOp::HitInterval, self.hit_interval, self.active_rate);
        } else {
            steps.at_most(FormulaOp::Cap, self.active_rate);
        }
        if self.uptime < 1.0 {
            steps.set(FormulaOp::ActiveStacks, self.uptime, self.rate);
        }
        steps.finish(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Modifier;

    fn storm(interval: f64, lockout: f64, duration: f64, limit: f64) -> ModDb {
        let mut db = ModDb::new();
        db.add_list([
            Modifier::number(INTERVAL, ModType::Base, interval),
            Modifier::number(super::super::rain::HIT_INTERVAL, ModType::Base, lockout),
            Modifier::number(DURATION, ModType::Base, duration),
            Modifier::number(LIMIT, ModType::Base, limit),
        ]);
        db
    }

    #[test]
    fn duration_limits_uptime_but_cast_speed_does_not_accelerate_pulses() {
        let mut db = storm(1400.0, 0.0, 10000.0, 1.0);
        let cfg = CalcConfig::spell();
        let rate = |db: &ModDb, uses| periodic_hits(db, &cfg, uses).unwrap().rate;
        assert!((rate(&db, 1.25) - 1.0 / 1.4).abs() < 1e-9);
        assert!((rate(&db, 0.05) - 0.5 / 1.4).abs() < 1e-9);
        db.add_list([Modifier::number("Duration", ModType::Inc, 100.0)]);
        assert!((rate(&db, 0.05) - 1.0 / 1.4).abs() < 1e-9);
        db.add_list([Modifier::number("Duration", ModType::More, -100.0)]);
        assert_eq!(rate(&db, 1.25), 0.0);
    }

    #[test]
    fn storms_share_rain_landings_and_the_enemy_hit_limit() {
        let mut db = storm(100.0, 150.0, 6000.0, 3.0);
        db.add_list([Modifier::flag(EVEN_LANDINGS)]);
        let cfg = CalcConfig::default();
        assert_eq!(periodic_hits(&db, &cfg, 2.0).unwrap().rate, 5.0);
        let reference = cfg.with_rain_hits_whenever_allowed(true);
        assert!((periodic_hits(&db, &reference, 2.0).unwrap().rate - 1.0 / 0.15).abs() < 1e-9);
        // Thunderstorm has variable intervals: cap its potential rate, rather
        // than rounding to an invented fixed 350 ms landing schedule.
        let thunder = storm(350.0, 500.0, 12000.0, 1.0);
        assert_eq!(periodic_hits(&thunder, &reference, 1.0).unwrap().rate, 2.0);
    }

    #[test]
    fn damage_trace_and_formula_use_periodic_hits_not_casts() {
        use crate::calc::offence::{MinimalInput, calculate_minimal, calculate_minimal_traced};
        let db = storm(1400.0, 0.0, 10000.0, 1.0);
        let cfg = CalcConfig::spell();
        let input = MinimalInput {
            base_action_rate: 1.25,
            base_hit_min: 100.0,
            base_hit_max: 100.0,
            ..Default::default()
        };
        let output = calculate_minimal(&db, &cfg, &input);
        let traced = calculate_minimal_traced(&db, &cfg, &input);
        assert_eq!(traced.output.dps, output.dps);
        assert!((output.action_rate - 1.0 / 1.4).abs() < 1e-6);
        let hits = periodic_hits(&db, &cfg, 1.25).unwrap();
        assert_eq!(hits.formula(output.action_rate).value, output.action_rate);
        let thunder = periodic_hits(&storm(350.0, 500.0, 12000.0, 1.0), &cfg, 1.0).unwrap();
        assert_eq!(thunder.formula(2.0).steps[1].op, FormulaOp::Cap);
    }
}
