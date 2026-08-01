//! Aggregate layer — "combine same-name modifiers into one number".
//!
//! The aggregation stop in the modifier lifecycle narrative (see the overview
//! in the crate root `lib.rs`): [`mod_db`]'s [`ModDb`](mod_db::ModDb) indexes
//! modifiers by [`ModName`](pobr_data::modifier::ModName) and exposes the query
//! primitives `sum` (adds Base/Inc), `more` (multiplies `Π(1+v/100)`), `flag`,
//! `override_`, and `list`, plus traced variants that produce a
//! [`TraceGraph`](crate::TraceGraph) for attribution. The standard stat
//! pipeline is `(base + Σbase) × (1 + Σinc/100) × Π(1 + more/100)`. A faithful
//! port of PoB2's `ModDB.lua` / `ModList.lua`.

pub mod mod_db;
