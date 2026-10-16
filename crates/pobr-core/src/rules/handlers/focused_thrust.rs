//! Distance-gated game text uses a captured radius as an enemy-distance threshold.
//! Template tags only accept literal thresholds; the game text supplies metres
//! while the existing enemyDistance config uses ten units per metre.

use pobr_data::modifier::{KeywordFlags, ModFlags, ModType};

use crate::modifier::{ModTag, Modifier};
use crate::rules::{DuplicateHandlerError, HandlerCtx, HandlerOutcome, HandlerRegistry};

pub fn register(registry: &mut HandlerRegistry) -> Result<(), DuplicateHandlerError> {
    registry.register("special:focused_thrust", Box::new(focused_thrust))?;
    registry.register("special:mauling_stuns", Box::new(mauling_stuns))
}

fn focused_thrust(ctx: &HandlerCtx<'_>) -> HandlerOutcome {
    let [damage, metres] = ctx.inputs else {
        return HandlerOutcome::default();
    };
    HandlerOutcome::player_mods(vec![
        Modifier::number("Damage", ModType::Inc, *damage)
            .with_flags(ModFlags::PROJECTILE | ModFlags::SPEAR)
            .with_keyword_flags(KeywordFlags::HIT)
            .with_tag(distance_gate(*metres, false)),
    ])
}

fn distance_gate(metres: f64, upper: bool) -> ModTag {
    ModTag::MultiplierThreshold {
        var: "enemyDistance".into(),
        threshold: metres * 10.0,
        upper,
    }
}

fn mauling_stuns(ctx: &HandlerCtx<'_>) -> HandlerOutcome {
    let [buildup, metres] = ctx.inputs else {
        return HandlerOutcome::default();
    };
    HandlerOutcome::player_mods(vec![
        Modifier::number("EnemyHeavyStunBuildup", ModType::Inc, *buildup)
            .with_tag(distance_gate(*metres, true)),
    ])
}
