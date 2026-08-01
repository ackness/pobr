//! # pobr-core — modifier calculation core
//!
//! This crate's spine is the **modifier**: item affixes, passives, gems, auras,
//! config toggles — everything in PoE2 gets translated into the same
//! [`Modifier`] data structure, dropped into [`ModDb`], then aggregated into
//! panel numbers by the calc layer via `sum`/`more` queries. The source layout
//! mirrors "the life of a modifier":
//!
//! ```text
//! model/      what a modifier is          —— Modifier + ModTag + eval context CalcConfig
//!   ↑ produced by the two layers below
//! parse/      free text → modifier        —— mod_parser engine / apply_range / mod_cache
//! rules/      curated rule data → modifier —— stat_map / config_options / special / buff interpreter
//! ingest/     source object → modifier    —— item / passive / skill / character / campaign (SourceId-attributed)
//!   ↓ injected into
//! aggregate/  modifier store + queries    —— ModDb: sum / more / flag / override / list
//!   ↓ consumed by
//! calc/       consumes modifiers, computes panels —— offence / defence / ailment / ehp / perform / trigger …
//!   ↓ traced back by
//! attribute/  every output traced to its modifiers —— TraceGraph + AttributionReport (PoBR's core value-add over PoB)
//! ```
//!
//! Calculations use only stable IDs internally
//! ([`ModName`](pobr_data::modifier::ModName) / `StatId` / `SourceId`); display
//! text goes through `pobr-i18n`. Mutable writes to `Env` in calc functions are
//! concentrated in `perform`, with parallelism only unrolled over read-only
//! snapshots (immutable / deterministic).

// Diagnostic env-var snapshot macro. `#[macro_use]` must precede the other mod
// declarations so every layer in this crate can use `dbg_env!` directly;
// `#[macro_export]` also lets pobr-build reuse it (see the dbg_env.rs module doc).
#[macro_use]
mod dbg_env;

// Modifier layers (the directory layout is the narrative)
pub mod aggregate;
pub mod attribute;
pub mod calc;
pub mod ingest;
pub mod model;
pub mod parse;
pub mod rules;

pub mod display_catalog;

// Backward-compat aliases: keep the pre-reorg `pobr_core::<module>::` /
// `crate::<module>::` paths resolving. Downstream crates / integration tests /
// benches heavily reference the old flat module paths (modifier::, mod_db::,
// mod_parser::, etc.); these root re-exports let them keep working unchanged
// while the new layered paths (model::modifier::, etc.) also stay reachable.
// `calc` / `rules` were never moved — they're already at the root, so no alias
// is needed for them.
pub use aggregate::mod_db;
pub use attribute::{attribution, trace};
pub use ingest::{campaign, character, item, item_text, passive, skill_source};
pub use model::{config, modifier};
pub use parse::{apply_range, mod_cache, mod_parser};

// Public API re-exports (names match the pre-reorg layout; paths point into the new layers)
pub use aggregate::mod_db::{HighPrecisionRules, ModContribution, ModDb, ModList};
pub use attribute::attribution::{
    AttributionEntry, AttributionGroup, AttributionMode, AttributionReport, AttributionRequest,
    attribute,
};
pub use attribute::trace::{
    CombineMode, CritTag, HandTag, PassId, TraceEdge, TraceGraph, TraceNode, TraceNodeId,
    TraceOperation, TraceOutput, TracedValue,
};
pub use display_catalog::{display_catalog, extract_display_values};
pub use ingest::campaign::{CampaignProgress, CampaignReward, CampaignState};
pub use ingest::character::CharacterBase;
pub use ingest::item::{
    ItemIngest, ItemModSection, apply_weapon_hand_conditions, ingest_item_with_ctx,
};
pub use ingest::item_text::{ItemTextError, parse_item_text, parse_pob_xml_item};
pub use ingest::passive::{AllocatedNode, PassiveIngest, ingest_passive_nodes_with_ctx};
pub use ingest::skill_source::{
    ActiveSkillJudgeInput, ActiveSkillSpec, GemIngest, GemModSource, SkillGatingError,
    SupportGemSpec, SupportIngestError, SupportJudgeInput, can_support, ingest_active_gem_with_ctx,
    ingest_gem_leveled, ingest_gem_with_ctx, ingest_support_gem_with_ctx, judge_support,
};
pub use model::config::{CalcConfig, EvalContext, StatLookup};
pub use model::modifier::{ActorRef, ModTag, ModValue, Modifier};
pub use rules::{
    DefenceKeystones, DuplicateHandlerError, Handler, HandlerCtx, HandlerOutcome, HandlerRegistry,
    MainSkillCtx,
};
