//! `special:mageblood_legacy` handler — equivalent to vendor
//! `ModParser.lua:5554-5557`'s `["legacy of (%w+)"]` closure.
//!
//! Each chosen variant on a Mageblood belt renders as a line `Legacy of
//! <X>`; vendor builds the mod name dynamically from the capture group,
//! `mod("LegacyOf"..firstToUpper(flask), "BASE", 1)`, plus
//! `flag("MagebloodEquipped")`. A capture-driven mod name can't be expressed
//! in the restricted template DSL (same precedent as
//! `special:granted_passive` — DSL names only support Literal / a closed
//! enum set), so this goes through a handler: it takes the flask name from
//! [`HandlerCtx::raw_captures`], capitalizes its first letter, and builds
//! `LegacyOf<X>`.
//!
//! Aggregate application (folding `LegacyOf*` stacks into armour/evasion/
//! resistances) lives on the calc side, in
//! [`crate::calc::mageblood`] (vendor `CalcPerform.lua:1502-1528`).

use pobr_data::modifier::ModType;

use crate::modifier::Modifier;
use crate::rules::registry::{DuplicateHandlerError, HandlerCtx, HandlerOutcome, HandlerRegistry};

/// The handler's stable id.
pub const ID: &str = "special:mageblood_legacy";

/// Registers the mageblood_legacy handler.
pub fn register(registry: &mut HandlerRegistry) -> Result<(), DuplicateHandlerError> {
    registry.register(ID, Box::new(mageblood_legacy_handler))
}

/// Lua's `firstToUpper`: capitalizes the first letter, leaves the rest unchanged.
fn first_to_upper(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn mageblood_legacy_handler(ctx: &HandlerCtx<'_>) -> HandlerOutcome {
    let Some(flask) = ctx.raw_captures.first() else {
        return HandlerOutcome::default();
    };
    let flask = flask.trim();
    if flask.is_empty() {
        return HandlerOutcome::default();
    }
    let legacy_name = format!("LegacyOf{}", first_to_upper(flask));
    HandlerOutcome::player_mods(vec![
        Modifier::number(legacy_name, ModType::Base, 1.0),
        Modifier::flag("MagebloodEquipped"),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produces_legacy_stack_and_flag() {
        // Special-channel input is already lowercase ("legacy of granite" -> captures "granite").
        let caps = vec!["granite".to_string()];
        let out = mageblood_legacy_handler(&HandlerCtx::with_inputs_and_captures(&[], &caps));
        assert_eq!(out.player_mods.len(), 2);
        assert_eq!(out.player_mods[0].name.as_str(), "LegacyOfGranite");
        assert_eq!(out.player_mods[0].mod_type, ModType::Base);
        assert_eq!(out.player_mods[0].value.as_number(), Some(1.0));
        assert_eq!(out.player_mods[1].name.as_str(), "MagebloodEquipped");
        assert_eq!(out.player_mods[1].mod_type, ModType::Flag);
    }

    #[test]
    fn empty_capture_yields_nothing() {
        let out = mageblood_legacy_handler(&HandlerCtx::with_inputs_and_captures(&[], &[]));
        assert!(out.player_mods.is_empty());
    }
}
