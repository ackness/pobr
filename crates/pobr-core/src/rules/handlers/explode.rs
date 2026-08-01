//! `special:explode_on_kill` handler — equivalent to vendor `explodeFunc`
//! (ModParser.lua:2217-2230).
//!
//! Vendor produces `mod("ExplodeMod", "LIST", { type, value=chance, amount,
//! ... })` plus `flag("CanExplode")`. The PoBR calc side **has no enemy
//! explosion consumer yet** — per the C-3 convention, this handler produces
//! the same marker mods as PoB2 (their value lands in the ModDb; the
//! consumption gap is tracked separately, not filled in here):
//! `CanExplode` FLAG (matches vendor's `flag("CanExplode")`),
//! `EnemyExplodeChance` BASE from `$1` (the explosion trigger chance,
//! carrying vendor's `value=chance`), and `EnemyExplodeAmount` BASE from
//! `$2` (percentage of max life, carrying vendor's `amount`).
//!
//! handler_args convention (in the special_mods.json entry): `["$1", "$2"]`
//! = (chance, amount). The element type (vendor `type`) can't be expressed
//! by DSL enums for the explosion LIST payload, so the whole entry goes
//! through a handler — but since the element dimension has no consumer yet,
//! this handler doesn't break it out further (conservative, pending a calc
//! explosion channel).

use pobr_data::modifier::ModType;

use crate::modifier::Modifier;
use crate::rules::registry::{DuplicateHandlerError, HandlerCtx, HandlerOutcome, HandlerRegistry};

/// The handler's stable id.
pub const ID: &str = "special:explode_on_kill";

/// Registers the explode handler.
pub fn register(registry: &mut HandlerRegistry) -> Result<(), DuplicateHandlerError> {
    registry.register(ID, Box::new(explode_handler))
}

fn explode_handler(ctx: &HandlerCtx<'_>) -> HandlerOutcome {
    let chance = ctx.inputs.first().copied().unwrap_or(0.0);
    let amount = ctx.inputs.get(1).copied().unwrap_or(0.0);
    HandlerOutcome::player_mods(vec![
        Modifier::flag("CanExplode"),
        Modifier::number("EnemyExplodeChance", ModType::Base, chance),
        Modifier::number("EnemyExplodeAmount", ModType::Base, amount),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explode_produces_flag_and_markers() {
        let ctx = HandlerCtx::with_inputs(&[10.0, 5.0]);
        let outcome = explode_handler(&ctx);
        assert_eq!(outcome.player_mods.len(), 3);
        assert_eq!(outcome.player_mods[0].name.as_str(), "CanExplode");
        assert_eq!(outcome.player_mods[1].name.as_str(), "EnemyExplodeChance");
        assert_eq!(outcome.player_mods[1].value.as_number(), Some(10.0));
        assert_eq!(outcome.player_mods[2].value.as_number(), Some(5.0));
    }
}
