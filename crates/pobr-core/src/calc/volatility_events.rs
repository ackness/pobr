//! Restore expectation for an explicitly supplied Volatility explosion.
//!
//! Game `volatility_refresh_%_chance` / passive 43944 restores the equivalent
//! exploded amount on success. KeywordPopups.Volatility (poe2db.tw/us/Volatility)
//! sets a default 200 stack cap; `maximum_volatility_allowed` replaces it.
//! Neither the resetting explosion timer nor restored stacks schedule an event.
use pobr_data::prelude::*;

use super::{Env, session::detail::DerivedDetail, utility::query_config};

pub const QUANTITY: &str = "VolatilityExplosionQuantity";
pub const RATE: &str = "VolatilityExplosionsPerSecond";
pub const STATS: &[&str] = &[
    "VolatilityRefreshChance",
    "VolatilityExplosionQuantity",
    "VolatilityRestoredPerExplosion",
    "VolatilityRestoredPerSecond",
];

#[derive(Debug, Clone, PartialEq, Default)]
pub struct VolatilityEvents {
    pub chance: f64,
    pub quantity: f64,
    pub restored_per_explosion: f64,
    pub restored_per_second: f64,
    pub raw_chance: f64,
    pub requested_quantity: f64,
    pub maximum: f64,
    pub explosion_rate: f64,
}

impl VolatilityEvents {
    pub fn value(&self, id: &str) -> Option<f64> {
        Some(match id {
            "VolatilityRefreshChance" => self.chance,
            "VolatilityExplosionQuantity" => self.quantity,
            "VolatilityRestoredPerExplosion" => self.restored_per_explosion,
            "VolatilityRestoredPerSecond" => self.restored_per_second,
            _ => return None,
        })
    }

    pub fn detail(&self, id: &str) -> Option<DerivedDetail> {
        let (formula, inputs) = match id {
            "VolatilityRefreshChance" => (
                "clamp(VolatilityRefreshChanceBase, 0, 100)",
                vec![("VolatilityRefreshChanceBase", self.raw_chance)],
            ),
            "VolatilityExplosionQuantity" => (
                "min(VolatilityRequestedQuantity, VolatilityMaximum)",
                vec![
                    ("VolatilityRequestedQuantity", self.requested_quantity),
                    ("VolatilityMaximum", self.maximum),
                ],
            ),
            "VolatilityRestoredPerExplosion" => (
                "VolatilityExplosionQuantity * VolatilityRefreshChance / 100",
                vec![
                    ("VolatilityExplosionQuantity", self.quantity),
                    ("VolatilityRefreshChance", self.chance),
                ],
            ),
            "VolatilityRestoredPerSecond" => (
                "VolatilityRestoredPerExplosion * VolatilityExplosionsPerSecond",
                vec![
                    (
                        "VolatilityRestoredPerExplosion",
                        self.restored_per_explosion,
                    ),
                    ("VolatilityExplosionsPerSecond", self.explosion_rate),
                ],
            ),
            _ => return None,
        };
        Some(DerivedDetail {
            formula: formula.into(),
            inputs: inputs
                .into_iter()
                .map(|(id, value)| (id.into(), value))
                .collect(),
            context: vec![],
        })
    }
}

pub(super) fn calculate(env: &mut Env) {
    // Both absent controls must be discoverable. Invalid/zero snapshots are off.
    let quantity = env.cfg.multiplier_if_set(QUANTITY).unwrap_or(0.0);
    let rate = env.cfg.multiplier_if_set(RATE).unwrap_or(0.0);
    env.player.output.volatility_events = VolatilityEvents::default();
    if env.player.minion.is_some() {
        return;
    }
    let cfg = query_config(&env.cfg);
    let db = &env.player.mod_db;
    // A configured explosion cannot fabricate a source or consume cap modifiers
    // when this actor has no refresh mechanic. `named` also respects basis gates.
    if !db
        .named("VolatilityRefreshChance")
        .any(|m| m.mod_type == ModType::Base && m.value.as_number().is_some() && m.matches(&cfg))
    {
        return;
    }
    let raw_chance = db.sum(
        ModType::Base,
        &cfg,
        &[ModName::from("VolatilityRefreshChance")],
    );
    if !raw_chance.is_finite() {
        return;
    }
    let raw_maximum = db.override_(&cfg, "VolatilityMaximum").unwrap_or(200.0);
    let valid_maximum = raw_maximum.is_finite() && raw_maximum >= 0.0;
    let maximum = if valid_maximum {
        raw_maximum.floor()
    } else {
        0.0
    };
    let chance = raw_chance.clamp(0.0, 100.0);
    // Source properties remain useful in default/off builds; reading these
    // actual properties does not establish any held stacks or explosion event.
    env.player.output.volatility_events = VolatilityEvents {
        chance,
        raw_chance,
        maximum,
        ..Default::default()
    };
    if !env.cfg.mode_combat
        || !quantity.is_finite()
        || quantity <= 0.0
        || quantity.fract() != 0.0
        || !valid_maximum
    {
        return;
    }
    let clamped_quantity = quantity.min(maximum);
    let restored_per_explosion = clamped_quantity * (chance / 100.0);
    // A supplied rate describes eligible explosions of this same quantity.
    // Reject overflow as unsupported input instead of returning infinity.
    let explosion_rate =
        if rate.is_finite() && rate >= 0.0 && (restored_per_explosion * rate).is_finite() {
            rate
        } else {
            0.0
        };
    env.player.output.volatility_events = VolatilityEvents {
        chance,
        quantity: clamped_quantity,
        restored_per_explosion,
        restored_per_second: restored_per_explosion * explosion_rate,
        raw_chance,
        requested_quantity: quantity,
        maximum,
        explosion_rate,
    };
}
