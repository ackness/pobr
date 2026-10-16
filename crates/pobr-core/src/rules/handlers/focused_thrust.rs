//! Focused Thrust's captured radius becomes an enemy-distance threshold.
//! Template tags only accept literal thresholds; the game text supplies metres
//! while the existing enemyDistance config uses ten units per metre.

use pobr_data::modifier::{KeywordFlags, ModFlags, ModType};

use crate::modifier::{ModTag, Modifier};
use crate::rules::{DuplicateHandlerError, HandlerCtx, HandlerOutcome, HandlerRegistry};

pub fn register(registry: &mut HandlerRegistry) -> Result<(), DuplicateHandlerError> {
    registry.register("special:focused_thrust", Box::new(focused_thrust))
}

fn focused_thrust(ctx: &HandlerCtx<'_>) -> HandlerOutcome {
    let [damage, metres] = ctx.inputs else {
        return HandlerOutcome::default();
    };
    HandlerOutcome::player_mods(vec![
        Modifier::number("Damage", ModType::Inc, *damage)
            .with_flags(ModFlags::PROJECTILE | ModFlags::SPEAR)
            .with_keyword_flags(KeywordFlags::HIT)
            .with_tag(ModTag::MultiplierThreshold {
                var: "enemyDistance".into(),
                threshold: metres * 10.0,
                upper: false,
            }),
    ])
}
