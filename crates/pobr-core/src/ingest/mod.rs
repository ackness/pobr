//! Source ingest layer — "source object → modifier".
//!
//! One of layer 2's stops in the modifier lifecycle narrative (see the
//! overview in the crate root `lib.rs`): converts the game's various source
//! objects into [`Modifier`](crate::Modifier)s attributed with a
//! [`SourceId`](pobr_data::source::SourceId), and injects them into
//! [`ModDb`](crate::ModDb). The SourceId lets aggregated results be traced back
//! through the [`attribute`](crate::attribute) layer to "which item / which
//! passive / which gem contributed this".
//! - [`item`] / [`item_text`]: equipment (including the flask/charm modifier
//!   branch); raw text → calc view.
//! - [`passive`]: mod collection from allocated passive tree nodes.
//! - [`skill_source`]: active / support gems → skill modifiers and base damage
//!   values.
//! - [`character`]: innate base values derived from class / level / attributes.
//! - [`campaign`]: campaign progress penalties and permanent rewards.
//!
//! Mirrors the source assembly in PoB2's `Modules/CalcSetup.lua`.

pub mod campaign;
pub mod character;
pub mod item;
pub mod item_text;
pub mod passive;
pub mod skill_source;
