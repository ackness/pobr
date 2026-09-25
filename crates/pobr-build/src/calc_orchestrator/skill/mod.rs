//! skill — the engine-semantics layer: given a socket group + data tables,
//! produce the modifiers to inject (vendor `CalcActiveSkill` counterpart).
//!
//! Submodules:
//! - [`resolve`]: main-skill selection, gem level/quality bonuses, skill-name derivation;
//! - [`minions`]: summoning-gem recognition and minion wiring;
//! - [`mods`]: per-level stat-set → Modifier mapping (base/cost/cooldown/DoT/corpse
//!   explosion/crossbow reload/quality/unselected sets/supports);
//! - [`triggers`]: trigger-chain recognition and injection;
//! - [`buffs`]: aura/curse/buff/warcry specs and spirit reservation.

pub mod buffs;
pub mod minions;
pub mod mods;
pub mod resolve;
pub mod triggers;
