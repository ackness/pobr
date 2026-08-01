//! The parsing layer — "free text -> modifiers".
//!
//! One of the two layers in the modifier-lifecycle narrative (see the
//! overview in the crate root `lib.rs`): translates human-readable modifier
//! text (e.g. `"25% increased Fire Damage"`) into [`Modifier`](crate::Modifier).
//! - [`mod_parser`]: the data-driven scan engine (consumes
//!   `overlay/mod_parser_rules.json`, replicating PoB2 `ModParser.lua`'s
//!   `scan()` + `parseMod()`) plus the legacy hand-written parser pending
//!   removal.
//! - [`apply_range`]: resolves range-bearing modifiers like `+(40-50) to
//!   maximum Life` to a single-value string via `range` (0..1) before
//!   feeding them to the parser (mirrors PoB2 `ItemTools.lua::applyRange`).
//! - [`mod_cache`]: a `text -> Vec<Modifier>` parse cache (zero repeated
//!   parsing on hot paths).
//!
//! Division of labor with [`rules`](crate::rules): this layer handles
//! **free text**, while `rules` handles **curated rule data** (JSON rule
//! tables + handlers); both produce modifiers, but from different input
//! shapes.

pub mod apply_range;
pub mod mod_cache;
pub mod mod_parser;
