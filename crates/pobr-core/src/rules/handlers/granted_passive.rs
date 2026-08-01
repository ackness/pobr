//! `special:granted_passive` handler — equivalent to vendor
//! `["allocates (.+)"]` -> `mod("GrantedPassive", "LIST", passive)`
//! (ModParser.lua:5809).
//!
//! Anoint enchants like "Allocates <Notable>": the open-captured passive
//! name becomes `GrantedPassive` LIST Text(name); the orchestration layer's
//! `append_granted_passives` matches it by name against Notable nodes and
//! appends it as an AllocatedNode (CalcSetup.lua:1322-1331 notableMap).
//!
//! **Handled via a handler rather than a template**: the DSL's hard boundary
//! bans open `(.+)` captures (`special_mods_gate`'s
//! `no_open_captures_in_patterns`) — any entry with an open capture must go
//! through `handler_id`. The text name is passed through via
//! [`HandlerCtx::raw_captures`] (numeric `inputs` can't carry a name).
//!
//! Byte-for-byte aligned with legacy (mod_parser legacy.rs:1067): the name
//! is the trimmed capture text (special-channel input is already
//! lowercase-normalized). The conditional form "allocates X if you have the
//! matching modifier on forbidden Y" (ModParser.lua:5808) is classified
//! Unsupported by legacy — this handler produces empty mods for that form
//! instead (an empty-mods special-channel result means "recognized but
//! produced nothing"; this form isn't in the C1 corpus, so the discrepancy
//! is noted here rather than resolved).

use pobr_data::modifier::ModType;

use crate::modifier::{ModValue, Modifier};
use crate::rules::registry::{DuplicateHandlerError, HandlerCtx, HandlerOutcome, HandlerRegistry};

/// The handler's stable id.
pub const ID: &str = "special:granted_passive";

/// Registers the granted_passive handler.
pub fn register(registry: &mut HandlerRegistry) -> Result<(), DuplicateHandlerError> {
    registry.register(ID, Box::new(granted_passive_handler))
}

fn granted_passive_handler(ctx: &HandlerCtx<'_>) -> HandlerOutcome {
    let Some(name) = ctx.raw_captures.first() else {
        return HandlerOutcome::default();
    };
    let name = name.trim();
    // The conditional-grant form (forbidden flame/flesh) isn't modeled ->
    // don't mistake it for an unconditional grant (matching legacy's
    // conservative behavior).
    if name.is_empty() || name.contains("if you have the matching modifier") {
        return HandlerOutcome::default();
    }
    HandlerOutcome::player_mods(vec![Modifier::new(
        "GrantedPassive",
        ModType::List,
        ModValue::Text(name.to_string()),
    )])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produces_granted_passive_list() {
        let caps = vec!["killer instinct".to_string()];
        let outcome = granted_passive_handler(&HandlerCtx::with_inputs_and_captures(&[], &caps));
        assert_eq!(outcome.player_mods.len(), 1);
        assert_eq!(outcome.player_mods[0].name.as_str(), "GrantedPassive");
        assert_eq!(outcome.player_mods[0].mod_type, ModType::List);
        assert_eq!(
            outcome.player_mods[0].value,
            ModValue::Text("killer instinct".to_string())
        );
    }

    #[test]
    fn conditional_form_yields_nothing() {
        let caps = vec!["x if you have the matching modifier on forbidden flame".to_string()];
        let outcome = granted_passive_handler(&HandlerCtx::with_inputs_and_captures(&[], &caps));
        assert!(outcome.player_mods.is_empty());
    }
}
