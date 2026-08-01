//! Special-modifier handler implementations.
//!
//! Special entries whose real logic can't be expressed by the restricted
//! template DSL ([`crate::rules::special_mod`]) carry only a stable
//! `handler_id` (named `special:<name>`) in `overlay/special_mods.json`; at
//! runtime a [`HandlerRegistry`] handler registered in this directory
//! decides what to do.
//!
//! **DSL hard-boundary monitoring** (20-target-architecture §5): total
//! handler count must stay under 100, and under 10% of all special entries
//! (enforced by the gate test `special_mods_gate.rs`). Approaching that
//! ceiling signals a data-split failure — anything that can be templated
//! must go through the DSL; handlers are reserved for three kinds of real
//! logic: conditional branching, cross-domain LIST payloads, and PoB2
//! closure constructors.
//!
//! The registration aggregation point is [`register_special_handlers`]:
//! append-only, where each handler module exposes its own `register_*`
//! function and this file appends a call per line (minimizing shared-file
//! conflicts).
//!
//! [`HandlerRegistry`]: crate::rules::HandlerRegistry

use crate::rules::{DuplicateHandlerError, HandlerRegistry};

mod explode;
mod granted_passive;
mod mageblood;

/// Registers all special handlers (once at startup, zero I/O).
///
/// handler_id follows the naming `special:<name>` (`<name>` reuses the
/// snake_case of the matching vendor ModParser.lua constructor name). A
/// registration conflict (duplicate id) -> `Err` (fail-fast).
pub fn register_special_handlers(
    registry: &mut HandlerRegistry,
) -> Result<(), DuplicateHandlerError> {
    explode::register(registry)?;
    granted_passive::register(registry)?;
    mageblood::register(registry)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_without_conflict() {
        let mut registry = HandlerRegistry::new();
        register_special_handlers(&mut registry).expect("special handler 注册不冲突");
    }
}
