//! Modifier core layer — "what a modifier is".
//!
//! Layer 1 of the modifier lifecycle narrative (see the overview in the crate
//! root `lib.rs`):
//! - [`modifier`]: the modifier data type [`Modifier`](modifier::Modifier)
//!   (`{name, type, value, flags, keyword_flags, tags, source, origin}`), its
//!   tag system [`ModTag`](modifier::ModTag) (Condition / Multiplier / PerStat /
//!   …), and the evaluation entry points `matches` / `effective_number`. A
//!   faithful port of PoB2's `Mod` in `mod.lua` and `ModStore.lua::EvalMod`.
//! - [`config`]: the evaluation context for a modifier —
//!   [`CalcConfig`](config::CalcConfig) / [`EvalContext`](config::EvalContext) —
//!   flags / conditions / multipliers / damage_type etc. that decide whether and
//!   how strongly a modifier applies in the current situation.

pub mod config;
pub mod modifier;
