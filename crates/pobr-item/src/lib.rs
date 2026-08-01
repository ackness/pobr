//! pobr-item: **full-fidelity edit-view** parsing and reverse serialization
//! for raw item text.
//!
//! See target design in `devs/docs/architecture/02-crate-design.md` §6 and
//! `audits/rearchitecture-2026-06-10/blueprints/m5c-item-tree.md` Track A.
//!
//! Responsibility boundary:
//! - The **calc view** (strip annotations, gate variants, resolve ranges,
//!   then feed the calc engine) is still owned by `pobr-core::item_text` +
//!   `pobr-core::item::ingest_item`.
//! - The **edit view** (preserves the variant name list / per-line
//!   annotations / unmodeled annotations that the calc view deliberately
//!   discards) is owned by this crate's [`ItemDraft`], and supports BuildRaw round-tripping.
//!
//! Reuses `pobr-core::mod_parser` to parse English modifier text, avoiding
//! duplicated rules at the item layer; rule functions (variant gating /
//! range resolution) will eventually get a shared entry point from
//! pobr-core, with this crate only orchestrating them.

pub mod annotations;
pub mod build_raw;
pub mod draft;
pub mod tier;

pub use annotations::{ModLineAnnotations, parse_mod_line, round_to};
pub use draft::{
    CatalystState, DisplayLine, DisplayLineKind, DraftError, DraftHeader, ItemDraft, ItemStates,
    LineBucket, ModLineDraft, VariantState, classify_display_lines,
};
pub use tier::{TierIndex, TierInfo};
